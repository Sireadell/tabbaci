//! Payout rule for bonded sentences.
//!
//! A writer puts the same fixed stake B on every sentence and picks a word that means a chance:
//! is 95%, very likely 80%, likely 65%, disputed 50%.
//! When the sentence settles true or false the writer gets back
//!     B + B x ( 1/4 - (outcome - chance)^2 )
//! which is the stake plus B x (score of the word - score of "disputed") under the Brier score.
//! "Disputed" therefore always returns exactly B. A bold word wins up to B/4 when right and loses
//! up to 3B/4 when wrong. Gains are paid from the losses of other writers and a reserve
//! (failed challenger bonds, funders). If the pool is short, gains are scaled down together.
//! Every other ruling (unsure, disputed by judges, void) returns every stake in full.
//!
//! All maths is whole numbers: chances are in basis points (10000 = 100%), money is a u128 count of
//! the smallest unit. Gains round down, losses round up, so the contract never pays more than it holds.
use sha2::{Digest, Sha256};

pub const BPS: i128 = 10_000;
/// (word, chance in basis points)
pub const WORDS: [(&str, u16); 4] = [("is", 9500), ("very likely", 8000), ("likely", 6500), ("disputed", 5000)];

pub fn word_bps(word: &str) -> Option<u16> {
    WORDS.iter().find(|(w, _)| *w == word).map(|(_, p)| *p)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Outcome {
    True,
    False,
}

/// Net gain (positive) or loss (negative) against the stake, in the smallest money unit.
/// net = stake x (2500 - d^2 / 10000) / 10000 where d = |outcome x 10000 - chance| in basis points.
pub fn net(stake: u128, chance_bps: u16, outcome: Outcome) -> i128 {
    assert!(chance_bps as i128 <= BPS);
    let target = if outcome == Outcome::True { BPS } else { 0 };
    let d = (target - chance_bps as i128).abs();
    // scaled by 1e8: quarter = 25_000_000, d^2 up to 100_000_000
    let score = 25_000_000i128 - d * d;
    let num = stake as i128 * score;
    let den = 100_000_000i128;
    if num >= 0 {
        num / den // gains round down
    } else {
        -((-num + den - 1) / den) // losses round up
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub stake: u128,
    pub chance_bps: u16,
}

#[derive(Debug, PartialEq)]
pub struct Settlement {
    pub payouts: Vec<u128>,
    pub reserve_after: u128,
}

/// Settle every bonded sentence of one ruling.
/// `outcome` is None for unsure, disputed or void: everyone gets their stake back.
pub fn settle(entries: &[Entry], outcome: Option<Outcome>, reserve: u128) -> Settlement {
    let total_stake: u128 = entries.iter().map(|e| e.stake).sum();
    let Some(o) = outcome else {
        return Settlement { payouts: entries.iter().map(|e| e.stake).collect(), reserve_after: reserve };
    };
    let nets: Vec<i128> = entries.iter().map(|e| net(e.stake, e.chance_bps, o)).collect();
    let losses: u128 = nets.iter().filter(|n| **n < 0).map(|n| (-*n) as u128).sum();
    let gains: u128 = nets.iter().filter(|n| **n > 0).map(|n| *n as u128).sum();
    let pool = losses + reserve;
    let scale_num = gains.min(pool);
    let payouts: Vec<u128> = entries
        .iter()
        .zip(&nets)
        .map(|(e, n)| {
            if *n >= 0 {
                let g = *n as u128;
                let paid = if gains <= pool { g } else { mul_div(g, scale_num, gains) };
                e.stake + paid
            } else {
                e.stake - ((-*n) as u128)
            }
        })
        .collect();
    let paid_total: u128 = payouts.iter().sum();
    // Whatever is not paid out stays in the reserve, so no unit is ever lost.
    let reserve_after = reserve + total_stake - paid_total;
    Settlement { payouts, reserve_after }
}

fn mul_div(a: u128, b: u128, c: u128) -> u128 {
    // a x b / c with the product held in 256 bits, rounded down. Needs c > 0 and a result below 2^128.
    assert!(c > 0, "division by zero");
    let m = u64::MAX as u128;
    let (a_hi, a_lo) = (a >> 64, a & m);
    let (b_hi, b_lo) = (b >> 64, b & m);
    let ll = a_lo * b_lo;
    let lh = a_lo * b_hi;
    let hl = a_hi * b_lo;
    let hh = a_hi * b_hi;
    let mid = (ll >> 64) + (lh & m) + (hl & m);
    let lo = (ll & m) | (mid << 64);
    let hi = hh + (lh >> 64) + (hl >> 64) + (mid >> 64);
    assert!(hi < c, "result does not fit");
    let mut rem = hi;
    let mut q: u128 = 0;
    for i in (0..128).rev() {
        let carry = rem >> 127;
        rem = (rem << 1) | ((lo >> i) & 1);
        q <<= 1;
        if carry == 1 || rem >= c {
            rem = rem.wrapping_sub(c);
            q |= 1;
        }
    }
    q
}

/// Random audit: drawn from a random seed, the sentence and the round only.
/// The writer's word is not an input, so word choice cannot change the chance of being audited.
pub fn audit_drawn(random_seed: &[u8; 32], sentence_id: u64, round: u64, rate_bps: u16) -> bool {
    let mut h = Sha256::new();
    h.update(random_seed);
    h.update(sentence_id.to_be_bytes());
    h.update(round.to_be_bytes());
    let d = h.finalize();
    let v = u32::from_be_bytes([d[0], d[1], d[2], d[3]]) % 10_000;
    (v as u16) < rate_bps
}

/// Expected net per unit of stake for a writer whose true belief is `belief_bps`, writing chance `chance_bps`.
/// Used by tests, in floating point.
pub fn expected_net_f64(belief_bps: u16, chance_bps: u16) -> f64 {
    let q = belief_bps as f64 / 10_000.0;
    let p = chance_bps as f64 / 10_000.0;
    q * (0.25 - (1.0 - p).powi(2)) + (1.0 - q) * (0.25 - p.powi(2))
}
