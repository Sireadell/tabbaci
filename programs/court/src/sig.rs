use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use sha3::{Digest, Keccak256};

fn eip191_hash(text: &str) -> [u8; 32] {
    let mut h = Keccak256::new();
    h.update(format!("\x19Ethereum Signed Message:\n{}", text.len()).as_bytes());
    h.update(text.as_bytes());
    h.finalize().into()
}

pub fn address_of(vk: &VerifyingKey) -> [u8; 20] {
    let pt = vk.to_encoded_point(false);
    let hash = Keccak256::digest(&pt.as_bytes()[1..]);
    let mut a = [0u8; 20];
    a.copy_from_slice(&hash[12..]);
    a
}

/// True when `sig` (65 bytes, r|s|v) over `text` (Ethereum EIP-191) recovers to `addr`.
pub fn verify(text: &str, sig: &[u8], addr: &[u8; 20]) -> bool {
    if sig.len() != 65 {
        return false;
    }
    let v = sig[64];
    let rid = match v {
        0 | 27 => 0,
        1 | 28 => 1,
        _ => return false,
    };
    let Ok(signature) = Signature::from_slice(&sig[..64]) else { return false };
    let Some(rid) = RecoveryId::from_byte(rid) else { return false };
    let Ok(vk) = VerifyingKey::recover_from_prehash(&eip191_hash(text), &signature, rid) else {
        return false;
    };
    &address_of(&vk) == addr
}

#[cfg(any(feature = "standin", test))]
pub mod standin {
    use super::*;
    use k256::ecdsa::SigningKey;

    pub fn key() -> SigningKey {
        SigningKey::from_bytes(&[0x42u8; 32].into()).unwrap()
    }
    pub fn address() -> [u8; 20] {
        address_of(key().verifying_key())
    }
    pub fn sign(text: &str) -> Vec<u8> {
        let (sig, rid) = key().sign_prehash_recoverable(&eip191_hash(text)).unwrap();
        let mut out = sig.to_bytes().to_vec();
        out.push(27 + rid.to_byte());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standin_roundtrip() {
        let s = standin::sign("m:a:b");
        assert!(verify("m:a:b", &s, &standin::address()));
        assert!(!verify("m:a:c", &s, &standin::address()));
        assert!(!verify("m:a:b", &s[..64], &standin::address()));
    }
}
