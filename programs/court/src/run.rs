use crate::extract::{extract, window};
use crate::frames::{data_user, prose_user, request_body, request_hash};
use crate::outcome::{self, Ans};
use crate::sig;
use crate::spec::*;
use crate::util::{b64, canon_number, sha256, sha256_hex};
use serde::Serialize;

pub trait Fetcher {
    fn get(&self, url: &str, max_bytes: usize, timeout_s: u64) -> Result<(u16, Vec<u8>), String>;
}

pub struct JudgeReply {
    /// exact response bytes
    pub resp: Vec<u8>,
    /// the assistant's words
    pub content: String,
    /// signature fields as returned by the provider
    pub text: String,
    pub sig: Vec<u8>,
    pub addr: [u8; 20],
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
    a: &'static str,
    h: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    g: String,
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
    s: Vec<SrcOut>,
    j: Vec<JudgeOut>,
}

fn finish(mut v: Verdict) -> String {
    let mut s = serde_json::to_string(&v).unwrap();
    if s.len() > OUT_MAX {
        // Keep the bounded fields, drop detail. The verdict stays valid but says it was cut.
        v.s.clear();
        v.j.clear();
        v.o = "X";
        v.rc = RC_SIZE;
        s = serde_json::to_string(&v).unwrap();
    }
    s
}

fn bad(rd: u64, ih: String, t: u64, rc: u8) -> String {
    finish(Verdict { v: 1, rd, ih, o: "X", rc, t, w: false, s: vec![], j: vec![] })
}

fn host_ok(url: &str) -> bool {
    url.starts_with("https://") && !url.contains('@')
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
    if inp.v != 1
        || sha256_hex(inp.claim.as_bytes()) != inp.th
        || inp.m.len() != 3
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

    // 2. Data sentences: the numbers must agree first.
    let mut numbers_true = false;
    let user: String;
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
                v: 1,
                rd,
                ih,
                o: "V",
                rc: RC_SRC_DISAGREE,
                t: now_unix,
                w: false,
                s: souts,
                j: vec![],
            });
        }
        numbers_true = (first - claimed.unwrap()).abs() <= tol;
        user = data_user(&inp.claim, &canon_number(claimed.unwrap()));
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
        user = prose_user(&parts.join("\n---\n"), &inp.claim);
    }

    // 3. Ask the three judges, one after another, and check every signature.
    let mut answers = vec![];
    let mut jouts = vec![];
    let mut unsigned = 0;
    for (idx, model) in inp.m.iter().enumerate() {
        if elapsed() >= SELF_STOP_SECS {
            return bad(rd, ih, now_unix, RC_TIMEOUT);
        }
        let req = request_body(model, &user, inp.seed);
        let rh = request_hash(&req);
        match judge.ask(idx, model, &req, JUDGE_SECS) {
            Ok(r) => {
                let resp_hash = sha256_hex(&r.resp);
                let expect = format!("{}:{}:{}", model, rh, resp_hash);
                let good = r.kind == "provider_tee" && r.text == expect && sig::verify(&r.text, &r.sig, &r.addr);
                if good {
                    let a = Ans::parse(&r.content);
                    answers.push(a);
                    jouts.push(JudgeOut { m: idx, a: a.letter(), h: b64(&sha256(&r.resp)), g: b64(&r.sig) });
                } else {
                    unsigned += 1;
                    answers.push(Ans::U);
                    jouts.push(JudgeOut { m: idx, a: "X", h: b64(&sha256(&r.resp)), g: String::new() });
                }
            }
            Err(_) => {
                unsigned += 1;
                answers.push(Ans::U);
                jouts.push(JudgeOut { m: idx, a: "X", h: String::new(), g: String::new() });
            }
        }
    }

    let (o, w) = if inp.kind == "data" { outcome::data(&answers, numbers_true) } else { outcome::prose(&answers) };
    let rc = if unsigned > 0 && o == "U" { RC_UNSIGNED } else { RC_OK };
    finish(Verdict { v: 1, rd, ih, o, rc, t: now_unix, w, s: souts, j: jouts })
}
