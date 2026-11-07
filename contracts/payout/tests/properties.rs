use bond_payout::*;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 11
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const B: u128 = 1_000_000_000_000_000_000_000_000; // 1 NEAR in the smallest unit

fn pct(n: i128) -> f64 {
    n as f64 / B as f64 * 100.0
}

#[test]
fn published_examples() {
    // is: +24.75% right, -65.25% wrong. very likely: +21, -39. likely: +12.75, -17.25. disputed: 0 / 0.
    let cases = [("is", 24.75, -65.25), ("very likely", 21.0, -39.0), ("likely", 12.75, -17.25), ("disputed", 0.0, 0.0)];
    for (w, win, lose) in cases {
        let p = word_bps(w).unwrap();
        assert!((pct(net(B, p, Outcome::True)) - win).abs() < 1e-6, "{w} true");
        assert!((pct(net(B, p, Outcome::False)) - lose).abs() < 1e-6, "{w} false");
    }
}

#[test]
fn disputed_always_returns_the_stake() {
    for o in [Outcome::True, Outcome::False] {
        assert_eq!(net(B, 5000, o), 0);
    }
}

#[test]
fn honesty_pays_best_on_a_fine_grid() {
    // For every belief, writing the belief gives the highest expected gain among all chances.
    for q in (0..=10_000u16).step_by(100) {
        let best = (0..=10_000u16).step_by(100).max_by(|a, b| {
            expected_net_f64(q, *a).partial_cmp(&expected_net_f64(q, *b)).unwrap()
        });
        assert_eq!(best, Some(q), "belief {q}");
    }
}

#[test]
fn moving_toward_fifty_fifty_never_pays() {
    // No hedging reward: from the honest chance, every step toward 50% lowers the expected gain,
    // and so does every step away from it.
    for q in (0..=10_000u16).step_by(100) {
        let at = |p: u16| expected_net_f64(q, p);
        let mut p = q;
        while p + 100 <= 10_000 {
            assert!(at(p + 100) < at(p), "belief {q}: raising {p} to {} should lower the gain", p + 100);
            p += 100;
        }
        let mut p = q;
        while p >= 100 {
            assert!(at(p - 100) < at(p), "belief {q}: lowering {p} to {} should lower the gain", p - 100);
            p -= 100;
        }
    }
}

#[test]
fn disputed_earns_nothing_and_honest_earns_its_edge() {
    for q in (0..=10_000u16).step_by(50) {
        assert!(expected_net_f64(q, 5000).abs() < 1e-12, "disputed must be neutral for belief {q}");
        let edge = (q as f64 / 10_000.0 - 0.5).powi(2);
        assert!((expected_net_f64(q, q) - edge).abs() < 1e-12);
        assert!(expected_net_f64(q, q) >= -1e-12);
    }
}

#[test]
fn no_jump_anywhere_and_none_at_likely() {
    // One basis point of change in the word changes the gain by at most 0.021% of the stake.
    let limit = (B as f64 * 0.00021) as i128;
    for o in [Outcome::True, Outcome::False] {
        for p in 0..10_000u16 {
            let a = net(B, p, o);
            let b = net(B, p + 1, o);
            assert!((a - b).abs() <= limit, "jump at {p} for {o:?}");
        }
        // the exact neighbourhood of "likely"
        let around: Vec<i128> = (6490..=6510).map(|p| net(B, p, o)).collect();
        for w in around.windows(2) {
            assert!((w[0] - w[1]).abs() <= limit);
        }
    }
}

#[test]
fn integer_maths_matches_floating_point_on_1000_vectors() {
    let mut r = Lcg(2026);
    let mut vectors = vec![];
    for _ in 0..1000 {
        let stake = 1_000_000 + r.below(1_000_000_000_000) as u128;
        let p = r.below(10_001) as u16;
        let o = if r.below(2) == 0 { Outcome::True } else { Outcome::False };
        let n = net(stake, p, o);
        let t = if o == Outcome::True { 1.0 } else { 0.0 };
        let want = stake as f64 * (0.25 - (t - p as f64 / 10_000.0).powi(2));
        assert!((n as f64 - want).abs() <= 1.0 + stake as f64 * 1e-12, "stake {stake} p {p} {o:?}: {n} vs {want}");
        vectors.push(serde_json::json!({"stake": stake.to_string(), "p": p, "o": if o == Outcome::True {"T"} else {"F"}, "net": n.to_string()}));
    }
    // Write the vectors once so other programs (the checker) can verify the same numbers.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/vectors.json");
    if std::env::var("REGEN_VECTORS").is_ok() || !std::path::Path::new(path).exists() {
        std::fs::write(path, serde_json::to_string(&vectors).unwrap()).unwrap();
    } else {
        let have: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(have, vectors, "vectors file no longer matches the rule");
    }
}

fn random_batch(r: &mut Lcg) -> (Vec<Entry>, Option<Outcome>, u128) {
    let n = 1 + r.below(12) as usize;
    let entries = (0..n)
        .map(|_| Entry {
            stake: B / 100 + r.below(5) as u128 * B / 100,
            chance_bps: WORDS[r.below(4) as usize].1,
        })
        .collect();
    let outcome = match r.below(6) {
        0 => None,
        1 | 2 | 3 => Some(Outcome::True),
        _ => Some(Outcome::False),
    };
    let reserve = if r.below(2) == 0 { 0 } else { r.below(3) as u128 * B / 10 };
    (entries, outcome, reserve)
}

#[test]
fn not_one_unit_is_created_or_lost() {
    let mut r = Lcg(7);
    for _ in 0..20_000 {
        let (entries, outcome, reserve) = random_batch(&mut r);
        let s = settle(&entries, outcome, reserve);
        let stakes: u128 = entries.iter().map(|e| e.stake).sum();
        let paid: u128 = s.payouts.iter().sum();
        assert_eq!(paid + s.reserve_after, stakes + reserve, "money must be conserved exactly");
        for (e, p) in entries.iter().zip(&s.payouts) {
            // never below a quarter of the stake, never above 125% of it
            assert!(*p >= e.stake / 4 - 1);
            assert!(*p <= e.stake + e.stake / 4 + 1);
        }
    }
}

#[test]
fn without_a_reserve_the_group_never_pays_out_more_than_it_took_in() {
    let mut r = Lcg(99);
    for _ in 0..20_000 {
        let (entries, outcome, _) = random_batch(&mut r);
        let s = settle(&entries, outcome, 0);
        let stakes: u128 = entries.iter().map(|e| e.stake).sum();
        assert!(s.payouts.iter().sum::<u128>() <= stakes);
    }
}

#[test]
fn everyone_right_and_bold_with_no_reserve_gets_exactly_their_stakes_back() {
    let entries: Vec<Entry> = (0..5).map(|_| Entry { stake: B, chance_bps: 9500 }).collect();
    let s = settle(&entries, Some(Outcome::True), 0);
    assert_eq!(s.payouts.iter().sum::<u128>() + s.reserve_after, 5 * B);
    assert!(s.payouts.iter().all(|p| *p == B));
}

#[test]
fn a_reserve_pays_the_full_gain() {
    let entries = vec![Entry { stake: B, chance_bps: 9500 }];
    let s = settle(&entries, Some(Outcome::True), B / 2);
    assert_eq!(s.payouts[0], B + net(B, 9500, Outcome::True) as u128);
    assert!(s.reserve_after < B / 2);
}

#[test]
fn losses_fund_gains() {
    let entries = vec![Entry { stake: B, chance_bps: 9500 }, Entry { stake: B, chance_bps: 9500 }];
    // both say "is", sentence is false: both lose 65.25%, the pool grows by the forfeits
    let s = settle(&entries, Some(Outcome::False), 0);
    assert!(s.payouts.iter().all(|p| (*p as f64 / B as f64 - 0.3475).abs() < 1e-9));
    // the forfeited 65.25% of each stake stays in the reserve for later winners
    assert_eq!(s.reserve_after, 2 * B * 6525 / 10_000);
    assert_eq!(s.payouts.iter().sum::<u128>(), 2 * B * 3475 / 10_000);
}

#[test]
fn unsure_disputed_and_void_return_every_stake() {
    let mut r = Lcg(5);
    for _ in 0..2_000 {
        let (entries, _, reserve) = random_batch(&mut r);
        let s = settle(&entries, None, reserve);
        let want: Vec<u128> = entries.iter().map(|e| e.stake).collect();
        assert_eq!(s.payouts, want);
        assert_eq!(s.reserve_after, reserve);
    }
}

#[test]
fn audit_does_not_depend_on_the_word() {
    // audit_drawn takes no word at all. Check the rate and that it is repeatable.
    let mut hits = 0;
    let n = 40_000u64;
    for i in 0..n {
        let mut seed = [0u8; 32];
        seed[..8].copy_from_slice(&i.to_be_bytes());
        let a = audit_drawn(&seed, 77, 1, 1000);
        assert_eq!(a, audit_drawn(&seed, 77, 1, 1000));
        if a {
            hits += 1;
        }
    }
    let rate = hits as f64 / n as f64;
    assert!((rate - 0.10).abs() < 0.01, "audit rate {rate}");
}

#[test]
fn every_word_resolves_and_unknown_words_do_not() {
    for (w, p) in WORDS {
        assert_eq!(word_bps(w), Some(p));
    }
    assert_eq!(word_bps("certainly"), None);
}
