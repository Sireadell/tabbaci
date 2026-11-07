use crate::judges::standin::StandIn;
use crate::run::{run, Fetcher, Judge, JudgeReply};
use crate::util::sha256_hex;
use std::collections::HashMap;

struct Fake(HashMap<String, (u16, String)>);

impl Fetcher for Fake {
    fn get(&self, url: &str, _m: usize, _t: u64) -> Result<(u16, Vec<u8>), String> {
        self.0.get(url).map(|(c, b)| (*c, b.clone().into_bytes())).ok_or("no route".to_string())
    }
}

fn site(pairs: &[(&str, u16, &str)]) -> Fake {
    Fake(pairs.iter().map(|(u, c, b)| (u.to_string(), (*c, b.to_string()))).collect())
}

fn data_input(claim: &str, cv: &str, tol: f64) -> Vec<u8> {
    serde_json::json!({
        "v":1,"rd":4821,"kind":"data","th":sha256_hex(claim.as_bytes()),"claim":claim,"cv":cv,
        "src":[{"i":3,"u":"https://a.example/x","x":"json:$.count","tol":tol,"g":"price-1"},
               {"i":7,"u":"https://b.example/y","x":"json:$.total","tol":tol,"g":"chain"}],
        "m":["deepseek-ai/DeepSeek-V3.1","Qwen/Qwen3-30B-A3B-Instruct-2507","openai/gpt-oss-120b"],
        "seed":99,"asof":276636124
    })
    .to_string()
    .into_bytes()
}

fn prose_input(claim: &str) -> Vec<u8> {
    serde_json::json!({
        "v":1,"rd":5,"kind":"prose","th":sha256_hex(claim.as_bytes()),"claim":claim,
        "src":[{"i":1,"u":"https://a.example/p","g":"enc-1"},{"i":2,"u":"https://b.example/q","g":"enc-2"}],
        "m":["m1","m2","m3"],"seed":1,"asof":1,"terms":["launched"]
    })
    .to_string()
    .into_bytes()
}

fn go(input: &[u8], f: &Fake, answers: &[&str]) -> serde_json::Value {
    let mut j = StandIn::new(answers.iter().map(|s| s.to_string()).collect());
    let out = run(input, f, &mut j, &|| 1, 1_700_000_000);
    assert!(out.len() <= 900, "verdict too big: {}", out.len());
    serde_json::from_str(&out).unwrap()
}

#[test]
fn data_sentence_true_and_size() {
    let f = site(&[("https://a.example/x", 200, r#"{"count":250}"#), ("https://b.example/y", 200, r#"{"total":250}"#)]);
    let v = go(&data_input("NEAR has 250 validators", "250", 0.0), &f, &["TRUE", "TRUE", "TRUE"]);
    assert_eq!(v["o"], "T");
    assert_eq!(v["rc"], 0);
    assert_eq!(v["j"].as_array().unwrap().len(), 3);
    assert_eq!(v["s"].as_array().unwrap().len(), 2);
    println!("verdict bytes: {}", serde_json::to_string(&v).unwrap().len());
}

#[test]
fn data_sentence_false_when_number_differs() {
    let f = site(&[("https://a.example/x", 200, r#"{"count":250}"#), ("https://b.example/y", 200, r#"{"total":250}"#)]);
    let v = go(&data_input("NEAR has 300 validators", "300", 0.0), &f, &["TRUE", "TRUE", "TRUE"]);
    assert_eq!(v["o"], "F");
}

#[test]
fn sources_disagree_is_void() {
    let f = site(&[("https://a.example/x", 200, r#"{"count":250}"#), ("https://b.example/y", 200, r#"{"total":251}"#)]);
    let v = go(&data_input("NEAR has 250 validators", "250", 0.0), &f, &["TRUE", "TRUE", "TRUE"]);
    assert_eq!(v["o"], "V");
    assert_eq!(v["rc"], 1);
}

#[test]
fn tolerance_lets_close_numbers_agree() {
    let f = site(&[("https://a.example/x", 200, r#"{"count":250}"#), ("https://b.example/y", 200, r#"{"total":251}"#)]);
    let v = go(&data_input("NEAR has 250 validators", "250", 1.0), &f, &["TRUE", "TRUE", "TRUE"]);
    assert_eq!(v["o"], "T");
}

#[test]
fn judges_split_is_disputed() {
    let f = site(&[("https://a.example/x", 200, r#"{"count":250}"#), ("https://b.example/y", 200, r#"{"total":250}"#)]);
    let v = go(&data_input("NEAR has 250 validators", "250", 0.0), &f, &["TRUE", "TRUE", "FALSE"]);
    assert_eq!(v["o"], "D");
}

#[test]
fn fetch_failure_is_reported_not_guessed() {
    let f = site(&[("https://a.example/x", 200, r#"{"count":250}"#), ("https://b.example/y", 503, "down")]);
    let v = go(&data_input("NEAR has 250 validators", "250", 0.0), &f, &["TRUE", "TRUE", "TRUE"]);
    assert_eq!(v["o"], "X");
    assert_eq!(v["rc"], 2);
}

#[test]
fn http_not_https_is_refused() {
    let mut inp: serde_json::Value = serde_json::from_slice(&data_input("c", "1", 0.0)).unwrap();
    inp["src"][0]["u"] = "http://a.example/x".into();
    let f = site(&[]);
    let v = go(inp.to_string().as_bytes(), &f, &["TRUE", "TRUE", "TRUE"]);
    assert_eq!(v["rc"], 6);
}

#[test]
fn same_group_twice_is_refused() {
    let mut inp: serde_json::Value = serde_json::from_slice(&data_input("c", "1", 0.0)).unwrap();
    inp["src"][1]["g"] = "price-1".into();
    let f = site(&[]);
    let v = go(inp.to_string().as_bytes(), &f, &["TRUE", "TRUE", "TRUE"]);
    assert_eq!(v["rc"], 6);
}

#[test]
fn changed_claim_text_is_refused() {
    let mut inp: serde_json::Value = serde_json::from_slice(&data_input("NEAR has 250 validators", "250", 0.0)).unwrap();
    inp["claim"] = "NEAR has 999 validators".into();
    let f = site(&[]);
    let v = go(inp.to_string().as_bytes(), &f, &["TRUE", "TRUE", "TRUE"]);
    assert_eq!(v["rc"], 6);
}

#[test]
fn prose_sentence_two_sources() {
    let f = site(&[
        ("https://a.example/p", 200, "intro text. Near Protocol launched its mainnet in 2020. more text"),
        ("https://b.example/q", 200, "history: the network launched in October 2020, said the report"),
    ]);
    let v = go(&prose_input("NEAR mainnet launched in 2020."), &f, &["TRUE", "TRUE", "TRUE"]);
    assert_eq!(v["o"], "T");
    let v = go(&prose_input("NEAR mainnet launched in 2020."), &f, &["TRUE", "TRUE", "UNSURE"]);
    assert_eq!(v["o"], "T");
    assert_eq!(v["w"], true);
    let v = go(&prose_input("NEAR mainnet launched in 2020."), &f, &["TRUE", "FALSE", "UNSURE"]);
    assert_eq!(v["o"], "U");
}

/// A judge whose signature is from the gateway, not the model, must never count.
struct GatewaySigned(StandIn);
impl Judge for GatewaySigned {
    fn ask(&mut self, i: usize, m: &str, r: &[u8], t: u64) -> Result<JudgeReply, String> {
        let mut x = self.0.ask(i, m, r, t)?;
        x.kind = "gateway".into();
        Ok(x)
    }
}

#[test]
fn gateway_signature_counts_as_unsure() {
    let f = site(&[("https://a.example/x", 200, r#"{"count":250}"#), ("https://b.example/y", 200, r#"{"total":250}"#)]);
    let mut j = GatewaySigned(StandIn::new(vec!["TRUE".into(); 3]));
    let out = run(&data_input("NEAR has 250 validators", "250", 0.0), &f, &mut j, &|| 1, 1);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["o"], "U");
    assert_eq!(v["rc"], 4);
}

/// A judge that rewrites its answer after signing must not count.
struct Tamper(StandIn);
impl Judge for Tamper {
    fn ask(&mut self, i: usize, m: &str, r: &[u8], t: u64) -> Result<JudgeReply, String> {
        let mut x = self.0.ask(i, m, r, t)?;
        x.resp.push(b' ');
        Ok(x)
    }
}

#[test]
fn tampered_response_counts_as_unsure() {
    let f = site(&[("https://a.example/x", 200, r#"{"count":250}"#), ("https://b.example/y", 200, r#"{"total":250}"#)]);
    let mut j = Tamper(StandIn::new(vec!["TRUE".into(); 3]));
    let out = run(&data_input("NEAR has 250 validators", "250", 0.0), &f, &mut j, &|| 1, 1);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["o"], "U");
}

#[test]
fn self_stop_after_75_seconds() {
    let f = site(&[("https://a.example/x", 200, r#"{"count":250}"#), ("https://b.example/y", 200, r#"{"total":250}"#)]);
    let mut j = StandIn::new(vec!["TRUE".into(); 3]);
    let out = run(&data_input("NEAR has 250 validators", "250", 0.0), &f, &mut j, &|| 80, 1);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["o"], "X");
    assert_eq!(v["rc"], 3);
}

#[test]
fn input_hash_matches_exact_bytes() {
    use crate::util::{b64, sha256};
    let inp = data_input("NEAR has 250 validators", "250", 0.0);
    let f = site(&[("https://a.example/x", 200, r#"{"count":250}"#), ("https://b.example/y", 200, r#"{"total":250}"#)]);
    let v = go(&inp, &f, &["TRUE", "TRUE", "TRUE"]);
    assert_eq!(v["ih"], b64(&sha256(&inp)));
}
