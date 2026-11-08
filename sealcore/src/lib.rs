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

/// Scramble or unscramble free text (a writer's page) with a pad stream taken from the shared
/// point (big-endian bytes). Block i of the stream is keccak("words-v1" || shared || i).
/// The same call scrambles and unscrambles.
pub fn words_xor(shared_be: &Point, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for (i, chunk) in data.chunks(32).enumerate() {
        let mut v = Vec::with_capacity(8 + 64 + 8);
        v.extend_from_slice(b"words-v1");
        v.extend_from_slice(shared_be);
        v.extend_from_slice(&(i as u64).to_be_bytes());
        let pad = keccak(&v);
        for (j, b) in chunk.iter().enumerate() {
            out.push(b ^ pad[j]);
        }
    }
    out
}

/// Seal a page of text to the batch lock. Returns (C1 little-endian, scrambled text, keccak of the plain text).
pub fn seal_words(lock_be_bytes: &Point, r: &Fr, text: &[u8]) -> (Point, Vec<u8>, [u8; 32]) {
    let lock = affine_from_be(lock_be_bytes).expect("lock is not a valid point");
    let c1 = affine_to_be(G1::one() * *r).expect("r is not zero");
    let shared = affine_to_be(lock * *r).expect("shared point");
    (point_be_to_le(&c1), words_xor(&shared, text), keccak(text))
}

/// Open a sealed page with the batch key.
pub fn open_words(k: &Fr, c1_le: &Point, ct: &[u8]) -> Option<Vec<u8>> {
    let c1 = affine_from_be(&point_le_to_be(c1_le))?;
    let shared = affine_to_be(c1 * *k)?;
    Some(words_xor(&shared, ct))
}

// ---- Viewing keys -------------------------------------------------------------------------
// Plain words: the trader who sealed an order knows the secret number r behind C1 = r*G.
// Handing r over would let anyone forge nothing but would also expose the trader's other
// seals made with the same r, so instead the trader hands over only S = r*lock (the shared
// point that unlocks this one seal) plus a proof that S really comes from the same r as C1.
// Without the proof a trader could grind for a fake S that reads as a different order.

fn wide_challenge(parts: &[&[u8]]) -> Fr {
    let mut wide = [0u8; 64];
    for (i, tag) in [b"A", b"B"].iter().enumerate() {
        let mut v = Vec::new();
        v.extend_from_slice(b"view-proof-v1");
        v.extend_from_slice(*tag);
        for p in parts {
            v.extend_from_slice(p);
        }
        wide[i * 32..(i + 1) * 32].copy_from_slice(&keccak(&v));
    }
    Fr::interpret(&wide)
}

fn g_be() -> Point {
    affine_to_be(G1::one()).unwrap()
}

/// What the trader hands to an auditor: the shared point S (big-endian) and the proof (e, z).
pub struct ViewGrant {
    pub shared_be: Point,
    pub e_be: [u8; 32],
    pub z_be: [u8; 32],
}

/// `s` is a fresh random number for the proof. `r` is the trader's secret from sealing.
pub fn view_grant(lock_be_bytes: &Point, r: &Fr, s: &Fr) -> ViewGrant {
    let lock = affine_from_be(lock_be_bytes).expect("lock is not a valid point");
    let c1 = affine_to_be(G1::one() * *r).unwrap();
    let shared = affine_to_be(lock * *r).expect("shared point");
    let a1 = affine_to_be(G1::one() * *s).unwrap();
    let a2 = affine_to_be(lock * *s).unwrap();
    let e = wide_challenge(&[&g_be(), lock_be_bytes, &c1, &shared, &a1, &a2]);
    let z = *s + e * *r;
    ViewGrant { shared_be: shared, e_be: scalar_to_be(&e), z_be: scalar_to_be(&z) }
}

fn fr_from_be(b: &[u8; 32]) -> Option<Fr> {
    Fr::new(substrate_bn::arith::U256::from_slice(b).ok()?)
}

/// Auditor side: true only if S has the same secret behind it as C1 (given the lock).
pub fn view_check(lock_be_bytes: &Point, c1_le: &Point, g: &ViewGrant) -> bool {
    let (Some(lock), Some(c1), Some(sh)) = (
        affine_from_be(lock_be_bytes),
        affine_from_be(&point_le_to_be(c1_le)),
        affine_from_be(&g.shared_be),
    ) else {
        return false;
    };
    let (Some(e), Some(z)) = (fr_from_be(&g.e_be), fr_from_be(&g.z_be)) else {
        return false;
    };
    let a1 = G1::one() * z - c1 * e;
    let a2 = lock * z - sh * e;
    let (Some(a1b), Some(a2b)) = (affine_to_be(a1), affine_to_be(a2)) else {
        return false;
    };
    let c1b = point_le_to_be(c1_le);
    wide_challenge(&[&g_be(), lock_be_bytes, &c1b, &g.shared_be, &a1b, &a2b]) == e
}

/// Read a sealed 32-byte order using S instead of the batch key (only after `view_check` passed).
pub fn open_order_with_shared(shared_be: &Point, ct: &[u8; 32]) -> [u8; 32] {
    let pad = pad_from_shared(shared_be);
    let mut o = [0u8; 32];
    for i in 0..32 {
        o[i] = ct[i] ^ pad[i];
    }
    o
}

#[cfg(test)]
mod view_tests {
    use super::*;
    #[test]
    fn grant_checks_out_and_reads_the_order() {
        let k = batch_scalar(b"master", 21);
        let lock = lock_be(&k);
        let r = batch_scalar(b"trader", 1);
        let s = batch_scalar(b"proof", 1);
        let mut order = [0u8; 32];
        order[0] = 1;
        order[1] = 1;
        order[2] = 0x19;
        order[3] = 0x64;
        let (c1, ct) = seal(&lock, &r, &order);
        let g = view_grant(&lock, &r, &s);
        assert!(view_check(&lock, &c1, &g));
        assert_eq!(open_order_with_shared(&g.shared_be, &ct), order);
    }
    #[test]
    fn a_fake_shared_point_fails() {
        let k = batch_scalar(b"master", 21);
        let lock = lock_be(&k);
        let r = batch_scalar(b"trader", 1);
        let (c1, _ct) = seal(&lock, &r, &[0u8; 32]);
        let other = batch_scalar(b"trader", 2);
        let mut g = view_grant(&lock, &r, &batch_scalar(b"proof", 1));
        // swap in a shared point made from a different secret
        g.shared_be = view_grant(&lock, &other, &batch_scalar(b"proof", 1)).shared_be;
        assert!(!view_check(&lock, &c1, &g));
        // a grant made for another seal does not fit this one
        let g2 = view_grant(&lock, &other, &batch_scalar(b"proof", 3));
        assert!(!view_check(&lock, &c1, &g2));
    }
}

#[cfg(test)]
mod words_tests {
    use super::*;
    #[test]
    fn words_round_trip_and_hash() {
        let k = batch_scalar(b"master", 9);
        let lock = lock_be(&k);
        let r = batch_scalar(b"writer-random", 2);
        let text = b"The sky was blue on 3 March 2026. This sentence is longer than one 32 byte block so the stream must move on.";
        let (c1, ct, h) = seal_words(&lock, &r, text);
        assert_ne!(&ct[..], &text[..]);
        assert_eq!(ct.len(), text.len());
        assert_eq!(open_words(&k, &c1, &ct).unwrap(), text.to_vec());
        assert_eq!(keccak(&open_words(&k, &c1, &ct).unwrap()), h);
        let wrong = batch_scalar(b"master", 10);
        assert_ne!(open_words(&wrong, &c1, &ct).unwrap(), text.to_vec());
    }
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
