use crate::extract::{extract, window};
use crate::frames::{data_user, prose_user, request_body, request_hash, MAX_TOKENS_DATA, MAX_TOKENS_PROSE, SYSTEM, SYSTEM_PROSE};
use crate::outcome::{self, Ans};
use crate::sig;
use crate::spec::*;
use crate::util::{b64, canon_number, parse_reply, quote_in, sha256, sha256_hex};
use serde::Serialize;

pub trait Fetcher {
    fn get(&self, url: &str, max_bytes: usize, timeout_s: u64) -> Result<(u16, Vec<u8>), String>;
}

pub struct JudgeReply {
    /// exact response bytes
    pub resp: Vec<u8>,
    /// the assistant's words as the provider parsed them (the program reads them again from `resp`)
    pub content: String,
    /// signature fields as returned by the provider
    pub text: String,
    pub sig: Vec<u8>,
    /// the signer's ed25519 public key
    pub key: [u8; 32],
    pub kind: String,
}

pub trait Judge {
    fn ask(&mut self, idx: usize, model: &str, req: &[u8], timeout_s: u64) -> Result<JudgeReply, String>;
}

#[derive(Serialize)]
struct SrcOut {
    i: u32,
    c: u16,
    h: String,
    x: String,
}

#[derive(Serialize)]
struct JudgeOut {
    m: usize,
    /// the answer that counts: T, F, U, or X when the judge gave no signed reply
    a: &'static str,
    /// what the judge actually said (T, F or U), before the quote check
    #[serde(skip_serializing_if = "String::is_empty")]
    v: String,
    /// the sentence the judge copied from the excerpt
    #[serde(skip_serializing_if = "String::is_empty")]
    q: String,
    /// true when the judge said TRUE or FALSE but its quote is not on the page, so the answer was turned into U
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    qx: bool,
    /// the judge's exact response bytes, base64url. The signature covers a hash of these.
    #[serde(skip_serializing_if = "String::is_empty")]
    r: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    g: String,
    /// the key that signed, base64url
    #[serde(skip_serializing_if = "String::is_empty")]
    k: String,
}

#[derive(Serialize)]
struct Verdict {
    v: u32,
    rd: u64,
    ih: String,
    o: &'static str,
    rc: u8,
    t: u64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    w: bool,
    /// the excerpt the judges read (prose only). Judges' quotes are pieces of it.
    #[serde(skip_serializing_if = "String::is_empty")]
    ex: String,
    s: Vec<SrcOut>,
    j: Vec<JudgeOut>,
}

fn finish(mut v: Verdict) -> String {
    let mut s = serde_json::to_string(&v).unwrap();
    if s.len() > OUT_MAX {
        // Keep the bounded fields, drop detail. The verdict stays valid but says it was cut.
        v.s.clear();
        v.j.clear();
        v.ex.clear();
        v.o = "X";
        v.rc = RC_SIZE;
        s = serde_json::to_string(&v).unwrap();
    }
    s
}

fn bad(rd: u64, ih: String, t: u64, rc: u8) -> String {
    finish(Verdict { v: VERSION, rd, ih, o: "X", rc, t, w: false, ex: String::new(), s: vec![], j: vec![] })
}

fn host_ok(url: &str) -> bool {
    url.starts_with("https://") && !url.contains('@')
}

/// The words inside a provider response: choices[0].message.content.
pub fn content_of(resp: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(resp).ok()?;
    v["choices"][0]["message"]["content"].as_str().map(|s| s.to_string())
}

pub fn run(
    input_bytes: &[u8],
    fetcher: &dyn Fetcher,
    judge: &mut dyn Judge,
    elapsed: &dyn Fn() -> u64,
    now_unix: u64,
) -> String {
    let ih = b64(&sha256(input_bytes));
    let inp: Input = match serde_json::from_slice(input_bytes) {
        Ok(i) => i,
        Err(_) => return bad(0, ih, now_unix, RC_BAD_INPUT),
    };
    let rd = inp.rd;
    if inp.v != VERSION
        || sha256_hex(inp.claim.as_bytes()) != inp.th
        || !(inp.m.len() == 3 || inp.m.len() == 5)
        || inp.src.len() < 2
        || !(inp.kind == "data" || inp.kind == "prose")
        || (inp.kind == "data" && inp.cv.is_none())
        || inp.src.iter().any(|s| !host_ok(&s.u))
    {
        return bad(rd, ih, now_unix, RC_BAD_INPUT);
    }
    let mut groups: Vec<&str> = inp.src.iter().map(|s| s.g.as_str()).collect();
    groups.sort();
    groups.dedup();
    if groups.len() < 2 {
        return bad(rd, ih, now_unix, RC_BAD_INPUT);
    }

    // 1. Fetch every source.
    let mut souts: Vec<SrcOut> = vec![];
    let mut bodies: Vec<String> = vec![];
    for s in &inp.src {
        if elapsed() >= SELF_STOP_SECS {
            return bad(rd, ih, now_unix, RC_TIMEOUT);
        }
        match fetcher.get(&s.u, MAX_BYTES, SOURCE_SECS) {
            Ok((200, bytes)) => {
                let text = String::from_utf8_lossy(&bytes).to_string();
                let mut kept = text;
                let mut shown = String::new();
                if inp.kind == "data" {
                    match extract(&kept, &s.x) {
                        Some(v) => {
                            shown = v.clone();
                            kept = v;
                        }
                        None => return bad(rd, ih, now_unix, RC_FETCH),
                    }
                }
                shown.truncate(24);
                souts.push(SrcOut { i: s.i, c: 200, h: b64(&sha256(&bytes)), x: shown });
                bodies.push(kept);
            }
            _ => return bad(rd, ih, now_unix, RC_FETCH),
        }
    }

    // 2. Data sentences: the numbers must agree first. Prose sentences: cut the excerpt the judges will read.
    let mut numbers_true = false;
    let user: String;
    let system: &str;
    let max_tokens: u8;
    let mut excerpt = String::new();
    if inp.kind == "data" {
        let cv = inp.cv.clone().unwrap();
        let nums: Vec<Option<f64>> = bodies.iter().map(|b| b.parse::<f64>().ok()).collect();
        let claimed = cv.parse::<f64>().ok();
        if nums.iter().any(|n| n.is_none()) || claimed.is_none() {
            return bad(rd, ih, now_unix, RC_FETCH);
        }
        let tol = inp.src.iter().map(|s| s.tol).fold(0.0, f64::max);
        let first = nums[0].unwrap();
        if nums.iter().any(|n| (n.unwrap() - first).abs() > tol) {
            return finish(Verdict {
                v: VERSION,
                rd,
                ih,
                o: "V",
                rc: RC_SRC_DISAGREE,
                t: now_unix,
                w: false,
                ex: String::new(),
                s: souts,
                j: vec![],
            });
        }
        numbers_true = (first - claimed.unwrap()).abs() <= tol;
        user = data_user(&inp.claim, &canon_number(claimed.unwrap()));
        system = SYSTEM;
        max_tokens = MAX_TOKENS_DATA;
    } else {
        let mut parts = vec![];
        for b in &bodies {
            if let Some(w) = window(b, &inp.terms, 300) {
                parts.push(w);
            }
        }
        if parts.is_empty() {
            return bad(rd, ih, now_unix, RC_FETCH);
        }
        excerpt = parts.join("\n---\n");
        user = prose_user(&excerpt, &inp.claim);
        system = SYSTEM_PROSE;
        max_tokens = MAX_TOKENS_PROSE;
    }

    // 3. Ask the judges, one after another. Check every signature and, for prose, that every quote is on the page.
    let mut answers = vec![];
    let mut jouts = vec![];
    let mut unsigned = 0;
    for (idx, model) in inp.m.iter().enumerate() {
        if elapsed() >= SELF_STOP_SECS {
            return bad(rd, ih, now_unix, RC_TIMEOUT);
        }
        let req = request_body(model, system, &user, inp.seed, max_tokens);
        let rh = request_hash(&req);
        let none = |a: &'static str| JudgeOut {
            m: idx,
            a,
            v: String::new(),
            q: String::new(),
            qx: false,
            r: String::new(),
            g: String::new(),
            k: String::new(),
        };
        let reply = judge.ask(idx, model, &req, JUDGE_SECS).ok();
        let content = reply.as_ref().and_then(|r| {
            let expect = format!("{}:{}:{}", model, rh, sha256_hex(&r.resp));
            let good = r.kind == "provider_tee" && r.text == expect && sig::verify_ed(&r.text, &r.sig, &r.key);
            if good {
                content_of(&r.resp)
            } else {
                None
            }
        });
        match (reply, content) {
            (Some(r), Some(content)) => {
                let (word, quote) = if inp.kind == "prose" { parse_reply(&content) } else { (content.clone(), String::new()) };
                let said = Ans::parse(&word);
                let mut counted = said;
                let mut qx = false;
                if inp.kind == "prose" && said != Ans::U {
                    let on_page = quote_in(&excerpt, &quote) && bodies.iter().any(|b| quote_in(b, &quote));
                    if !on_page {
                        counted = Ans::U;
                        qx = true;
                    }
                }
                answers.push(counted);
                jouts.push(JudgeOut {
                    m: idx,
                    a: counted.letter(),
                    v: said.letter().to_string(),
                    q: if said == Ans::U { String::new() } else { quote },
                    qx,
                    r: b64(&r.resp),
                    g: b64(&r.sig),
                    k: b64(&r.key),
                });
            }
            _ => {
                unsigned += 1;
                answers.push(Ans::U);
                jouts.push(none("X"));
            }
        }
    }

    let (o, w) = if inp.kind == "data" { outcome::data(&answers, numbers_true) } else { outcome::prose(&answers) };
    let rc = if unsigned > 0 && o == "U" { RC_UNSIGNED } else { RC_OK };
    finish(Verdict { v: VERSION, rd, ih, o, rc, t: now_unix, w, ex: excerpt, s: souts, j: jouts })
}
