//! The appeal ladder: pure rules, no blockchain calls, so every money rule is tested on a normal computer.
//!
//! A ruling (level 0) can be challenged up to three times. Each challenge posts a bond that doubles
//! every time, adds at least one more source, and the last rung asks five judges instead of three.
//! A random audit is a free re-run by the contract itself: it posts no bond and uses no rung.
//!
//! Money: when the whole case ends, every challenger who argued against the ruling that did not
//! survive wins; every challenger who argued against the ruling that survived loses their bond.
//! A challenge whose re-run gave no ruling (void, unsure, disputed or timed out) gets the bond back.
//! Half of the lost bonds go to the winners (in proportion to their bonds), half to the reserve.
//! All maths is whole numbers. Nothing is created or lost.

pub const MAX_LEVEL: u8 = 3;
pub const MAX_PICKS: usize = 5;
pub const JUDGES_NORMAL: usize = 3;
pub const JUDGES_TOP: usize = 5;

/// The bond for challenge number `level` (1, 2 or 3): base, 2 x base, 4 x base.
pub fn challenge_bond(base: u128, level: u8) -> Option<u128> {
    if level == 0 || level > MAX_LEVEL {
        return None;
    }
    base.checked_mul(1u128 << (level - 1))
}

/// How many judges a challenge at this level asks.
pub fn judge_count(level: u8) -> usize {
    if level >= MAX_LEVEL {
        JUDGES_TOP
    } else {
        JUDGES_NORMAL
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
    True,
    False,
}

impl Side {
    pub fn from_letter(s: &str) -> Option<Side> {
        match s {
            "T" => Some(Side::True),
            "F" => Some(Side::False),
            _ => None,
        }
    }
    pub fn letter(self) -> &'static str {
        match self {
            Side::True => "T",
            Side::False => "F",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Verdict {
    /// the re-run gave the opposite decisive ruling, so the standing ruling changes
    Flipped,
    /// the re-run gave the same decisive ruling
    Upheld,
    /// the re-run gave no decisive ruling (void, unsure, disputed, timed out)
    NoResult,
}

impl Verdict {
    pub fn word(self) -> &'static str {
        match self {
            Verdict::Flipped => "flipped",
            Verdict::Upheld => "upheld",
            Verdict::NoResult => "no_result",
        }
    }
}

/// What a closed re-run means for the ruling that stands. `outcome` is the round's letter (T, F, U, D) or None for void.
pub fn appeal_verdict(standing: Side, outcome: Option<&str>) -> Verdict {
    match outcome.and_then(Side::from_letter) {
        Some(s) if s != standing => Verdict::Flipped,
        Some(_) => Verdict::Upheld,
        None => Verdict::NoResult,
    }
}

/// One closed re-run, as the settlement sees it. `bond` is 0 for a random audit.
#[derive(Clone, Debug)]
pub struct AppealIn {
    pub bond: u128,
    pub standing_before: Side,
    pub verdict: Verdict,
}

#[derive(Debug, PartialEq)]
pub struct CaseSettlement {
    /// what each appeal's challenger gets back, in the same order as the input
    pub credits: Vec<u128>,
    /// what goes to the reserve
    pub to_reserve: u128,
}

/// Settle every challenger bond of a finished case. `final_side` is the ruling that stands at the end.
pub fn settle_challengers(appeals: &[AppealIn], final_side: Side) -> CaseSettlement {
    let mut forfeited: u128 = 0;
    let mut winners_total: u128 = 0;
    for a in appeals {
        if a.bond == 0 {
            continue;
        }
        match a.verdict {
            Verdict::NoResult => {}
            _ => {
                if final_side != a.standing_before {
                    winners_total += a.bond;
                } else {
                    forfeited += a.bond;
                }
            }
        }
    }
    let winners_share = if winners_total == 0 { 0 } else { forfeited / 2 };
    let mut credits = Vec::with_capacity(appeals.len());
    let mut paid_from_pot: u128 = 0;
    for a in appeals {
        let c = if a.bond == 0 {
            0
        } else if a.verdict == Verdict::NoResult {
            a.bond
        } else if final_side != a.standing_before {
            let extra = mul_div(winners_share, a.bond, winners_total);
            paid_from_pot += extra;
            a.bond + extra
        } else {
            0
        };
        credits.push(c);
    }
    // Rounding dust and the other half of the lost bonds go to the reserve.
    CaseSettlement { credits, to_reserve: forfeited - paid_from_pot }
}

/// a x b / c with no overflow: the product is held in 256 bits. Rounds down. Needs c > 0 and a result that fits in 128 bits.
pub fn mul_div(a: u128, b: u128, c: u128) -> u128 {
    assert!(c > 0, "division by zero");
    // 128 x 128 -> 256 bits as (hi, lo)
    let (a_hi, a_lo) = (a >> 64, a & u64::MAX as u128);
    let (b_hi, b_lo) = (b >> 64, b & u64::MAX as u128);
    let ll = a_lo * b_lo;
    let lh = a_lo * b_hi;
    let hl = a_hi * b_lo;
    let hh = a_hi * b_hi;
    let mid = (ll >> 64) + (lh & u64::MAX as u128) + (hl & u64::MAX as u128);
    let lo = (ll & u64::MAX as u128) | (mid << 64);
    let hi = hh + (lh >> 64) + (hl >> 64) + (mid >> 64);
    assert!(hi < c, "result does not fit in 128 bits");
    // 256 / 128 by shift and subtract
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

/// Does `next` add at least one source to `prev` and keep every earlier one? Both are source ids.
pub fn adds_a_source(prev: &[u32], next: &[u32]) -> Result<(), &'static str> {
    if next.len() > MAX_PICKS {
        return Err("at most 5 sources");
    }
    let mut seen: Vec<u32> = vec![];
    for id in next {
        if seen.contains(id) {
            return Err("a source can only be picked once");
        }
        seen.push(*id);
    }
    if !prev.iter().all(|p| next.contains(p)) {
        return Err("a challenge must keep every earlier source");
    }
    if next.iter().all(|n| prev.contains(n)) {
        return Err("a challenge must add at least one new source");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bond_doubles_and_top_rung_has_five_judges() {
        assert_eq!(challenge_bond(100, 1), Some(100));
        assert_eq!(challenge_bond(100, 2), Some(200));
        assert_eq!(challenge_bond(100, 3), Some(400));
        assert_eq!(challenge_bond(100, 0), None);
        assert_eq!(challenge_bond(100, 4), None);
        assert_eq!(judge_count(1), 3);
        assert_eq!(judge_count(2), 3);
        assert_eq!(judge_count(3), 5);
    }

    #[test]
    fn verdict_words() {
        assert_eq!(appeal_verdict(Side::True, Some("F")), Verdict::Flipped);
        assert_eq!(appeal_verdict(Side::True, Some("T")), Verdict::Upheld);
        for o in [Some("U"), Some("D"), None] {
            assert_eq!(appeal_verdict(Side::True, o), Verdict::NoResult);
        }
    }

    #[test]
    fn mul_div_matches_plain_maths_and_survives_big_numbers() {
        assert_eq!(mul_div(10, 3, 4), 7);
        assert_eq!(mul_div(0, 5, 3), 0);
        // 1e26 x 1e26 / 1e26 would overflow u128 if multiplied first
        let big = 100_000_000_000_000_000_000_000_000u128;
        assert_eq!(mul_div(big, big, big), big);
        assert_eq!(mul_div(big, big / 2, big), big / 2);
        assert_eq!(mul_div(u128::MAX, u128::MAX, u128::MAX), u128::MAX);
        assert_eq!(mul_div(u128::MAX, 1, 2), u128::MAX / 2);
    }

    #[test]
    fn single_failed_challenge_goes_all_to_the_reserve() {
        let s = settle_challengers(&[AppealIn { bond: 100, standing_before: Side::True, verdict: Verdict::Upheld }], Side::True);
        assert_eq!(s, CaseSettlement { credits: vec![0], to_reserve: 100 });
    }

    #[test]
    fn a_flip_that_stands_returns_the_bond() {
        let s = settle_challengers(&[AppealIn { bond: 100, standing_before: Side::True, verdict: Verdict::Flipped }], Side::False);
        assert_eq!(s, CaseSettlement { credits: vec![100], to_reserve: 0 });
    }

    #[test]
    fn flip_then_flip_back_pays_the_last_challenger() {
        // L1 flips T to F (bond 100), L2 flips F back to T (bond 200). Final T.
        let s = settle_challengers(
            &[
                AppealIn { bond: 100, standing_before: Side::True, verdict: Verdict::Flipped },
                AppealIn { bond: 200, standing_before: Side::False, verdict: Verdict::Flipped },
            ],
            Side::True,
        );
        // L1 lost 100. L2 wins: 200 back + half of 100 = 250. Reserve gets the other 50.
        assert_eq!(s, CaseSettlement { credits: vec![0, 250], to_reserve: 50 });
    }

    #[test]
    fn audit_flip_costs_nobody_and_a_no_result_refunds() {
        let s = settle_challengers(
            &[
                AppealIn { bond: 0, standing_before: Side::True, verdict: Verdict::Flipped },
                AppealIn { bond: 100, standing_before: Side::False, verdict: Verdict::NoResult },
            ],
            Side::False,
        );
        assert_eq!(s, CaseSettlement { credits: vec![0, 100], to_reserve: 0 });
    }

    #[test]
    fn source_growth_rules() {
        assert!(adds_a_source(&[1, 2], &[1, 2, 3]).is_ok());
        assert!(adds_a_source(&[1, 2], &[2, 1, 4]).is_ok());
        assert!(adds_a_source(&[1, 2], &[1, 2]).is_err());
        assert!(adds_a_source(&[1, 2], &[1, 3]).is_err());
        assert!(adds_a_source(&[1, 2], &[1, 2, 3, 3]).is_err());
        assert!(adds_a_source(&[1, 2, 3, 4, 5], &[1, 2, 3, 4, 5, 6]).is_err());
    }
}
