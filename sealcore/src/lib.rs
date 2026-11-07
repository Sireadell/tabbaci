//! Sealed-order maths on the BN254 curve (the curve NEAR contracts have built-in maths for).
//!
//! Plain words: a batch has a secret number `k`. Its public "lock" is `k * G`.
//! To seal an order, a trader picks a random number `r`, publishes `C1 = r * G`, and
//! scrambles the order with a pad taken from `S = r * lock`. Only someone who knows `k`
//! can compute the same point as `S = k * C1`, so only the key holder can open the order.
//!
//! Byte formats: points are 64 bytes, x then y. `*_be` is big-endian, `*_le` is the
//! little-endian form NEAR's built-in `alt_bn128_g1_multiexp` uses.
use substrate_bn::{AffineG1, Fq, Fr, Group, G1};
use sha2::{Digest as _, Sha256};
use sha3::Keccak256;

pub type Point = [u8; 64];

fn rev32(b: &[u8]) -> [u8; 32] {
    let mut o = [0u8; 32];
    for i in 0..32 {
        o[i] = b[31 - i];
    }
    o
}

pub fn point_be_to_le(p: &Point) -> Point {
    let mut o = [0u8; 64];
    o[..32].copy_from_slice(&rev32(&p[..32]));
    o[32..].copy_from_slice(&rev32(&p[32..]));
    o
}

pub fn point_le_to_be(p: &Point) -> Point {
    point_be_to_le(p)
}

/// Batch secret number from the hidden master secret and the batch id.
/// 64 bytes of hash are reduced mod the group order so the result is uniform.
pub fn batch_scalar(master: &[u8], batch_id: u64) -> Fr {
    let mut wide = [0u8; 64];
    for (i, tag) in [b"A", b"B"].iter().enumerate() {
        let mut h = Sha256::new();
        h.update(b"sealed-batch-v1");
        h.update(tag);
        h.update((master.len() as u64).to_be_bytes());
        h.update(master);
        h.update(batch_id.to_be_bytes());
        wide[i * 32..(i + 1) * 32].copy_from_slice(&h.finalize());
    }
    Fr::interpret(&wide)
}

pub fn scalar_to_be(k: &Fr) -> [u8; 32] {
    // into_u256 gives the plain number (to_big_endian would give the internal form)
    let mut o = [0u8; 32];
    k.into_u256().to_big_endian(&mut o).unwrap();
    o
}

pub fn scalar_to_le(k: &Fr) -> [u8; 32] {
    rev32(&scalar_to_be(k))
}

pub fn scalar_from_le(b: &[u8; 32]) -> Option<Fr> {
    let u = substrate_bn::arith::U256::from_slice(&rev32(b)).ok()?;
    Fr::new(u)
}

fn affine_to_be(p: G1) -> Option<Point> {
    let a = AffineG1::from_jacobian(p)?;
    let mut o = [0u8; 64];
    a.x().to_big_endian(&mut o[..32]).unwrap();
    a.y().to_big_endian(&mut o[32..]).unwrap();
    Some(o)
}

fn affine_from_be(p: &Point) -> Option<G1> {
    let x = Fq::from_slice(&p[..32]).ok()?;
    let y = Fq::from_slice(&p[32..]).ok()?;
    AffineG1::new(x, y).ok().map(Into::into)
}

/// k * G, in big-endian bytes.
pub fn lock_be(k: &Fr) -> Point {
    affine_to_be(G1::one() * *k).expect("k is not zero")
}

pub fn lock_le(k: &Fr) -> Point {
    point_be_to_le(&lock_be(k))
}

pub fn keccak(data: &[u8]) -> [u8; 32] {
    let mut h = Keccak256::new();
    h.update(data);
    h.finalize().into()
}

/// The pad used to scramble a 32-byte order, from the shared point (big-endian bytes).
pub fn pad_from_shared(shared_be: &Point) -> [u8; 32] {
    let mut v = Vec::with_capacity(64 + 8);
    v.extend_from_slice(b"order-v1");
    v.extend_from_slice(shared_be);
    keccak(&v)
}

/// Seal a 32-byte order. Returns (C1 little-endian for NEAR, scrambled order).
pub fn seal(lock_be_bytes: &Point, r: &Fr, order: &[u8; 32]) -> (Point, [u8; 32]) {
    let lock = affine_from_be(lock_be_bytes).expect("lock is not a valid point");
    let c1 = affine_to_be(G1::one() * *r).expect("r is not zero");
    let shared = affine_to_be(lock * *r).expect("shared point");
    let pad = pad_from_shared(&shared);
    let mut ct = [0u8; 32];
    for i in 0..32 {
        ct[i] = order[i] ^ pad[i];
    }
    (point_be_to_le(&c1), ct)
}

/// Open a sealed order with the batch key (reference version, used by tests and the checker).
pub fn open(k: &Fr, c1_le: &Point, ct: &[u8; 32]) -> Option<[u8; 32]> {
    let c1 = affine_from_be(&point_le_to_be(c1_le))?;
    let shared = affine_to_be(c1 * *k)?;
    let pad = pad_from_shared(&shared);
    let mut o = [0u8; 32];
    for i in 0..32 {
        o[i] = ct[i] ^ pad[i];
    }
    Some(o)
}

/// Same as `pad_from_shared`, but takes the shared point as NEAR returns it (little-endian).
pub fn pad_from_shared_le(shared_le: &Point) -> [u8; 32] {
    pad_from_shared(&point_le_to_be(shared_le))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip() {
        let k = batch_scalar(b"master", 7);
        let lock = lock_be(&k);
        let r = batch_scalar(b"trader-random", 1);
        let mut order = [0u8; 32];
        order[0] = 1;
        order[1] = 1;
        let (c1, ct) = seal(&lock, &r, &order);
        assert_ne!(ct, order);
        assert_eq!(open(&k, &c1, &ct).unwrap(), order);
        let wrong = batch_scalar(b"master", 8);
        assert_ne!(open(&wrong, &c1, &ct).unwrap(), order);
    }
    #[test]
    fn same_batch_same_lock() {
        assert_eq!(lock_be(&batch_scalar(b"m", 3)), lock_be(&batch_scalar(b"m", 3)));
        assert_ne!(lock_be(&batch_scalar(b"m", 3)), lock_be(&batch_scalar(b"m", 4)));
    }
}

#[cfg(test)]
mod scalar_tests {
    use super::*;
    #[test]
    fn five_is_five() {
        let five = Fr::from_str("5").unwrap();
        let le = scalar_to_le(&five);
        assert_eq!(le[0], 5, "low byte");
        assert!(le[1..].iter().all(|b| *b == 0));
    }
    #[test]
    fn scalar_round_trip() {
        let k = batch_scalar(b"m", 11);
        let back = scalar_from_le(&scalar_to_le(&k)).unwrap();
        assert!(k == back);
        // key times G must equal the lock the same way the contract computes it
        assert_eq!(lock_be(&k), lock_be(&back));
    }
}
