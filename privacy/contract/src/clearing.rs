//! One-price clearing for a batch of opened orders. Pure integer maths, no chain calls, so the
//! same rules can be re-run in Node by the checker and compared line by line.
//!
//! An order is (side, limit price in basis points of its own side, amount it is willing to spend).
//! Side 0 buys YES at price p, side 1 buys NO at price 10000 - p. For a candidate price p:
//! An order is eligible when its limit is at least its own side's price (p for YES, 10000 - p for NO).
//!   an order asks for amount * 10000 / (its side price) shares (floor), and
//!   yes_shares / no_shares are the sums of those asks over eligible orders
//!   matched(p) = min(yes_shares, no_shares)
//! The clearing price is the p with the largest matched amount. Ties go to the p closest to 5000,
//! then to the lower p. Orders on the short side fill completely. Orders on the long side share
//! `matched` in proportion to what they asked for (largest remainder, ties by arrival order), so
//! YES shares and NO shares come out exactly equal. A trader pays ceil(shares * side price / 10000)
//! and gets the rest of the deposit back.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Req {
    pub side: u8,
    pub limit_bp: u16,
    pub amount: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Fill {
    pub shares: u128,
    pub cost: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cleared {
    /// 0 when nothing matched
    pub price_bp: u16,
    pub matched: u128,
    pub fills: Vec<Fill>,
}

/// floor(a * b / c) and the remainder, without overflowing: the product is held in 256 bits.
fn mul_div(a: u128, b: u128, c: u128) -> (u128, u128) {
    let (a0, a1) = (a & u64::MAX as u128, a >> 64);
    let (b0, b1) = (b & u64::MAX as u128, b >> 64);
    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;
    let mid = (p00 >> 64) + (p01 & u64::MAX as u128) + (p10 & u64::MAX as u128);
    let lo = (p00 & u64::MAX as u128) | (mid << 64);
    let hi = p11 + (p01 >> 64) + (p10 >> 64) + (mid >> 64);
    // long division of (hi, lo) by c, one bit at a time
    let (mut q, mut r) = (0u128, 0u128);
    for i in (0..256).rev() {
        let bit = if i >= 128 { (hi >> (i - 128)) & 1 } else { (lo >> i) & 1 };
        let carry = r >> 127;
        r = (r << 1) | bit;
        q <<= 1;
        if carry == 1 || r >= c {
            r = r.wrapping_sub(c);
            q |= 1;
        }
    }
    (q, r)
}

fn side_price(side: u8, p: u16) -> u128 {
    if side == 0 {
        p as u128
    } else {
        10_000 - p as u128
    }
}

/// Shares an order asks for at the given price of its own side.
fn demand(r: &Req, side_p: u128) -> u128 {
    r.amount * 10_000 / side_p
}

fn eligible(r: &Req, p: u16) -> bool {
    r.limit_bp as u128 >= side_price(r.side, p)
}

/// Shares asked on one side at price p (sum of each eligible order's own floor).
fn asked(orders: &[Req], p: u16, side: u8) -> u128 {
    let mut t = 0u128;
    for r in orders {
        if r.side == side && eligible(r, p) {
            t += demand(r, side_price(side, p));
        }
    }
    t
}

fn matched_at(orders: &[Req], p: u16) -> u128 {
    asked(orders, p, 0).min(asked(orders, p, 1))
}

/// First p in 1..=9999 for which `f(p)` holds, given that `f` is false then true (monotone). 10000 if never.
fn first_true(f: impl Fn(u16) -> bool) -> u16 {
    let (mut lo, mut hi) = (1u32, 10_000u32);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if f(mid as u16) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo as u16
}

/// YES asks fall and NO asks rise as p goes up, so matched = min(YES, NO) peaks where they cross.
/// Two binary searches find the crossing and the whole stretch of prices that reach the peak;
/// the clearing price is the point of that stretch closest to 5000.
fn find_price(orders: &[Req]) -> (u16, u128) {
    let p0 = first_true(|p| asked(orders, p, 1) >= asked(orders, p, 0));
    let mut peak = 0u128;
    if p0 <= 9_999 {
        peak = peak.max(matched_at(orders, p0));
    }
    if p0 >= 2 {
        peak = peak.max(matched_at(orders, p0 - 1));
    }
    if peak == 0 {
        return (0, 0);
    }
    // YES asks >= peak holds on a prefix [1, a]; NO asks >= peak holds on a suffix [b, 9999]
    let a = first_true(|p| asked(orders, p, 0) < peak) - 1;
    let b = first_true(|p| asked(orders, p, 1) >= peak);
    debug_assert!(b <= a);
    (5000u16.clamp(b, a), peak)
}

pub fn clear(orders: &[Req]) -> Cleared {
    let mut fills = vec![Fill::default(); orders.len()];
    let (best_p, best_m) = find_price(orders);
    if best_m == 0 {
        return Cleared { price_bp: 0, matched: 0, fills };
    }
    let p = best_p;
    // total shares asked per side at p
    let mut ask = [0u128; 2];
    let mut dem = vec![0u128; orders.len()];
    for (i, r) in orders.iter().enumerate() {
        if eligible(r, p) {
            dem[i] = demand(r, side_price(r.side, p));
            ask[r.side as usize] += dem[i];
        }
    }
    for side in 0..2u8 {
        let total = ask[side as usize];
        if total == 0 {
            continue;
        }
        // shares this side receives: all of what it asked if short, otherwise `matched`
        let target = best_m.min(total);
        let mut given = 0u128;
        let mut rem: Vec<(u128, usize)> = Vec::new();
        for (i, r) in orders.iter().enumerate() {
            if r.side != side || dem[i] == 0 {
                continue;
            }
            let (s, rest) = mul_div(dem[i], target, total);
            fills[i].shares = s;
            given += s;
            rem.push((rest, i));
        }
        // largest remainder; equal remainders go to the earlier order
        rem.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut left = target - given;
        for (_, i) in rem {
            if left == 0 {
                break;
            }
            // never give more than the order asked for
            if fills[i].shares < dem[i] {
                fills[i].shares += 1;
                left -= 1;
            }
        }
        debug_assert_eq!(left, 0);
    }
    for (i, r) in orders.iter().enumerate() {
        let sp = side_price(r.side, p);
        let c = (fills[i].shares * sp + 9_999) / 10_000;
        fills[i].cost = c;
    }
    Cleared { price_bp: p, matched: best_m, fills }
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: u128 = 1_000_000_000_000_000_000_000_000;

    fn req(side: u8, limit_bp: u16, n: u128) -> Req {
        Req { side, limit_bp, amount: n * N }
    }

    fn check_sound(orders: &[Req], c: &Cleared) {
        let (mut ys, mut ns, mut paid) = (0u128, 0u128, 0u128);
        for (r, f) in orders.iter().zip(&c.fills) {
            assert!(f.cost <= r.amount, "cost above the amount the trader offered");
            if f.shares > 0 {
                assert!(r.limit_bp as u128 >= side_price(r.side, c.price_bp), "filled above the limit");
            }
            if r.side == 0 { ys += f.shares } else { ns += f.shares }
            paid += f.cost;
        }
        assert_eq!(ys, ns, "YES and NO shares must be equal");
        assert_eq!(ys, c.matched);
        assert!(paid >= c.matched, "collateral must cover every share");
        assert!(paid <= c.matched + orders.len() as u128, "rounding dust is at most one unit per order");
    }

    /// The slow, obvious search: try every price. The fast search must agree with it.
    fn slow(orders: &[Req]) -> (u16, u128) {
        let (mut bp, mut bm) = (0u16, 0u128);
        for p in 1..10_000u16 {
            let m = matched_at(orders, p);
            if m == 0 {
                continue;
            }
            let better = m > bm || (m == bm && (p as i32 - 5000).abs() < (bp as i32 - 5000).abs());
            if better {
                bp = p;
                bm = m;
            }
        }
        (bp, bm)
    }

    #[test]
    fn fast_price_search_equals_trying_every_price() {
        let mut seed = 99u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        for t in 0..300 {
            let n = (next() % 14) as usize;
            let big = t % 3 == 0;
            let o: Vec<Req> = (0..n)
                .map(|_| Req {
                    side: (next() % 2) as u8,
                    limit_bp: 1 + (next() % 9999) as u16,
                    amount: if big { (1 + next() as u128 % 90) * N / 3 } else { 1 + next() as u128 % 60_000 },
                })
                .collect();
            assert_eq!(find_price(&o), slow(&o), "case {t}: {o:?}");
        }
    }

    #[test]
    fn mul_div_is_exact_on_huge_numbers() {
        assert_eq!(mul_div(10, 10, 3), (33, 1));
        let big = u128::MAX / 3;
        assert_eq!(mul_div(big, 3, 3), (big, 0));
        assert_eq!(mul_div(u128::MAX, u128::MAX, u128::MAX), (u128::MAX, 0));
        assert_eq!(mul_div(1u128 << 100, 1u128 << 100, 1u128 << 100), (1u128 << 100, 0));
        assert_eq!(mul_div(u128::MAX, 2, 4), (u128::MAX / 2, 2));
    }

    #[test]
    fn two_sides_cross() {
        let o = [req(0, 7000, 3), req(1, 5000, 2)];
        let c = clear(&o);
        assert!(c.price_bp >= 5000 && c.price_bp <= 7000);
        check_sound(&o, &c);
        assert!(c.matched > 0);
    }

    #[test]
    fn no_cross_means_no_trade() {
        // YES wants at most 30 percent, NO wants at most 30 percent: prices sum to 60, they never meet
        let o = [req(0, 3000, 3), req(1, 3000, 3)];
        let c = clear(&o);
        assert_eq!(c.price_bp, 0);
        assert_eq!(c.matched, 0);
        assert!(c.fills.iter().all(|f| f.cost == 0 && f.shares == 0));
    }

    #[test]
    fn empty_and_one_sided() {
        assert_eq!(clear(&[]).price_bp, 0);
        let c = clear(&[req(0, 9000, 5)]);
        assert_eq!(c.matched, 0);
    }

    #[test]
    fn long_side_is_shared_pro_rata() {
        let o = [req(0, 9000, 6), req(0, 9000, 3), req(1, 9000, 3)];
        let c = clear(&o);
        check_sound(&o, &c);
        // the first YES order asked twice as much as the second
        let (a, b) = (c.fills[0].shares, c.fills[1].shares);
        assert!(a > b && (a as i128 - 2 * b as i128).abs() <= 2);
    }

    #[test]
    fn order_below_the_clearing_price_gets_nothing() {
        let o = [req(0, 8000, 4), req(0, 2000, 4), req(1, 6000, 4)];
        let c = clear(&o);
        check_sound(&o, &c);
        assert_eq!(c.fills[1].shares, 0);
        assert_eq!(c.fills[1].cost, 0);
    }

    #[test]
    fn same_orders_same_answer_in_any_arrival_order_for_the_price() {
        let o = [req(0, 7500, 5), req(1, 6000, 3), req(0, 6500, 2), req(1, 4000, 4)];
        let mut r = o;
        r.reverse();
        assert_eq!(clear(&o).price_bp, clear(&r).price_bp);
        assert_eq!(clear(&o).matched, clear(&r).matched);
    }

    #[test]
    fn many_orders_stay_sound() {
        let mut seed = 12345u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        for _ in 0..40 {
            let n = 1 + (next() % 100) as usize;
            let o: Vec<Req> = (0..n)
                .map(|_| Req {
                    side: (next() % 2) as u8,
                    limit_bp: 1 + (next() % 9999) as u16,
                    amount: 1 + (next() as u128 % 1000) * N / 100 + next() as u128,
                })
                .collect();
            let c = clear(&o);
            if c.matched > 0 {
                check_sound(&o, &c);
            }
        }
    }

    #[test]
    fn write_vectors_for_the_node_checker() {
        // Only writes when asked, so a normal test run never touches the file system.
        if std::env::var("WRITE_CLEARING_VECTORS").is_err() {
            return;
        }
        let mut seed = 777u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        let mut out = String::from("[");
        for t in 0..200 {
            let n = (next() % 12) as usize;
            let o: Vec<Req> = (0..n)
                .map(|_| Req {
                    side: (next() % 2) as u8,
                    limit_bp: 1 + (next() % 9999) as u16,
                    amount: (1 + next() as u128 % 5000) * N / 1000,
                })
                .collect();
            let c = clear(&o);
            if t > 0 {
                out.push(',');
            }
            let ord: Vec<String> = o
                .iter()
                .map(|r| format!("[{},{},\"{}\"]", r.side, r.limit_bp, r.amount))
                .collect();
            let fl: Vec<String> = c.fills.iter().map(|f| format!("[\"{}\",\"{}\"]", f.shares, f.cost)).collect();
            out.push_str(&format!(
                "{{\"orders\":[{}],\"price_bp\":{},\"matched\":\"{}\",\"fills\":[{}]}}",
                ord.join(","),
                c.price_bp,
                c.matched,
                fl.join(",")
            ));
        }
        out.push(']');
        std::fs::write(std::env::var("WRITE_CLEARING_VECTORS").unwrap(), out).unwrap();
    }
}
