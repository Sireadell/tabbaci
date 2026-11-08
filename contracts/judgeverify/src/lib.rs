//! Lets a contract re-check a NEAR AI judge signature itself, with NEAR's built-in ed25519 check.
//!
//! A provider_tee signature covers the text `<model>:<sha256 of request bytes>:<sha256 of
//! response bytes>`. The contract rebuilds that text from the model name and the two hashes it
//! holds, and asks the host whether the signer's key signed it. The signer's key must also be one
//! the contract trusts (a model report that passed the nonce-fresh checks in programs/court).
//!
//! This is a standalone helper. The courtroom contract does not call it yet.
use near_sdk::env;

/// The exact text the judge signs.
pub fn signed_text(model: &str, request_hash_hex: &str, response_hash_hex: &str) -> String {
    format!("{}:{}:{}", model, request_hash_hex, response_hash_hex)
}

/// True when `signature` (64 bytes) by `signer` (32 byte key) covers the text above.
pub fn judge_signature_ok(
    signer: &[u8; 32],
    signature: &[u8; 64],
    model: &str,
    request_hash_hex: &str,
    response_hash_hex: &str,
) -> bool {
    let text = signed_text(model, request_hash_hex, response_hash_hex);
    env::ed25519_verify(signature, text.as_bytes(), signer)
}

/// Same, but only when the signer is one of the keys the contract already trusts.
pub fn trusted_judge_signature_ok(
    trusted: &[[u8; 32]],
    signer: &[u8; 32],
    signature: &[u8; 64],
    model: &str,
    request_hash_hex: &str,
    response_hash_hex: &str,
) -> bool {
    trusted.contains(signer) && judge_signature_ok(signer, signature, model, request_hash_hex, response_hash_hex)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use sha2::{Digest, Sha256};

    fn h(b: &[u8]) -> String {
        Sha256::digest(b).iter().map(|x| format!("{:02x}", x)).collect()
    }

    fn sign(sk: &SigningKey, model: &str, rq: &str, rs: &str) -> [u8; 64] {
        sk.sign(signed_text(model, rq, rs).as_bytes()).to_bytes()
    }

    #[test]
    fn a_genuine_signature_passes_and_costs_little_gas() {
        near_sdk::testing_env!(near_sdk::test_utils::VMContextBuilder::new().build());
        let sk = SigningKey::from_bytes(&[5u8; 32]);
        let (rq, rs) = (h(b"request"), h(b"response"));
        let sig = sign(&sk, "m/one", &rq, &rs);
        let pk = sk.verifying_key().to_bytes();
        let before = env::used_gas().as_gas();
        assert!(judge_signature_ok(&pk, &sig, "m/one", &rq, &rs));
        let used = env::used_gas().as_gas() - before;
        println!("GAS ed25519 check: {:.3} Tgas", used as f64 / 1e12);
        assert!(used < 1_000_000_000_000, "over 1 Tgas");
    }

    #[test]
    fn changed_model_hash_or_signature_fails() {
        near_sdk::testing_env!(near_sdk::test_utils::VMContextBuilder::new().build());
        let sk = SigningKey::from_bytes(&[5u8; 32]);
        let (rq, rs) = (h(b"request"), h(b"response"));
        let sig = sign(&sk, "m/one", &rq, &rs);
        let pk = sk.verifying_key().to_bytes();
        assert!(!judge_signature_ok(&pk, &sig, "m/two", &rq, &rs));
        assert!(!judge_signature_ok(&pk, &sig, "m/one", &h(b"other"), &rs));
        assert!(!judge_signature_ok(&pk, &sig, "m/one", &rq, &h(b"other")));
        let mut bad = sig;
        bad[0] ^= 1;
        assert!(!judge_signature_ok(&pk, &bad, "m/one", &rq, &rs));
    }

    #[test]
    fn a_valid_signature_from_an_untrusted_key_is_refused() {
        near_sdk::testing_env!(near_sdk::test_utils::VMContextBuilder::new().build());
        let good = SigningKey::from_bytes(&[5u8; 32]);
        let stranger = SigningKey::from_bytes(&[6u8; 32]);
        let (rq, rs) = (h(b"request"), h(b"response"));
        let trusted = [good.verifying_key().to_bytes()];
        let sig_good = sign(&good, "m/one", &rq, &rs);
        let sig_stranger = sign(&stranger, "m/one", &rq, &rs);
        assert!(trusted_judge_signature_ok(&trusted, &trusted[0], &sig_good, "m/one", &rq, &rs));
        let sp = stranger.verifying_key().to_bytes();
        assert!(judge_signature_ok(&sp, &sig_stranger, "m/one", &rq, &rs));
        assert!(!trusted_judge_signature_ok(&trusted, &sp, &sig_stranger, "m/one", &rq, &rs));
    }
}
