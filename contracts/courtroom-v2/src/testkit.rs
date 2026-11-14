//! Test helpers: signed stand-in verdicts, built the way the sealed program builds them.
use crate::judging;
use crate::Signer;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{Signer as _, SigningKey};
use near_sdk::serde_json;
use sha2::{Digest, Sha256};

pub fn test_key() -> SigningKey {
    SigningKey::from_bytes(&[9u8; 32])
}

pub fn test_signers(models: &[&str]) -> Vec<Signer> {
    let k = hex::encode(test_key().verifying_key().to_bytes());
    models.iter().map(|m| Signer { model: m.to_string(), key: k.clone(), from_ns: 0 }).collect()
}

pub fn hex_sha(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}

/// One judge's signed response: (response bytes, signature).
pub fn signed_judge(sk: &SigningKey, model: &str, req: &[u8], content: &str) -> (Vec<u8>, Vec<u8>) {
    let resp = serde_json::to_vec(&serde_json::json!({
        "id": "chatcmpl-test", "object": "chat.completion", "model": model,
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}]
    }))
    .unwrap();
    let text = format!("{}:{}:{}", model, hex_sha(req), hex_sha(&resp));
    (resp, sk.sign(text.as_bytes()).to_bytes().to_vec())
}

fn b(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The full verdict the program returns for a number sentence round, for a given ruling letter.
pub fn signed_data_verdict(input: &str, round: u64, ih: &str, letter: &str) -> String {
    let inp: serde_json::Value = serde_json::from_str(input).unwrap();
    let models: Vec<String> = inp["m"].as_array().unwrap().iter().map(|m| m.as_str().unwrap().to_string()).collect();
    let cv: f64 = inp["cv"].as_str().unwrap().parse().unwrap();
    let seed = inp["seed"].as_u64().unwrap();
    let claim = inp["claim"].as_str().unwrap();
    let nsrc = inp["src"].as_array().unwrap().len();
    let (words, num): ([&str; 3], f64) = match letter {
        "F" => (["TRUE", "TRUE", "TRUE"], cv + 1.0),
        "U" => (["TRUE", "UNSURE", "UNSURE"], cv),
        "D" => (["TRUE", "TRUE", "FALSE"], cv),
        _ => (["TRUE", "TRUE", "TRUE"], cv),
    };
    let src: Vec<serde_json::Value> = (0..nsrc)
        .map(|i| {
            let n = if letter == "V" && i == 1 { num + 1.0 } else { num };
            serde_json::json!({"i": i + 1, "c": 200, "h": "h", "x": judging::canon_number(n)})
        })
        .collect();
    if letter == "X" {
        return serde_json::json!({"v": 2, "rd": round, "ih": ih, "o": "X", "rc": 2, "t": 1, "s": [], "j": []}).to_string();
    }
    if letter == "V" {
        return serde_json::json!({"v": 2, "rd": round, "ih": ih, "o": "V", "rc": 1, "t": 1, "s": src, "j": []}).to_string();
    }
    let user = judging::data_user(claim, &judging::canon_number(cv));
    let sk = test_key();
    let j: Vec<serde_json::Value> = (0..3)
        .map(|i| {
            let req = judging::request_bytes(&models[i], judging::SYSTEM, &user, seed, judging::MAX_TOKENS_DATA);
            let (resp, sig) = signed_judge(&sk, &models[i], &req, words[i]);
            let a = match words[i] {
                "TRUE" => "T",
                "FALSE" => "F",
                _ => "U",
            };
            serde_json::json!({"m": i, "a": a, "v": a, "r": b(&resp), "g": b(&sig), "k": b(&sk.verifying_key().to_bytes())})
        })
        .collect();
    serde_json::json!({"v": 2, "rd": round, "ih": ih, "o": letter, "rc": 0, "t": 1, "s": src, "j": j}).to_string()
}
