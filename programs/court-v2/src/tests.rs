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
        "v":2,"rd":4821,"kind":"data","th":sha256_hex(claim.as_bytes()),"claim":claim,"cv":cv,
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
        "v":2,"rd":5,"kind":"prose","th":sha256_hex(claim.as_bytes()),"claim":claim,
        "src":[{"i":1,"u":"https://a.example/p","g":"enc-1"},{"i":2,"u":"https://b.example/q","g":"enc-2"}],
        "m":["m1","m2","m3"],"seed":1,"asof":1,"terms":["launched"]
    })
    .to_string()
    .into_bytes()
}

fn go(input: &[u8], f: &Fake, answers: &[&str]) -> serde_json::Value {
    let mut j = StandIn::new(answers.iter().map(|s| s.to_string()).collect());
    let out = run(input, f, &mut j, &|| 1, 1_700_000_000);
    assert!(out.len() <= 6000, "verdict too big: {}", out.len());
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
const Q1: &str = "Near Protocol launched its mainnet in 2020.";

fn prose_site() -> Fake {
    site(&[
        ("https://a.example/p", 200, "intro text. Near Protocol launched its mainnet in 2020. more text"),
        ("https://b.example/q", 200, "history: the network launched in October 2020, said the report"),
    ])
}

#[test]
fn prose_sentence_two_sources() {
    let f = prose_site();
    let t = format!("TRUE\n{}", Q1);
    let v = go(&prose_input("NEAR mainnet launched in 2020."), &f, &[&t, &t, &t]);
    assert_eq!(v["o"], "T");
    let v = go(&prose_input("NEAR mainnet launched in 2020."), &f, &[&t, &t, "UNSURE\nNONE"]);
    assert_eq!(v["o"], "T");
    assert_eq!(v["w"], true);
    let fa = format!("FALSE\n{}", Q1);
    let v = go(&prose_input("NEAR mainnet launched in 2020."), &f, &[&t, &fa, "UNSURE\nNONE"]);
    assert_eq!(v["o"], "U");
}

#[test]
fn every_counted_judge_carries_a_quote_that_is_on_the_page() {
    let f = prose_site();
    let t = format!("TRUE\n{}", Q1);
    let v = go(&prose_input("NEAR mainnet launched in 2020."), &f, &[&t, &t, &t]);
    assert_eq!(v["v"], 2);
    let ex = v["ex"].as_str().unwrap();
    assert!(ex.contains("launched its mainnet"));
    for j in v["j"].as_array().unwrap() {
        assert_eq!(j["a"], "T");
        assert_eq!(j["q"], Q1);
        assert!(j["r"].as_str().unwrap().len() > 40);
        assert_eq!(j["g"].as_str().unwrap().len(), 86);
        assert_eq!(j["k"].as_str().unwrap().len(), 43);
        assert!(j.get("qx").is_none());
    }
}

#[test]
fn a_quote_that_is_not_on_the_page_turns_that_judge_into_unsure() {
    let f = prose_site();
    let t = format!("TRUE\n{}", Q1);
    let made_up = "TRUE\nNEAR was founded on the Moon.";
    let v = go(&prose_input("NEAR mainnet launched in 2020."), &f, &[&t, &t, made_up]);
    assert_eq!(v["o"], "T");
    assert_eq!(v["w"], true);
    let j = &v["j"][2];
    assert_eq!(j["a"], "U");
    assert_eq!(j["v"], "T");
    assert_eq!(j["qx"], true);
    // two bad quotes leave no majority
    let v = go(&prose_input("NEAR mainnet launched in 2020."), &f, &[&t, made_up, made_up]);
    assert_eq!(v["o"], "U");
}

#[test]
fn true_or_false_without_a_quote_does_not_count() {
    let f = prose_site();
    let v = go(&prose_input("NEAR mainnet launched in 2020."), &f, &["TRUE", "TRUE", "TRUE"]);
    assert_eq!(v["o"], "U");
    assert_eq!(v["j"][0]["qx"], true);
}

#[test]
fn quote_matching_ignores_spaces_and_capitals_only() {
    let f = prose_site();
    let loose = "TRUE\n  near   PROTOCOL launched its\nmainnet in 2020. ";
    let v = go(&prose_input("NEAR mainnet launched in 2020."), &f, &[loose, loose, loose]);
    assert_eq!(v["o"], "T");
    let changed = "TRUE\nNear Protocol launched its mainnet in 2021.";
    let v = go(&prose_input("NEAR mainnet launched in 2020."), &f, &[changed, changed, changed]);
    assert_eq!(v["o"], "U");
}

#[test]
fn a_quote_in_the_page_but_outside_the_excerpt_does_not_count() {
    let f = site(&[
        ("https://a.example/p", 200, &format!("The far away fact. {} Near Protocol launched its mainnet in 2020. {}", "x".repeat(800), "y".repeat(800))),
        ("https://b.example/q", 200, "history: the network launched in October 2020, said the report"),
    ]);
    let far = "TRUE
The far away fact.".to_string();
    let v = go(&prose_input("NEAR mainnet launched in 2020."), &f, &[&far, &far, &far]);
    assert_eq!(v["o"], "U");
}

#[test]
fn a_forged_judge_signature_counts_as_no_answer() {
    struct Forger(StandIn);
    impl Judge for Forger {
        fn ask(&mut self, i: usize, m: &str, r: &[u8], t: u64) -> Result<JudgeReply, String> {
            let mut x = self.0.ask(i, m, r, t)?;
            x.sig[0] ^= 1;
            Ok(x)
        }
    }
    let f = prose_site();
    let t = format!("TRUE\n{}", Q1);
    let mut j = Forger(StandIn::new(vec![t.clone(), t.clone(), t]));
    let out = run(&prose_input("NEAR mainnet launched in 2020."), &f, &mut j, &|| 1, 1);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["o"], "U");
    assert_eq!(v["rc"], 4);
    assert_eq!(v["j"][0]["a"], "X");
}

#[test]
fn version_one_input_is_refused() {
    let mut inp: serde_json::Value = serde_json::from_slice(&prose_input("c")).unwrap();
    inp["v"] = 1.into();
    let v = go(inp.to_string().as_bytes(), &prose_site(), &["TRUE", "TRUE", "TRUE"]);
    assert_eq!(v["rc"], 6);
}

/// The contract's tests read this file, so a change in the program's output that the contract cannot read shows up there.
/// Rewrite it with: FIXTURE=1 cargo test fixture
#[test]
fn fixture_for_the_contract() {
    let f = prose_site();
    let inp = prose_input("NEAR mainnet launched in 2020.");
    let t = format!("TRUE\n{}", Q1);
    let mut j = StandIn::new(vec![t.clone(), t.clone(), "TRUE\nNEAR was founded on the Moon.".to_string()]);
    let out = run(&inp, &f, &mut j, &|| 1, 1_700_000_000);
    let body = serde_json::json!({
        "input": String::from_utf8(inp).unwrap(),
        "output": out,
        "signer": hex::encode(crate::sig::standin_ed::public()),
    });
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/prose_round.json");
    if std::env::var("FIXTURE").is_ok() {
        std::fs::write(path, serde_json::to_string_pretty(&body).unwrap()).unwrap();
    }
    let on_disk: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(on_disk, body, "the saved fixture is out of date: run FIXTURE=1 cargo test fixture");
}
