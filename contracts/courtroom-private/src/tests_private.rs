//! Private rounds: what the contract accepts, refuses and records.
use super::*;
use crate::testkit::{private_page_fp, signed_private_verdict, test_signers};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use courtroom_rules::Judged;
use near_sdk::serde_json;
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::testing_env;
use sha2::{Digest, Sha256};

const NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const PREFIX: &str = "https://api.github.com/repos/sireadell/private-repo/";
const URL: &str = "https://api.github.com/repos/sireadell/private-repo/commits?author=sireadell&since=2026-11-01T00:00:00Z&until=2026-12-01T00:00:00Z&per_page=100";
const CLAIM: &str = "Sireadell made more than 20 commits to a private GitHub repo in November 2026.";

fn as_(who: &str, bh: u64, deposit: u128) {
    let mut b = VMContextBuilder::new();
    b.predecessor_account_id(who.parse().unwrap())
        .current_account_id("court.t".parse().unwrap())
        .block_height(bh)
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .prepaid_gas(Gas::from_tgas(900))
        .random_seed([7u8; 32]);
    testing_env!(b.build());
}

fn refuses<F: FnOnce()>(f: F, want: &str) {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    let e = r.err().unwrap_or_else(|| panic!("should have refused with: {want}"));
    let got = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
    assert!(got.contains(want), "refused for the wrong reason: wanted \"{want}\", got \"{got}\"");
}

/// A contract with one public pair of sources (ids 1, 2) and one private source (id 3).
fn court() -> Courtroom {
    near_sdk::mock::with_mocked_blockchain(|b| {
        let _ = b.take_storage();
    });
    as_("dao.t", 10, 0);
    let models: Vec<String> = vec!["m1".into(), "m2".into(), "m3".into()];
    let mut c = Courtroom::new(
        "dao.t".parse().unwrap(),
        "https://example.com/court-private.wasm".into(),
        "a".repeat(64),
        models.clone(),
        test_signers(&["m1", "m2", "m3"]),
        Some("fkey-private.t".parse().unwrap()),
        Some("court".into()),
        U128(NEAR / 10),
        LadderConfig {
            top_models: models,
            challenge_base: U128(NEAR / 2),
            window_blocks: 100,
            run_deadline_blocks: 1000,
            pending_timeout_blocks: 50,
            audit_rate_bps: 0,
        },
    );
    c.add_source("https://s1.example/".into(), "g1".into());
    c.add_source("https://s2.example/".into(), "g2".into());
    c.add_private_source(PREFIX.into(), "own-github".into(), "PRIVATE_SOURCE_KEY".into());
    c
}

fn pick(id: u32, url: &str, x: &str) -> Pick {
    Pick { id, url: url.into(), x: x.into(), tol: 0.0 }
}

fn open_private(c: &mut Courtroom) -> u64 {
    as_("writer.t", 10, 0);
    c.open_round(CLAIM.into(), "private".into(), Some("20".into()), vec![pick(3, URL, "len:$")], vec![], None, Some("gt".into()))
}

fn deliver(c: &mut Courtroom, round: u64, out: Option<String>) {
    as_("anyone.t", 11, 0);
    c.begin_attempt(round);
    let n = c.get_round(round).unwrap().attempts.len() as u8;
    as_("court.t", 12, 0);
    c.on_attempt(round, n, Ok(out));
}

#[test]
fn a_private_round_carries_the_key_name_and_host_but_never_a_key() {
    let mut c = court();
    let r = open_private(&mut c);
    let round = c.get_round(r).unwrap();
    let inp: serde_json::Value = serde_json::from_str(&round.input).unwrap();
    assert_eq!(inp["v"], 3);
    assert_eq!(inp["kind"], "private");
    assert_eq!(inp["op"], "gt");
    assert_eq!(inp["cv"], "20");
    assert_eq!(inp["src"].as_array().unwrap().len(), 1);
    assert_eq!(inp["src"][0]["h"], "api.github.com");
    assert_eq!(inp["src"][0]["sk"], "PRIVATE_SOURCE_KEY");
    assert_eq!(inp["src"][0]["u"], URL);
}

#[test]
fn a_ruled_private_round_records_the_fingerprint_and_the_checked_signatures() {
    let mut c = court();
    let r = open_private(&mut c);
    let round = c.get_round(r).unwrap();
    let out = signed_private_verdict(&round.input, r, &round.ih, true, ["TRUE", "TRUE", "TRUE"]);
    deliver(&mut c, r, Some(out));
    let round = c.get_round(r).unwrap();
    assert_eq!(round.status, "settled");
    assert_eq!(round.outcome.as_deref(), Some("T"));
    assert_eq!(round.fingerprint, Some(private_page_fp()));
    assert_eq!(round.checked.len(), 3);
    for j in &round.checked {
        assert!(j.sig_ok);
        assert_eq!(URL_SAFE_NO_PAD.decode(&j.sig).unwrap().len(), 64);
        assert_eq!(URL_SAFE_NO_PAD.decode(&j.key).unwrap().len(), 32);
    }
    assert!(c.get_case(r).is_some(), "a true or false private ruling opens a case like any other");
}

#[test]
fn a_false_private_sentence_settles_false() {
    let mut c = court();
    let r = open_private(&mut c);
    let round = c.get_round(r).unwrap();
    deliver(&mut c, r, Some(signed_private_verdict(&round.input, r, &round.ih, false, ["TRUE", "TRUE", "TRUE"])));
    assert_eq!(c.get_round(r).unwrap().outcome.as_deref(), Some("F"));
}

#[test]
fn a_private_challenge_reads_the_same_source_again() {
    let mut c = court();
    let r = open_private(&mut c);
    let round = c.get_round(r).unwrap();
    deliver(&mut c, r, Some(signed_private_verdict(&round.input, r, &round.ih, true, ["TRUE", "TRUE", "TRUE"])));
    as_("ch.t", 13, 5 * NEAR);
    c.deposit();
    as_("ch.t", 13, 0);
    refuses(|| { c.challenge(r, vec![pick(3, URL, "len:$"), pick(1, "https://s1.example/x", "json:$.v")]); }, "a private challenge reads the same private source again");
    let id = c.challenge(r, vec![pick(3, URL, "len:$")]);
    let rr = c.get_round(id).unwrap();
    let inp: serde_json::Value = serde_json::from_str(&rr.input).unwrap();
    assert_eq!(inp["kind"], "private");
    assert_eq!(inp["src"][0]["sk"], "PRIVATE_SOURCE_KEY");
    assert_ne!(rr.ih, round.ih, "a new seed and round number");
}

#[test]
fn every_private_source_and_round_refusal() {
    let mut c = court();
    as_("someone.t", 10, 0);
    refuses(|| { c.add_private_source(PREFIX.into(), "g".into(), "PRIVATE_X".into()); }, "only the DAO");
    as_("dao.t", 10, 0);
    refuses(|| { c.add_private_source(PREFIX.into(), "g".into(), "NEAR_AI_KEY".into()); }, "key_env must be PRIVATE_");
    refuses(|| { c.add_private_source(PREFIX.into(), "g".into(), "PRIVATE_lower".into()); }, "key_env must be PRIVATE_");
    refuses(|| { c.add_private_source("https://evil.example/x/".into(), "g".into(), "PRIVATE_X".into()); }, "a path on api.github.com");
    refuses(|| { c.add_private_source("https://api.github.com/".into(), "g".into(), "PRIVATE_X".into()); }, "a path on api.github.com");
    refuses(|| { c.add_private_source("http://api.github.com/repos/".into(), "g".into(), "PRIVATE_X".into()); }, "a path on api.github.com");
    as_("writer.t", 10, 0);
    let open = |c: &mut Courtroom, kind: &str, picks: Vec<Pick>, op: Option<&str>| {
        c.open_round(CLAIM.into(), kind.into(), Some("20".into()), picks, vec![], None, op.map(|s| s.to_string()));
    };
    refuses(|| open(&mut c, "private", vec![pick(3, URL, "len:$")], None), "op must be");
    refuses(|| open(&mut c, "private", vec![pick(3, URL, "len:$")], Some("about")), "op must be");
    refuses(|| open(&mut c, "private", vec![pick(1, "https://s1.example/x", "json:$.v")], Some("gt")), "not a private source");
    refuses(|| open(&mut c, "private", vec![pick(3, URL, "len:$"), pick(3, URL, "len:$")], Some("gt")), "exactly one private source");
    refuses(|| open(&mut c, "private", vec![pick(3, "https://api.github.com/repos/someone-else/x", "len:$")], Some("gt")), "outside the approved source");
    refuses(|| open(&mut c, "private", vec![pick(3, URL, "find:a|b")], Some("gt")), "bad extractor");
    refuses(|| open(&mut c, "data", vec![pick(3, URL, "len:$"), pick(1, "https://s1.example/x", "json:$.v")], None), "only be used in a private round");
    refuses(|| open(&mut c, "prose", vec![pick(1, "https://s1.example/x", ""), pick(3, URL, "")], None), "only be used in a private round");
    refuses(|| open(&mut c, "data", vec![pick(1, "https://s1.example/x", "json:$.v"), pick(2, "https://s2.example/x", "json:$.v")], Some("gt")), "op is for private rounds only");
    // public rounds keep the two-group rule
    refuses(|| open(&mut c, "data", vec![pick(1, "https://s1.example/x", "json:$.v")], None), "2 or 3 sources");
}

// ---------- the contract's own check of a private verdict ----------

fn fixture() -> (String, String, String, Vec<Signer>, String) {
    // programs/court-private/testdata/private_round.json was written by the program's own code (stand-in judges)
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../programs/court-private/testdata/private_round.json");
    let f: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let input = f["input"].as_str().unwrap().to_string();
    let out = f["output"].as_str().unwrap().to_string();
    let signer = f["signer"].as_str().unwrap().to_string();
    let ih = URL_SAFE_NO_PAD.encode(Sha256::digest(input.as_bytes()));
    let signers: Vec<Signer> = ["m1", "m2", "m3"].iter().map(|m| Signer { model: m.to_string(), key: signer.clone(), from_ns: 0 }).collect();
    let fp_hex = f["page_sha256_hex"].as_str().unwrap().to_string();
    (input, out, ih, signers, fp_hex)
}

fn check(input: &str, ih: &str, out: &str, signers: &[Signer]) -> (Judged, Vec<CheckedJudge>) {
    testing_env!(VMContextBuilder::new().build());
    judging::judge_output_v3(40, ih, input, Some(out), signers)
}

fn failed(r: (Judged, Vec<CheckedJudge>)) -> String {
    match r.0 {
        Judged::Failed(w) => w,
        other => panic!("expected a failed attempt, got {:?}", other),
    }
}

fn changed(out: &str, f: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut v: serde_json::Value = serde_json::from_str(out).unwrap();
    f(&mut v);
    v.to_string()
}

#[test]
fn the_contract_accepts_what_the_real_private_program_code_produced() {
    let (input, out, ih, signers, fp_hex) = fixture();
    let (j, checked) = check(&input, &ih, &out, &signers);
    assert_eq!(j, Judged::Final("T".into()), "TRUE, TRUE, UNSURE with the page meeting the sentence");
    assert!(checked.iter().all(|c| c.sig_ok && !c.sig.is_empty() && !c.key.is_empty()));
    assert_eq!(checked[2].answer, "U");
    let fp = judging::private_fingerprint(&out).unwrap();
    assert_eq!(hex::encode(URL_SAFE_NO_PAD.decode(fp).unwrap()), fp_hex, "the fingerprint is sha256 of the page bytes");
}

#[test]
fn a_private_verdict_that_carries_any_page_detail_is_refused() {
    let (input, out, ih, signers, _) = fixture();
    let cases: Vec<(&str, String)> = vec![
        ("an excerpt", changed(&out, |v| v["ex"] = "some page text".into())),
        ("an extracted value", changed(&out, |v| v["s"][0]["x"] = "37".into())),
        ("any other source field", changed(&out, |v| v["s"][0]["n"] = 1234.into())),
        ("a judge quote", changed(&out, |v| v["j"][0]["q"] = "some page text".into())),
        ("no bit", changed(&out, |v| { v["s"][0].as_object_mut().unwrap().remove("y"); })),
        ("a second source", changed(&out, |v| { let s = v["s"][0].clone(); v["s"].as_array_mut().unwrap().push(s); })),
        ("a fingerprint that is not sha256", changed(&out, |v| v["s"][0]["h"] = "abc".into())),
        ("a failed fetch status", changed(&out, |v| v["s"][0]["c"] = 404.into())),
    ];
    for (why, bad) in cases {
        assert_eq!(failed(check(&input, &ih, &bad, &signers)), "PRIVATE_LEAK", "{why} was accepted");
    }
}

#[test]
fn a_private_verdict_with_the_bit_flipped_is_refused() {
    let (input, out, ih, signers, _) = fixture();
    let bad = changed(&out, |v| v["s"][0]["y"] = false.into());
    assert_eq!(failed(check(&input, &ih, &bad, &signers)), "OUTCOME_MISMATCH");
}

#[test]
fn judges_asked_about_another_source_do_not_count() {
    // the stored request names a different month: the signed answers were for another question
    let (input, out, _, signers, _) = fixture();
    let mut inp: serde_json::Value = serde_json::from_str(&input).unwrap();
    inp["src"][0]["u"] = URL.replace("2026-11-01", "2026-10-01").into();
    let input2 = inp.to_string();
    let ih2 = URL_SAFE_NO_PAD.encode(Sha256::digest(input2.as_bytes()));
    let out2 = changed(&out, |v| v["ih"] = ih2.clone().into());
    assert_eq!(failed(check(&input2, &ih2, &out2, &signers)), "BAD_JUDGE_SIGNATURE");
}

#[test]
fn untrusted_or_forged_judges_and_other_versions_are_refused() {
    let (input, out, ih, signers, _) = fixture();
    assert_eq!(failed(check(&input, &ih, &out, &[])), "UNTRUSTED_JUDGE_KEY");
    let forged = changed(&out, |v| {
        let mut g = URL_SAFE_NO_PAD.decode(v["j"][0]["g"].as_str().unwrap()).unwrap();
        g[0] ^= 1;
        v["j"][0]["g"] = URL_SAFE_NO_PAD.encode(g).into();
    });
    assert_eq!(failed(check(&input, &ih, &forged, &signers)), "BAD_JUDGE_SIGNATURE");
    assert_eq!(failed(check(&input, &ih, &changed(&out, |v| v["v"] = 2.into()), &signers)), "BAD_OUTPUT");
    assert_eq!(failed(check(&input, &ih, &changed(&out, |v| v["o"] = "V".into()), &signers)), "BAD_OUTPUT");
    assert_eq!(failed(check(&input, &ih, &changed(&out, |v| v["s"][0]["i"] = 4.into()), &signers)), "BAD_OUTPUT");
}

#[test]
fn a_box_error_on_a_private_round_counts_as_a_failed_run() {
    let (input, _, ih, signers, _) = fixture();
    let out = serde_json::json!({"v": 3, "rd": 40, "ih": ih, "o": "X", "rc": 8, "t": 1, "s": [], "j": []}).to_string();
    assert_eq!(failed(check(&input, &ih, &out, &signers)), "BOX_ERROR_8");
}
