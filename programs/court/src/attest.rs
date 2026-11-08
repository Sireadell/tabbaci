//! Checks on NEAR AI Cloud model reports and response signatures, written from the published
//! verification pages (docs.near.ai, "NEAR model attestations", "quote-nonce-signer" and
//! "response signatures"). Pure functions, no network, so every rule has a test.
//!
//! What this file checks on each model report:
//!   1. the echoed nonce is the one we sent
//!   2. the quote is a TDX version 4 quote and debug mode is off
//!   3. the quote's report data is the signer identity (zero padded to 32 bytes) then the nonce
//!   4. the quote's config id is 01, sha256(app_compose), then 15 zero bytes
//!   5. replaying the runtime event log gives the quote's RTMR3
//! What it does NOT check: Intel's signature chain on the quote itself (a collateral lookup that
//! needs the network). `Checked::chain_checked` stays false until that is done, and callers must
//! print it as CANNOT VERIFY.
use crate::util::sha256_hex;
use serde_json::Value;
use sha2::{Digest, Sha256, Sha384};

const QUOTE_HEADER: usize = 48;
const QUOTE_BODY: usize = 584;
const TD_ATTRIBUTES: usize = QUOTE_HEADER + 120;
const MRCONFIGID: usize = QUOTE_HEADER + 184;
const RTMR3: usize = QUOTE_HEADER + 472;
const REPORT_DATA: usize = QUOTE_HEADER + 520;

/// The freshness number for a round: the sha256 of the exact input bytes, as 64 hex characters.
/// It is different for every round and anyone can recompute it from the stored input.
pub fn round_nonce(input_bytes: &[u8]) -> String {
    sha256_hex(input_bytes)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    pub model: String,
    pub signing_algo: String,
    /// lowercase hex, no 0x
    pub signing_address: String,
    pub signing_public_key: String,
    pub rtmr3: String,
    /// Intel's signature chain was not verified here
    pub chain_checked: bool,
}

fn s<'a>(v: &'a Value, k: &str) -> Result<&'a str, String> {
    v.get(k).and_then(|x| x.as_str()).ok_or_else(|| format!("report has no {k}"))
}

fn json_or_text(v: &Value) -> Result<Value, String> {
    match v {
        Value::String(t) => serde_json::from_str(t).map_err(|e| format!("bad json text: {e}")),
        other => Ok(other.clone()),
    }
}

/// Replay the runtime event log (events with imr 3, in order) into an RTMR3 value.
pub fn replay_rtmr3(event_log: &Value) -> Result<[u8; 48], String> {
    let log = json_or_text(event_log)?;
    let events = log.as_array().ok_or("event log is not a list")?;
    let mut rtmr = [0u8; 48];
    for e in events {
        if e.get("imr").and_then(|x| x.as_u64()) != Some(3) {
            continue;
        }
        let et = e.get("event_type").and_then(|x| x.as_u64()).ok_or("event without type")? as u32;
        let name = e.get("event").and_then(|x| x.as_str()).ok_or("event without name")?;
        let payload = hex::decode(e.get("event_payload").and_then(|x| x.as_str()).unwrap_or(""))
            .map_err(|_| "bad event payload")?;
        let mut h = Sha384::new();
        h.update(et.to_le_bytes());
        h.update(b":");
        h.update(name.as_bytes());
        h.update(b":");
        h.update(&payload);
        let digest: [u8; 48] = h.finalize().into();
        let given = e.get("digest").and_then(|x| x.as_str()).unwrap_or("");
        if !given.is_empty() && hex::decode(given).map_err(|_| "bad digest")? != digest {
            return Err(format!("digest mismatch for event {name}"));
        }
        let mut n = Sha384::new();
        n.update(rtmr);
        n.update(digest);
        rtmr = n.finalize().into();
    }
    Ok(rtmr)
}

fn identity(report: &Value) -> Result<(String, Vec<u8>), String> {
    let algo = s(report, "signing_algo")?.to_string();
    let addr = hex::decode(s(report, "signing_address")?.trim_start_matches("0x")).map_err(|_| "bad signing_address")?;
    let want = match algo.as_str() {
        "ecdsa" => 20,
        "ed25519" => 32,
        _ => return Err("unsupported signing algorithm".into()),
    };
    if addr.len() != want {
        return Err("signing address has the wrong length".into());
    }
    Ok((algo, addr))
}

/// Run every check above on one model report. `nonce_hex` is the 64 character number we sent.
pub fn check_report(report: &Value, nonce_hex: &str) -> Result<Checked, String> {
    let nonce = hex::decode(nonce_hex).map_err(|_| "nonce is not hex")?;
    if nonce.len() != 32 {
        return Err("nonce must be 32 bytes".into());
    }
    let (algo, addr) = identity(report)?;
    if !s(report, "request_nonce")?.eq_ignore_ascii_case(nonce_hex) {
        return Err("echoed nonce is not the one we sent".into());
    }
    let quote = hex::decode(s(report, "intel_quote")?).map_err(|_| "quote is not hex")?;
    if quote.len() < QUOTE_HEADER + QUOTE_BODY {
        return Err("quote is too short".into());
    }
    if u16::from_le_bytes([quote[0], quote[1]]) != 4 || u32::from_le_bytes(quote[4..8].try_into().unwrap()) != 0x81 {
        return Err("not a TDX version 4 quote".into());
    }
    if quote[TD_ATTRIBUTES] & 1 == 1 {
        return Err("TDX debug mode is on".into());
    }
    let mut bound = addr.clone();
    bound.resize(32, 0);
    bound.extend_from_slice(&nonce);
    if quote[REPORT_DATA..REPORT_DATA + 64] != bound[..] {
        return Err("quote report data does not bind this signer and nonce".into());
    }
    let info = report.get("info").ok_or("report has no info")?;
    let tcb = json_or_text(info.get("tcb_info").ok_or("report has no tcb_info")?)?;
    let compose = tcb.get("app_compose").and_then(|x| x.as_str()).ok_or("tcb_info has no app_compose")?;
    let mut want_cfg = vec![1u8];
    want_cfg.extend_from_slice(&Sha256::digest(compose.as_bytes()));
    want_cfg.resize(48, 0);
    if quote[MRCONFIGID..MRCONFIGID + 48] != want_cfg[..] {
        return Err("quote config id does not match app_compose".into());
    }
    let rt = replay_rtmr3(report.get("event_log").ok_or("report has no event_log")?)?;
    if quote[RTMR3..RTMR3 + 48] != rt[..] {
        return Err("event log does not replay to the quote's RTMR3".into());
    }
    Ok(Checked {
        model: s(report, "model_name").unwrap_or("").to_string(),
        signing_algo: algo,
        signing_address: hex::encode(addr),
        signing_public_key: report.get("signing_public_key").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        rtmr3: hex::encode(rt),
        chain_checked: false,
    })
}

/// Check every candidate. Any candidate that fails makes the whole set unusable, as the docs say
/// ("verify every returned candidate"). An empty or missing list is also a failure.
pub fn check_all(reports: &Value, nonce_hex: &str) -> Result<Vec<Checked>, String> {
    let list = reports.as_array().ok_or("no model_attestations")?;
    if list.is_empty() {
        return Err("model_attestations is empty".into());
    }
    list.iter().map(|r| check_report(r, nonce_hex)).collect()
}

/// A provider_tee signature is accepted only when its signer and algorithm match exactly one
/// checked report.
pub fn match_signer<'a>(checked: &'a [Checked], algo: &str, address: &str) -> Result<&'a Checked, String> {
    let address = address.trim_start_matches("0x").to_ascii_lowercase();
    let hits: Vec<&Checked> =
        checked.iter().filter(|c| c.signing_algo == algo && c.signing_address == address).collect();
    match hits.len() {
        1 => Ok(hits[0]),
        0 => Err("signer matches no checked report".into()),
        _ => Err("signer matches more than one checked report".into()),
    }
}

/// The text a provider_tee signature must carry.
pub fn expected_text(model: &str, request: &[u8], response: &[u8]) -> String {
    format!("{}:{}:{}", model, sha256_hex(request), sha256_hex(response))
}

/// Verify the JSON from GET /v1/signature/<id>. Returns the algorithm and signer on success.
pub fn verify_signature(sig: &Value, expected: &str) -> Result<(String, String), String> {
    if sig.get("text").and_then(|x| x.as_str()) != Some(expected) {
        return Err("signature text does not match the exact request and response bytes".into());
    }
    let (algo, addr) = identity(sig)?;
    let bytes = hex::decode(s(sig, "signature")?.trim_start_matches("0x")).map_err(|_| "signature is not hex")?;
    match algo.as_str() {
        "ed25519" => {
            let pk: [u8; 32] = addr.clone().try_into().map_err(|_| "bad key")?;
            let vk = ed25519_dalek::VerifyingKey::from_bytes(&pk).map_err(|_| "bad ed25519 key")?;
            let sg: [u8; 64] = bytes.try_into().map_err(|_| "ed25519 signature must be 64 bytes")?;
            vk.verify_strict(expected.as_bytes(), &ed25519_dalek::Signature::from_bytes(&sg))
                .map_err(|_| "ed25519 signature does not verify")?;
        }
        _ => {
            let a: [u8; 20] = addr.clone().try_into().map_err(|_| "bad address")?;
            if !crate::sig::verify(expected, &bytes, &a) {
                return Err("ecdsa signature does not verify".into());
            }
        }
    }
    Ok((algo, hex::encode(addr)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn fixture() -> (Value, String) {
        let v: Value = serde_json::from_str(include_str!("../testdata/model_report.json")).unwrap();
        (v["report"].clone(), v["nonce"].as_str().unwrap().to_string())
    }

    #[test]
    fn a_real_report_passes_every_check() {
        let (r, n) = fixture();
        let c = check_report(&r, &n).unwrap();
        assert_eq!(c.signing_algo, "ed25519");
        assert_eq!(c.signing_address.len(), 64);
        assert!(!c.chain_checked);
    }

    #[test]
    fn a_different_nonce_is_refused() {
        let (r, _) = fixture();
        let other = "11".repeat(32);
        assert!(check_report(&r, &other).unwrap_err().contains("nonce"));
    }

    #[test]
    fn a_replayed_report_for_another_round_is_refused() {
        let (r, n) = fixture();
        let mut r2 = r.clone();
        r2["request_nonce"] = Value::String(round_nonce(b"another round"));
        assert!(check_report(&r2, &n).is_err());
        assert!(check_report(&r, &round_nonce(b"another round")).is_err());
    }

    #[test]
    fn a_swapped_signer_is_refused() {
        let (mut r, n) = fixture();
        r["signing_address"] = Value::String("ab".repeat(32));
        assert!(check_report(&r, &n).unwrap_err().contains("bind"));
    }

    #[test]
    fn a_changed_config_text_is_refused() {
        let (mut r, n) = fixture();
        let mut tcb = json_or_text(&r["info"]["tcb_info"]).unwrap();
        let c = tcb["app_compose"].as_str().unwrap().to_string();
        tcb["app_compose"] = Value::String(format!("{c} "));
        r["info"]["tcb_info"] = tcb;
        assert!(check_report(&r, &n).unwrap_err().contains("app_compose"));
    }

    #[test]
    fn a_tampered_event_log_is_refused() {
        let (mut r, n) = fixture();
        let mut log = json_or_text(&r["event_log"]).unwrap();
        let i = log.as_array().unwrap().iter().position(|e| e["imr"] == 3 && e["event"] == "system-preparing").unwrap();
        log[i]["event"] = Value::String("system-preparing-x".into());
        r["event_log"] = log;
        assert!(check_report(&r, &n).is_err());
    }

    #[test]
    fn debug_mode_is_refused() {
        let (mut r, n) = fixture();
        let mut q = hex::decode(r["intel_quote"].as_str().unwrap()).unwrap();
        q[TD_ATTRIBUTES] |= 1;
        r["intel_quote"] = Value::String(hex::encode(q));
        assert!(check_report(&r, &n).unwrap_err().contains("debug"));
    }

    #[test]
    fn empty_or_missing_candidate_lists_are_refused() {
        assert!(check_all(&Value::Array(vec![]), &"00".repeat(32)).is_err());
        assert!(check_all(&Value::Null, &"00".repeat(32)).is_err());
        let (r, n) = fixture();
        assert_eq!(check_all(&Value::Array(vec![r.clone()]), &n).unwrap().len(), 1);
        let mut bad = r.clone();
        bad["request_nonce"] = Value::String("00".repeat(32));
        assert!(check_all(&Value::Array(vec![r, bad]), &n).is_err(), "one bad candidate spoils the set");
    }

    fn signed(model: &str, req: &[u8], resp: &[u8], sk: &SigningKey) -> Value {
        let text = expected_text(model, req, resp);
        let sig = sk.sign(text.as_bytes());
        serde_json::json!({
            "text": text,
            "signature": hex::encode(sig.to_bytes()),
            "signing_address": hex::encode(sk.verifying_key().to_bytes()),
            "signing_algo": "ed25519",
            "signature_kind": "provider_tee"
        })
    }

    #[test]
    fn ed25519_signature_over_the_exact_bytes_verifies() {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let (req, resp) = (b"{\"a\":1}".as_slice(), b"{\"id\":\"x\"}".as_slice());
        let sig = signed("m/one", req, resp, &sk);
        let (algo, addr) = verify_signature(&sig, &expected_text("m/one", req, resp)).unwrap();
        assert_eq!(algo, "ed25519");
        assert_eq!(addr, hex::encode(sk.verifying_key().to_bytes()));
    }

    #[test]
    fn signature_over_other_bytes_or_other_model_fails() {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let sig = signed("m/one", b"req", b"resp", &sk);
        assert!(verify_signature(&sig, &expected_text("m/one", b"req", b"resp2")).is_err());
        assert!(verify_signature(&sig, &expected_text("m/two", b"req", b"resp")).is_err());
        let mut forged = sig.clone();
        forged["signature"] = Value::String("00".repeat(64));
        assert!(verify_signature(&forged, &expected_text("m/one", b"req", b"resp")).is_err());
        let mut other_key = sig.clone();
        other_key["signing_address"] =
            Value::String(hex::encode(SigningKey::from_bytes(&[8u8; 32]).verifying_key().to_bytes()));
        assert!(verify_signature(&other_key, &expected_text("m/one", b"req", b"resp")).is_err());
    }

    #[test]
    fn signer_must_match_exactly_one_checked_report() {
        let (r, n) = fixture();
        let c = check_report(&r, &n).unwrap();
        let one = vec![c.clone()];
        assert!(match_signer(&one, "ed25519", &c.signing_address).is_ok());
        assert!(match_signer(&one, "ecdsa", &c.signing_address).is_err());
        assert!(match_signer(&one, "ed25519", &"cd".repeat(32)).is_err());
        let two = vec![c.clone(), c.clone()];
        assert!(match_signer(&two, "ed25519", &c.signing_address).unwrap_err().contains("more than one"));
    }

    #[test]
    fn round_nonce_is_per_round() {
        assert_eq!(round_nonce(b"a").len(), 64);
        assert_ne!(round_nonce(b"a"), round_nonce(b"b"));
    }
}
