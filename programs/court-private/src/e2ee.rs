//! NEAR AI Cloud end-to-end encryption, protocol version 2, from the published guide:
//! X25519 key exchange against the model's Ed25519 key (converted), HKDF-SHA256 with info
//! "ed25519_encryption", XChaCha20-Poly1305. Wire format: ephemeral public key (32 bytes),
//! nonce (24 bytes), ciphertext with tag. Randomness is passed in so tests are exact.
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::{Digest, Sha256, Sha512};
use x25519_dalek::{PublicKey, StaticSecret};

const INFO: &[u8] = b"ed25519_encryption";

fn key_from_shared(shared: &[u8; 32]) -> [u8; 32] {
    let mut k = [0u8; 32];
    Hkdf::<Sha256>::new(None, shared).expand(INFO, &mut k).expect("32 bytes is a valid length");
    k
}

/// Ed25519 public key to X25519 public key, the same conversion libsodium uses.
pub fn ed_pub_to_x(ed: &[u8; 32]) -> Result<[u8; 32], String> {
    let vk = ed25519_dalek::VerifyingKey::from_bytes(ed).map_err(|_| "bad ed25519 key")?;
    Ok(vk.to_montgomery().to_bytes())
}

/// Ed25519 seed to X25519 secret (first half of sha512(seed), clamped by the library).
pub fn ed_seed_to_x(seed: &[u8; 32]) -> [u8; 32] {
    let h = Sha512::digest(seed);
    let mut o = [0u8; 32];
    o.copy_from_slice(&h[..32]);
    o
}

pub fn encrypt_v2(plain: &[u8], model_ed_pub: &[u8; 32], eph_secret: [u8; 32], nonce: [u8; 24]) -> Result<Vec<u8>, String> {
    let recipient = PublicKey::from(ed_pub_to_x(model_ed_pub)?);
    let eph = StaticSecret::from(eph_secret);
    let shared = eph.diffie_hellman(&recipient);
    let key = key_from_shared(shared.as_bytes());
    let ct = XChaCha20Poly1305::new(&key.into())
        .encrypt(XNonce::from_slice(&nonce), plain)
        .map_err(|_| "encrypt failed")?;
    let mut out = PublicKey::from(&eph).as_bytes().to_vec();
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt with an X25519 secret (client side: `ed_seed_to_x(client_seed)`).
pub fn decrypt_v2(data: &[u8], x_secret: [u8; 32]) -> Result<Vec<u8>, String> {
    if data.len() < 32 + 24 + 16 {
        return Err("encrypted field is too short".into());
    }
    let eph: [u8; 32] = data[..32].try_into().unwrap();
    let shared = StaticSecret::from(x_secret).diffie_hellman(&PublicKey::from(eph));
    let key = key_from_shared(shared.as_bytes());
    XChaCha20Poly1305::new(&key.into())
        .decrypt(XNonce::from_slice(&data[32..56]), &data[56..])
        .map_err(|_| "decrypt failed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn model_can_read_what_the_client_sent() {
        let model = SigningKey::from_bytes(&[3u8; 32]);
        let model_pub = model.verifying_key().to_bytes();
        let wire = encrypt_v2(b"Sentence: the sky is blue", &model_pub, [9u8; 32], [5u8; 24]).unwrap();
        assert!(!wire.windows(8).any(|w| w == b"Sentence"), "plain text must not appear on the wire");
        assert_eq!(decrypt_v2(&wire, ed_seed_to_x(&[3u8; 32])).unwrap(), b"Sentence: the sky is blue");
    }

    #[test]
    fn another_key_cannot_read_it_and_tampering_is_caught() {
        let model_pub = SigningKey::from_bytes(&[3u8; 32]).verifying_key().to_bytes();
        let wire = encrypt_v2(b"secret", &model_pub, [9u8; 32], [5u8; 24]).unwrap();
        assert!(decrypt_v2(&wire, ed_seed_to_x(&[4u8; 32])).is_err());
        let mut bad = wire.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert!(decrypt_v2(&bad, ed_seed_to_x(&[3u8; 32])).is_err());
    }

    #[test]
    fn same_inputs_same_bytes_and_new_ephemeral_key_changes_them() {
        let model_pub = SigningKey::from_bytes(&[3u8; 32]).verifying_key().to_bytes();
        let a = encrypt_v2(b"x", &model_pub, [9u8; 32], [5u8; 24]).unwrap();
        assert_eq!(a, encrypt_v2(b"x", &model_pub, [9u8; 32], [5u8; 24]).unwrap());
        assert_ne!(a, encrypt_v2(b"x", &model_pub, [10u8; 32], [5u8; 24]).unwrap());
    }
}
