//! Private rounds: a sentence about a page only the writer's key can read.
use crate::judges::standin::StandIn;
use crate::keyed::{leaks_key, KeyedFetcher};
use crate::run::{run, Fetcher, Judge, JudgeReply};
use crate::spec::{op_holds, op_words};
use crate::util::{b64, sha256, sha256_hex};
use std::cell::RefCell;

const KEY: &str = "ghp_CANARYtoken0000aaaaBBBBccccDDDD1234";
const AI_KEY: &str = "nearai-CANARY-ai-key-55aa77bb";
const MARKER: &str = "SECRET-MARKER-7c1e-do-not-publish";
const URL: &str = "https://api.github.com/repos/sireadell/private-repo/commits?author=sireadell&since=2026-11-01T00:00:00Z&until=2026-12-01T00:00:00Z&per_page=100";
const CLAIM: &str = "Sireadell made more than 20 commits to a private GitHub repo in November 2026.";

/// A fake GitHub: the commit list for the private repo, each commit message carrying the marker.
struct Gh {
    body: String,
    status: u16,
    seen: RefCell<Vec<(String, String, String)>>,
    public_calls: RefCell<usize>,
}

impl KeyedFetcher for Gh {
    fn get_with_header(&self, url: &str, name: &str, value: &str, _m: usize) -> Result<(u16, Vec<u8>), String> {
        self.seen.borrow_mut().push((url.into(), name.into(), value.into()));
        Ok((self.status, self.body.clone().into_bytes()))
    }
}

impl Fetcher for Gh {
    fn get(&self, _u: &str, _m: usize, _t: u64) -> Result<(u16, Vec<u8>), String> {
        *self.public_calls.borrow_mut() += 1;
        Err("a private round must not use the public reader".into())
    }
}

fn commits(n: usize) -> String {
    let list: Vec<serde_json::Value> = (0..n)
        .map(|i| {
            serde_json::json!({"sha": format!("{:040x}", i + 1), "commit": {"message": format!("{MARKER} change {i}"),
                "author": {"name": "Sireadell", "date": "2026-11-10T10:00:00Z"}}})
        })
        .collect();
    serde_json::to_string(&list).unwrap()
}

fn gh(n: usize) -> Gh {
    Gh { body: commits(n), status: 200, seen: RefCell::new(vec![]), public_calls: RefCell::new(0) }
}

fn secrets(name: &str) -> String {
    match name {
        "PRIVATE_SOURCE_KEY" => KEY.into(),
        "NEAR_AI_KEY" => AI_KEY.into(),
        _ => String::new(),
    }
}

fn private_input_with(f: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let mut v = serde_json::json!({
        "v":3,"rd":40,"kind":"private","th":sha256_hex(CLAIM.as_bytes()),"claim":CLAIM,"cv":"20","op":"gt",
        "src":[{"i":9,"u":URL,"x":"len:$","g":"own-github","h":"api.github.com","sk":"PRIVATE_SOURCE_KEY"}],
        "m":["m1","m2","m3"],"seed":5,"asof":1
    });
    f(&mut v);
    v.to_string().into_bytes()
}

fn private_input() -> Vec<u8> {
    private_input_with(|_| {})
}

/// Records every request the judges are sent.
struct Recorder(StandIn, RefCell<Vec<Vec<u8>>>);
impl Judge for Recorder {
    fn ask(&mut self, i: usize, m: &str, r: &[u8], t: u64) -> Result<JudgeReply, String> {
        self.1.borrow_mut().push(r.to_vec());
        self.0.ask(i, m, r, t)
    }
}

fn go_with(input: &[u8], g: &Gh, answers: &[&str]) -> (String, Vec<Vec<u8>>) {
    let mut j = Recorder(StandIn::new(answers.iter().map(|s| s.to_string()).collect()), RefCell::new(vec![]));
    let out = run(input, g, g, &secrets, &mut j, &|| 1, 1_700_000_000);
    assert!(out.len() <= 6000);
    let reqs = j.1.borrow().clone();
    (out, reqs)
}

fn go(input: &[u8], g: &Gh) -> serde_json::Value {
    serde_json::from_str(&go_with(input, g, &["TRUE", "TRUE", "TRUE"]).0).unwrap()
}

/// Nothing of the page and no secret anywhere in what leaves, including the decoded judge responses.
fn assert_clean(out: &str) {
    let v: serde_json::Value = serde_json::from_str(out).unwrap();
    let mut all = out.to_string();
    for j in v["j"].as_array().into_iter().flatten() {
        if let Some(r) = j["r"].as_str().and_then(crate::util::unb64) {
            all.push_str(&String::from_utf8_lossy(&r));
        }
    }
    assert!(!all.contains(MARKER), "the page text left the chip");
    assert!(!all.contains("Sireadell\",\"date"), "page fields left the chip");
    assert!(!leaks_key(&all, KEY), "the source key left the chip");
    assert!(!leaks_key(&all, AI_KEY), "the AI key left the chip");
    assert!(v.get("ex").is_none(), "a private verdict has no excerpt");
    for s in v["s"].as_array().into_iter().flatten() {
        assert!(s.get("x").is_none(), "a private verdict has no extracted value");
    }
    for j in v["j"].as_array().into_iter().flatten() {
        assert!(j.get("q").is_none(), "a private verdict has no quotes");
    }
}

#[test]
fn a_true_private_sentence_is_ruled_true_and_only_the_fingerprint_leaves() {
    let g = gh(37);
    let (out, _) = go_with(&private_input(), &g, &["TRUE", "TRUE", "TRUE"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["v"], 3);
    assert_eq!(v["o"], "T");
    assert_eq!(v["rc"], 0);
    let s = &v["s"][0];
    assert_eq!(s["i"], 9);
    assert_eq!(s["c"], 200);
    assert_eq!(s["y"], true);
    assert_eq!(s["h"], b64(&sha256(g.body.as_bytes())), "fingerprint is sha256 of the page bytes, base64url");
    assert!(!out.contains("\"37\"") && !out.contains(":37"), "the exact count stays in the chip");
    assert_eq!(v["j"].as_array().unwrap().len(), 3);
    assert_clean(&out);
    println!("private verdict bytes: {}", out.len());
}

#[test]
fn a_false_private_sentence_is_ruled_false() {
    let v = go(&private_input(), &gh(12));
    assert_eq!(v["o"], "F");
    assert_eq!(v["s"][0]["y"], false);
}

#[test]
fn the_key_goes_only_to_the_named_host_with_the_bearer_header() {
    let g = gh(25);
    go(&private_input(), &g);
    let seen = g.seen.borrow();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, URL);
    assert_eq!(seen[0].1, "Authorization");
    assert_eq!(seen[0].2, format!("Bearer {KEY}"));
    assert_eq!(*g.public_calls.borrow(), 0, "a private round never uses the public reader");
}

#[test]
fn the_judges_see_the_sentence_and_the_address_but_never_the_page_or_the_number() {
    let (_, reqs) = go_with(&private_input(), &gh(37), &["TRUE", "TRUE", "TRUE"]);
    assert_eq!(reqs.len(), 3);
    for r in &reqs {
        let t = String::from_utf8_lossy(r);
        assert!(t.contains("more than 20"), "the judges read the comparison");
        assert!(t.contains("api.github.com/repos/sireadell/private-repo"), "the judges read the public address");
        assert!(!t.contains(MARKER), "a judge was shown the page");
        assert!(!t.contains("37"), "a judge was shown the number on the page");
        assert!(!leaks_key(&t, KEY) && !leaks_key(&t, AI_KEY));
    }
}

#[test]
fn a_sentence_that_says_something_else_is_disputed_by_the_judges() {
    // the judges say the sentence does not say "more than 20", so the number cannot decide it
    let (out, _) = go_with(&private_input(), &gh(37), &["FALSE", "FALSE", "FALSE"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["o"], "D");
}

#[test]
fn canary_marker_and_keys_appear_nowhere_across_many_rulings() {
    let mut all = String::new();
    for n in [0usize, 5, 20, 21, 100] {
        for words in [["TRUE", "TRUE", "TRUE"], ["TRUE", "UNSURE", "TRUE"], ["FALSE", "TRUE", "TRUE"]] {
            let (out, _) = go_with(&private_input(), &gh(n), &words);
            assert_clean(&out);
            all.push_str(&out);
        }
    }
    assert!(!all.contains(MARKER));
    assert!(!leaks_key(&all, KEY));
}

#[test]
fn a_judge_that_repeats_the_key_cannot_carry_it_out() {
    let echo = format!("TRUE {KEY}");
    let (out, _) = go_with(&private_input(), &gh(30), &[&echo, "TRUE", "TRUE"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["o"], "X");
    assert_eq!(v["rc"], 7);
    assert!(v["j"].as_array().unwrap().is_empty());
    assert_clean(&out);
}

#[test]
fn a_page_that_echoes_the_key_into_the_value_is_refused() {
    let g = Gh { body: format!("{{\"total_count\":\"{KEY}\"}}"), ..gh(0) };
    let inp = private_input_with(|v| v["src"][0]["x"] = "json:$.total_count".into());
    let (out, _) = go_with(&inp, &g, &["TRUE", "TRUE", "TRUE"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["rc"], 2);
    assert_clean(&out);
}

#[test]
fn the_search_count_field_works_as_well_as_the_list_length() {
    let g = Gh { body: format!("{{\"total_count\":42,\"incomplete_results\":false,\"items\":[{{\"m\":\"{MARKER}\"}}]}}"), ..gh(0) };
    let inp = private_input_with(|v| {
        v["src"][0]["u"] = "https://api.github.com/search/commits?q=repo:sireadell/private-repo+author:sireadell+committer-date:2026-11-01..2026-11-30&per_page=1".into();
        v["src"][0]["x"] = "json:$.total_count".into();
    });
    let (out, _) = go_with(&inp, &g, &["TRUE", "TRUE", "TRUE"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["o"], "T");
    assert_clean(&out);
}

#[test]
fn a_missing_key_stops_before_any_request() {
    let g = gh(30);
    let mut j = StandIn::new(vec!["TRUE".into(); 3]);
    let out = run(&private_input(), &g, &g, &|_: &str| String::new(), &mut j, &|| 1, 1);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["rc"], 8);
    assert!(g.seen.borrow().is_empty());
    assert_eq!(j.calls, 0);
}

#[test]
fn a_failed_read_is_reported_not_guessed() {
    let g = Gh { status: 404, ..gh(30) };
    let v = go(&private_input(), &g);
    assert_eq!(v["o"], "X");
    assert_eq!(v["rc"], 2);
}

#[test]
fn every_bad_private_input_is_refused_before_any_request() {
    type Change = fn(&mut serde_json::Value);
    let cases: Vec<(&str, Change)> = vec![
        ("the NEAR AI key named as the source key", |v| v["src"][0]["sk"] = "NEAR_AI_KEY".into()),
        ("a key name outside PRIVATE_", |v| v["src"][0]["sk"] = "GITHUB_TOKEN".into()),
        ("a lowercase key name", |v| v["src"][0]["sk"] = "PRIVATE_source".into()),
        ("an empty key name", |v| v["src"][0]["sk"] = "".into()),
        ("a host the build does not allow", |v| {
            v["src"][0]["h"] = "evil.example".into();
            v["src"][0]["u"] = "https://evil.example/x".into();
        }),
        ("an address on another host", |v| v["src"][0]["u"] = "https://api.github.com.evil.example/x".into()),
        ("http", |v| v["src"][0]["u"] = "http://api.github.com/x".into()),
        ("a user part in the address", |v| v["src"][0]["u"] = "https://api.github.com@evil.example/x".into()),
        ("a text extractor", |v| v["src"][0]["x"] = "find:<a>|</a>".into()),
        ("an unknown comparison", |v| v["op"] = "about".into()),
        ("no comparison", |v| v["op"] = "".into()),
        ("no number", |v| v["cv"] = serde_json::Value::Null),
        ("a number that is not a number", |v| v["cv"] = "twenty".into()),
        ("two sources", |v| {
            let s = v["src"][0].clone();
            v["src"] = serde_json::json!([s.clone(), s]);
        }),
        ("version 2", |v| v["v"] = 2.into()),
        ("two judges", |v| v["m"] = serde_json::json!(["m1", "m2"])),
    ];
    for (why, change) in cases {
        let g = gh(30);
        let mut inp: serde_json::Value = serde_json::from_slice(&private_input()).unwrap();
        change(&mut inp);
        let mut j = StandIn::new(vec!["TRUE".into(); 3]);
        let out = run(inp.to_string().as_bytes(), &g, &g, &secrets, &mut j, &|| 1, 1);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["rc"], 6, "{why} was not refused");
        assert!(g.seen.borrow().is_empty(), "{why}: a request left");
        assert_eq!(j.calls, 0, "{why}: a judge was asked");
        assert!(!leaks_key(&out, KEY));
    }
}

#[test]
fn a_public_round_can_never_send_a_key() {
    let g = gh(30);
    let claim = "NEAR has 250 validators";
    let inp = serde_json::json!({
        "v":3,"rd":4,"kind":"data","th":sha256_hex(claim.as_bytes()),"claim":claim,"cv":"250",
        "src":[{"i":3,"u":"https://api.github.com/x","x":"json:$.c","g":"a","sk":"PRIVATE_SOURCE_KEY"},
               {"i":7,"u":"https://b.example/y","x":"json:$.c","g":"b"}],
        "m":["m1","m2","m3"],"seed":1,"asof":1
    });
    let mut j = StandIn::new(vec!["TRUE".into(); 3]);
    let out = run(inp.to_string().as_bytes(), &g, &g, &secrets, &mut j, &|| 1, 1);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["rc"], 6);
    assert!(g.seen.borrow().is_empty());
    assert_eq!(*g.public_calls.borrow(), 0);
}

#[test]
fn self_stop_applies_to_private_rounds() {
    let g = gh(30);
    let mut j = StandIn::new(vec!["TRUE".into(); 3]);
    let out = run(&private_input(), &g, &g, &secrets, &mut j, &|| 80, 1);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["rc"], 3);
    assert!(g.seen.borrow().is_empty());
}

#[test]
fn comparison_table() {
    assert!(op_holds("gt", 21.0, 20.0, 0.0) && !op_holds("gt", 20.0, 20.0, 0.0));
    assert!(op_holds("ge", 20.0, 20.0, 0.0) && !op_holds("ge", 19.0, 20.0, 0.0));
    assert!(op_holds("lt", 19.0, 20.0, 0.0) && !op_holds("lt", 20.0, 20.0, 0.0));
    assert!(op_holds("le", 20.0, 20.0, 0.0) && !op_holds("le", 21.0, 20.0, 0.0));
    assert!(op_holds("eq", 20.0, 20.0, 0.0) && op_holds("eq", 21.0, 20.0, 1.0) && !op_holds("eq", 22.0, 20.0, 1.0));
    assert_eq!(op_words("gt"), Some("more than"));
    assert!(op_words("ne").is_none());
}

#[test]
fn the_boundary_is_exact() {
    assert_eq!(go(&private_input(), &gh(20))["o"], "F", "20 is not more than 20");
    assert_eq!(go(&private_input(), &gh(21))["o"], "T");
}

/// The contract's and the checker's tests read this file.
/// Rewrite it with: FIXTURE=1 cargo test fixture
#[test]
fn fixture_private_round_for_the_contract_and_checker() {
    let g = gh(37);
    let inp = private_input();
    let mut j = StandIn::new(vec!["TRUE".into(), "TRUE".into(), "UNSURE".into()]);
    let out = run(&inp, &g, &g, &secrets, &mut j, &|| 1, 1_700_000_000);
    assert_clean(&out);
    let body = serde_json::json!({
        "input": String::from_utf8(inp).unwrap(),
        "output": out,
        "signer": hex::encode(crate::sig::standin_ed::public()),
        // the page bytes, so a test can show that the person who holds the page can match the fingerprint
        "page_sha256_hex": sha256_hex(g.body.as_bytes()),
    });
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/private_round.json");
    if std::env::var("FIXTURE").is_ok() {
        std::fs::write(path, serde_json::to_string_pretty(&body).unwrap()).unwrap();
    }
    let on_disk: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(on_disk, body, "the saved fixture is out of date: run FIXTURE=1 cargo test fixture");
}
