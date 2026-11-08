//! Tests of the real contract code, run on a normal computer with a mocked blockchain.
//! The stand-in judge below answers with the exact verdict shape the sealed box returns, so the contract cannot tell the difference.
use super::*;
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::testing_env;

const NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const STAKE: u128 = NEAR / 10;
const BASE: u128 = NEAR / 2;
const WINDOW: u64 = 100;
const RUN_DEADLINE: u64 = 1000;
const PENDING_TIMEOUT: u64 = 50;
const NAMES: [&str; 6] = ["w1.t", "w2.t", "w3.t", "ch1.t", "ch2.t", "ch3.t"];

fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

struct World {
    c: Courtroom,
    bh: u64,
    /// every yoctoNEAR that ever entered the contract
    money_in: u128,
}

impl World {
    fn new(audit_rate_bps: u16, top_judges: usize) -> World {
        // a fresh chain for every world, so storage from an earlier world cannot leak in
        near_sdk::mock::with_mocked_blockchain(|b| {
            let _ = b.take_storage();
        });
        let mut w = World { c: unsafe_placeholder(), bh: 10, money_in: 0 };
        w.as_("dao.t", 0);
        let top: Vec<String> = (0..top_judges).map(|i| format!("top-{i}")).collect();
        w.c = Courtroom::new(
            acc("dao.t"),
            "https://example.com/court.wasm".into(),
            "a".repeat(64),
            vec!["m1".into(), "m2".into(), "m3".into()],
            None,
            None,
            U128(STAKE),
            LadderConfig {
                top_models: top,
                challenge_base: U128(BASE),
                window_blocks: WINDOW,
                run_deadline_blocks: RUN_DEADLINE,
                pending_timeout_blocks: PENDING_TIMEOUT,
                audit_rate_bps,
            },
        );
        for g in 1..=6 {
            w.c.add_source(format!("https://s{g}.example/"), format!("g{g}"));
        }
        w
    }

    fn as_(&mut self, who: &str, deposit: u128) {
        let mut b = VMContextBuilder::new();
        b.predecessor_account_id(acc(who))
            .current_account_id(acc("court.t"))
            .block_height(self.bh)
            .attached_deposit(NearToken::from_yoctonear(deposit))
            .prepaid_gas(Gas::from_tgas(900))
            .random_seed([7u8; 32]);
        testing_env!(b.build());
    }

    fn seed(&mut self, seed: [u8; 32]) {
        let mut b = VMContextBuilder::new();
        b.predecessor_account_id(acc("court.t"))
            .current_account_id(acc("court.t"))
            .block_height(self.bh)
            .random_seed(seed);
        testing_env!(b.build());
    }

    fn advance(&mut self, blocks: u64) {
        self.bh += blocks;
    }

    fn fund(&mut self, who: &str, amount: u128) {
        self.as_(who, amount);
        self.c.deposit();
        self.money_in += amount;
    }

    fn fund_reserve(&mut self, amount: u128) {
        self.as_("funder.t", amount);
        self.c.fund_reserve();
        self.money_in += amount;
    }

    fn pk(id: u32) -> Pick {
        Pick { id, url: format!("https://s{id}.example/x"), x: "json:$.v".into(), tol: 0.0 }
    }

    fn picks(ids: &[u32]) -> Vec<Pick> {
        ids.iter().map(|i| Self::pk(*i)).collect()
    }

    fn open(&mut self) -> u64 {
        self.as_("opener.t", 0);
        self.c.open_round("The count is 5".into(), "data".into(), Some("5".into()), Self::picks(&[1, 2]), vec![], None)
    }

    fn bond(&mut self, who: &str, round: u64, word: &str) {
        self.as_(who, 0);
        self.c.bond(round, word.into());
    }

    /// The stand-in judge: the verdict the sealed box would return for this round and letter.
    fn verdict(&self, round: u64, letter: &str) -> String {
        let r = self.c.get_round(round).unwrap();
        format!(r#"{{"v":1,"rd":{},"ih":"{}","o":"{}","rc":0,"t":1,"s":[],"j":[]}}"#, round, r.ih, letter)
    }

    /// Start an attempt and deliver the stand-in judge's answer. `answer` None delivers nothing (a failed run).
    fn run(&mut self, round: u64, answer: Option<&str>) {
        self.as_("anyone.t", 0);
        self.c.begin_attempt(round);
        let n = self.c.get_round(round).unwrap().attempts.len() as u8;
        let out = answer.map(|l| self.verdict(round, l));
        self.as_("court.t", 0);
        self.c.on_attempt(round, n, Ok(out));
    }

    fn challenge(&mut self, who: &str, round: u64, ids: &[u32]) -> u64 {
        self.as_(who, 0);
        self.c.challenge(round, Self::picks(ids))
    }

    fn finalize(&mut self, round: u64) {
        self.as_("anyone.t", 0);
        self.c.finalize(round);
    }

    fn bal(&self, who: &str) -> u128 {
        self.c.balance(acc(who)).0
    }

    fn total_balances(&self) -> u128 {
        NAMES.iter().map(|n| self.bal(n)).sum()
    }

    /// Nothing created or lost: what is on the ledgers, in the reserve and locked equals what came in.
    fn assert_books(&self) {
        let (reserve, locked) = self.c.accounting();
        assert_eq!(self.total_balances() + reserve.0 + locked.0, self.money_in, "money was created or lost");
    }
}

// The placeholder only exists so World::new can build the struct before the contract is initialised.
fn unsafe_placeholder() -> Courtroom {
    let mut b = VMContextBuilder::new();
    b.predecessor_account_id(acc("dao.t")).current_account_id(acc("court.t"));
    testing_env!(b.build());
    Courtroom::new(
        acc("dao.t"),
        "u".into(),
        "a".repeat(64),
        vec!["a".into(), "b".into(), "c".into()],
        None,
        None,
        U128(1),
        LadderConfig {
            top_models: vec!["a".into(), "b".into(), "c".into()],
            challenge_base: U128(1),
            window_blocks: 1,
            run_deadline_blocks: 1,
            pending_timeout_blocks: 1,
            audit_rate_bps: 0,
        },
    )
}

fn funded_world(rate: u16) -> World {
    let mut w = World::new(rate, 5);
    for n in NAMES {
        w.fund(n, 20 * NEAR);
    }
    w
}

// ---------------------------------------------------------------- the full ladder

#[test]
fn full_ladder_with_the_stand_in_judge() {
    let mut w = funded_world(0);
    let r = w.open();
    w.bond("w1.t", r, "is");
    w.bond("w2.t", r, "likely");
    w.bond("w3.t", r, "disputed");
    w.run(r, Some("T"));
    assert_eq!(w.c.get_round(r).unwrap().status, "settled");
    assert_eq!(w.c.get_case(r).unwrap().standing, "T");
    // bonds wait for the end of the case
    assert_eq!(w.c.accounting().1 .0, 3 * STAKE);

    // rung 1: bond BASE, 3 sources, 3 judges, flips to F
    let c1 = w.challenge("ch1.t", r, &[1, 2, 3]);
    let r1 = w.c.get_round(c1).unwrap();
    assert_eq!((r1.level, r1.bond.0), (1, BASE));
    let inp: serde_json::Value = serde_json::from_str(&r1.input).unwrap();
    assert_eq!(inp["m"].as_array().unwrap().len(), 3);
    assert_eq!(inp["src"].as_array().unwrap().len(), 3);
    w.run(c1, Some("F"));
    assert_eq!(w.c.get_case(r).unwrap().standing, "F");

    // rung 2: bond 2 x BASE, 4 sources, 3 judges, flips back to T
    let c2 = w.challenge("ch2.t", r, &[1, 2, 3, 4]);
    let r2 = w.c.get_round(c2).unwrap();
    assert_eq!((r2.level, r2.bond.0), (2, 2 * BASE));
    w.run(c2, Some("T"));
    assert_eq!(w.c.get_case(r).unwrap().standing, "T");

    // rung 3 (top): bond 4 x BASE, 5 sources, 5 judges, flips to F
    let c3 = w.challenge("ch3.t", r, &[1, 2, 3, 4, 5]);
    let r3 = w.c.get_round(c3).unwrap();
    assert_eq!((r3.level, r3.bond.0), (3, 4 * BASE));
    let inp: serde_json::Value = serde_json::from_str(&r3.input).unwrap();
    assert_eq!(inp["m"].as_array().unwrap().len(), 5);
    assert_eq!(inp["src"].as_array().unwrap().len(), 5);
    w.run(c3, Some("F"));
    w.assert_books();

    // the top rung closes the ladder: no more challenges, and the case can be finished at once
    w.finalize(r);
    let case = w.c.get_case(r).unwrap();
    assert!(case.finalized);
    assert_eq!(case.final_outcome.as_deref(), Some("F"));
    assert_eq!(w.c.accounting().1 .0, 0);

    // challengers: rung 1 and 3 backed F (won), rung 2 backed T (lost 2 x BASE)
    // pot = 2 BASE, winners share half = BASE, split 1 : 4 by bond
    let share = BASE; // half of 2 x BASE
    let total_win = BASE + 4 * BASE;
    let e1 = logic::ladder::mul_div(share, BASE, total_win);
    let e3 = logic::ladder::mul_div(share, 4 * BASE, total_win);
    assert_eq!(w.bal("ch1.t"), 20 * NEAR + e1);
    assert_eq!(w.bal("ch2.t"), 20 * NEAR - 2 * BASE);
    assert_eq!(w.bal("ch3.t"), 20 * NEAR + e3);
    // sentence bonds paid on F: "is" loses 65.25%, "likely" loses 17.25%, "disputed" gets the stake back
    assert_eq!(w.bal("w1.t"), 20 * NEAR - STAKE * 6525 / 10_000);
    assert_eq!(w.bal("w2.t"), 20 * NEAR - STAKE * 1725 / 10_000);
    assert_eq!(w.bal("w3.t"), 20 * NEAR);
    w.assert_books();
}

#[test]
fn a_failed_challenge_pays_the_reserve_and_the_ruling_stands() {
    let mut w = funded_world(0);
    let r = w.open();
    w.bond("w1.t", r, "is");
    w.run(r, Some("T"));
    let c1 = w.challenge("ch1.t", r, &[1, 2, 3]);
    w.run(c1, Some("T")); // upheld
    w.advance(WINDOW);
    w.finalize(r);
    assert_eq!(w.bal("ch1.t"), 20 * NEAR - BASE);
    // the lost bond went to the reserve, and paid the winning writer's gain
    assert_eq!(w.bal("w1.t"), 20 * NEAR + STAKE * 2475 / 10_000);
    let (reserve, locked) = w.c.accounting();
    assert_eq!(locked.0, 0);
    assert_eq!(reserve.0, BASE - STAKE * 2475 / 10_000);
    w.assert_books();
}

// ---------------------------------------------------------------- refunds and nothing stuck

#[test]
fn void_original_refunds_every_bond() {
    let mut w = funded_world(0);
    let r = w.open();
    w.bond("w1.t", r, "is");
    w.bond("w2.t", r, "likely");
    for _ in 0..3 {
        w.run(r, None);
    }
    let rr = w.c.get_round(r).unwrap();
    assert_eq!(rr.status, "void");
    assert_eq!(w.bal("w1.t"), 20 * NEAR);
    assert_eq!(w.bal("w2.t"), 20 * NEAR);
    assert!(w.c.get_case(r).is_none());
    w.assert_books();
    assert_eq!(w.c.accounting().1 .0, 0);
}

#[test]
fn unsure_and_disputed_rulings_refund_and_cannot_be_challenged() {
    for letter in ["U", "D"] {
        let mut w = funded_world(0);
        let r = w.open();
        w.bond("w1.t", r, "is");
        w.run(r, Some(letter));
        assert_eq!(w.bal("w1.t"), 20 * NEAR);
        assert!(w.c.get_case(r).is_none());
        w.assert_books();
    }
}

#[test]
fn a_challenge_with_no_ruling_gets_its_bond_back() {
    for answer in [Some("U"), Some("D"), None] {
        let mut w = funded_world(0);
        let r = w.open();
        w.run(r, Some("T"));
        let c1 = w.challenge("ch1.t", r, &[1, 2, 3]);
        if answer.is_none() {
            for _ in 0..3 {
                w.run(c1, None);
            }
            assert_eq!(w.c.get_round(c1).unwrap().status, "void");
        } else {
            w.run(c1, answer);
        }
        assert_eq!(w.c.get_case(r).unwrap().standing, "T");
        w.advance(WINDOW);
        w.finalize(r);
        assert_eq!(w.bal("ch1.t"), 20 * NEAR, "bond must come back");
        w.assert_books();
    }
}

#[test]
fn a_round_nobody_runs_times_out_and_refunds() {
    let mut w = funded_world(0);
    let r = w.open();
    w.bond("w1.t", r, "is");
    w.advance(RUN_DEADLINE - 1);
    w.as_("anyone.t", 0);
    let early = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.c.expire_round(r)));
    assert!(early.is_err(), "too early to expire");
    w.advance(1);
    w.as_("anyone.t", 0);
    w.c.expire_round(r);
    assert_eq!(w.c.get_round(r).unwrap().void_reason.as_deref(), Some("TIMED_OUT"));
    assert_eq!(w.bal("w1.t"), 20 * NEAR);
    assert_eq!(w.c.accounting().1 .0, 0);
}

#[test]
fn an_unrun_challenge_times_out_and_the_case_can_finish() {
    let mut w = funded_world(0);
    let r = w.open();
    w.run(r, Some("F"));
    let c1 = w.challenge("ch1.t", r, &[1, 2, 3]);
    w.advance(RUN_DEADLINE);
    w.as_("anyone.t", 0);
    w.c.expire_round(c1);
    w.advance(WINDOW);
    w.finalize(r);
    assert_eq!(w.bal("ch1.t"), 20 * NEAR);
    assert!(w.c.get_case(r).unwrap().finalized);
    w.assert_books();
}

#[test]
fn a_run_that_never_answers_is_counted_as_failed_and_a_late_answer_is_ignored() {
    let mut w = funded_world(0);
    let r = w.open();
    w.as_("anyone.t", 0);
    w.c.begin_attempt(r);
    w.advance(PENDING_TIMEOUT - 1);
    w.as_("anyone.t", 0);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.c.expire_round(r))).is_err());
    w.advance(1);
    w.as_("anyone.t", 0);
    w.c.expire_round(r);
    let rr = w.c.get_round(r).unwrap();
    assert!(!rr.pending);
    assert_eq!(rr.failures, 1);
    assert_eq!(rr.attempts[0].result, "TIMED_OUT");
    // the answer shows up late: ignored
    let out = w.verdict(r, "T");
    w.as_("court.t", 0);
    w.c.on_attempt(r, 1, Ok(Some(out)));
    assert_eq!(w.c.get_round(r).unwrap().status, "open");
    // and a fresh attempt still works
    w.run(r, Some("T"));
    assert_eq!(w.c.get_round(r).unwrap().status, "settled");
}

// ---------------------------------------------------------------- random audits

#[test]
fn an_audit_must_be_run_before_the_case_can_finish_and_costs_nobody() {
    let mut w = funded_world(10_000);
    w.fund_reserve(NEAR); // gains need a pool to be paid from
    let r = w.open();
    w.bond("w1.t", r, "likely");
    w.run(r, Some("T"));
    let case = w.c.get_case(r).unwrap();
    let a = case.audit_round.expect("rate 10000 always draws an audit");
    let ar = w.c.get_round(a).unwrap();
    assert_eq!((ar.role.as_str(), ar.bond.0, ar.level), ("audit", 0, 0));
    assert_eq!(ar.claim, "The count is 5");
    assert_ne!(ar.ih, w.c.get_round(r).unwrap().ih, "an audit is a fresh request with its own number and seed");
    // the case cannot finish while the audit is open, even after the window
    w.advance(WINDOW);
    w.as_("anyone.t", 0);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.c.finalize(r))).is_err());
    // the audit disagrees: the ruling flips for free
    w.run(a, Some("F"));
    assert_eq!(w.c.get_case(r).unwrap().standing, "F");
    assert_eq!(w.c.get_round(a).unwrap().appeal_verdict.as_deref(), Some("flipped"));
    // the window restarts so anyone can challenge the new ruling
    let c1 = w.challenge("ch1.t", r, &[1, 2, 3]);
    w.run(c1, Some("T")); // restores T
    w.advance(WINDOW);
    w.finalize(r);
    // ch1 argued against F and F fell: wins, bond back, nobody lost a bond
    assert_eq!(w.bal("ch1.t"), 20 * NEAR);
    // writer was right (T): likely +12.75%
    assert_eq!(w.bal("w1.t"), 20 * NEAR + STAKE * 1275 / 10_000);
    w.assert_books();
}

#[test]
fn an_audit_nobody_runs_times_out_so_the_money_is_never_locked_forever() {
    let mut w = funded_world(10_000);
    let r = w.open();
    w.bond("w1.t", r, "is");
    w.run(r, Some("F"));
    let a = w.c.get_case(r).unwrap().audit_round.unwrap();
    w.advance(RUN_DEADLINE);
    w.as_("anyone.t", 0);
    w.c.expire_round(a);
    w.advance(WINDOW);
    w.finalize(r);
    assert_eq!(w.bal("w1.t"), 20 * NEAR - STAKE * 6525 / 10_000);
    assert_eq!(w.c.accounting().1 .0, 0);
    w.assert_books();
}

#[test]
fn the_audit_draw_follows_the_rate_and_ignores_the_word() {
    use bond_payout::audit_drawn;
    let mut hits = 0u32;
    let n = 20_000u32;
    for i in 0..n {
        let mut seed = [0u8; 32];
        seed[..4].copy_from_slice(&i.to_be_bytes());
        if audit_drawn(&seed, i as u64, 1, 2000) {
            hits += 1;
        }
    }
    let rate = hits as f64 / n as f64;
    assert!((0.185..0.215).contains(&rate), "20% expected, got {rate}");
    // never at rate 0, always at 10000
    assert!(!audit_drawn(&[1; 32], 1, 1, 0));
    assert!(audit_drawn(&[1; 32], 1, 1, 10_000));
}

#[test]
fn audit_drawn_inside_the_contract_uses_the_chain_seed() {
    // rate 5000 with different seeds: some rulings are audited, some are not
    let mut audited = 0;
    let runs = 60;
    for i in 0..runs {
        let mut w = funded_world(5000);
        let r = w.open();
        let mut s = [0u8; 32];
        s[0] = i as u8;
        s[1] = 99;
        w.as_("anyone.t", 0);
        w.c.begin_attempt(r);
        let out = w.verdict(r, "T");
        w.seed(s);
        w.c.on_attempt(r, 1, Ok(Some(out)));
        if w.c.get_case(r).unwrap().audit_round.is_some() {
            audited += 1;
        }
    }
    assert!(audited > 15 && audited < 45, "about half expected, got {audited}/{runs}");
}

// ---------------------------------------------------------------- every refusal

fn refuses<F: FnOnce() + std::panic::UnwindSafe>(f: F, why: &str) {
    let r = std::panic::catch_unwind(f);
    assert!(r.is_err(), "should have refused: {why}");
}

#[test]
fn every_challenge_refusal() {
    // fresh world per refusal so each one is tested alone
    let ruled = || {
        let mut w = funded_world(0);
        let r = w.open();
        w.run(r, Some("T"));
        (w, r)
    };
    {
        let (mut w, _) = ruled();
        refuses(std::panic::AssertUnwindSafe(|| { w.challenge("ch1.t", 999, &[1, 2, 3]); }), "unknown round");
    }
    {
        let mut w = funded_world(0);
        let r = w.open();
        refuses(std::panic::AssertUnwindSafe(|| { w.challenge("ch1.t", r, &[1, 2, 3]); }), "no ruling yet");
    }
    {
        let mut w = funded_world(0);
        let r = w.open();
        w.run(r, Some("U"));
        refuses(std::panic::AssertUnwindSafe(|| { w.challenge("ch1.t", r, &[1, 2, 3]); }), "unsure ruling");
    }
    {
        let (mut w, r) = ruled();
        w.advance(WINDOW);
        refuses(std::panic::AssertUnwindSafe(|| { w.challenge("ch1.t", r, &[1, 2, 3]); }), "window closed");
    }
    {
        let (mut w, r) = ruled();
        w.challenge("ch1.t", r, &[1, 2, 3]);
        refuses(std::panic::AssertUnwindSafe(|| { w.challenge("ch2.t", r, &[1, 2, 3, 4]); }), "a re-run is open");
    }
    {
        let (mut w, r) = ruled();
        refuses(std::panic::AssertUnwindSafe(|| { w.challenge("ch1.t", r, &[1, 2]); }), "no new source");
    }
    {
        let (mut w, r) = ruled();
        refuses(std::panic::AssertUnwindSafe(|| { w.challenge("ch1.t", r, &[1, 3]); }), "drops an earlier source");
    }
    {
        let (mut w, r) = ruled();
        refuses(std::panic::AssertUnwindSafe(|| { w.challenge("ch1.t", r, &[1, 2, 3, 3]); }), "duplicate source");
    }
    {
        let (mut w, r) = ruled();
        refuses(std::panic::AssertUnwindSafe(|| { w.challenge("ch1.t", r, &[1, 2, 99]); }), "source not approved");
    }
    {
        let (mut w, r) = ruled();
        w.as_("ch1.t", 0);
        let mut p = World::picks(&[1, 2, 3]);
        p[2].url = "https://evil.example/x".into();
        refuses(std::panic::AssertUnwindSafe(|| { w.c.challenge(r, p); }), "url outside the source");
    }
    {
        let (mut w, r) = ruled();
        w.as_("poor.t", 0);
        refuses(std::panic::AssertUnwindSafe(|| { w.c.challenge(r, World::picks(&[1, 2, 3])); }), "no balance for the bond");
    }
    {
        let (mut w, r) = ruled();
        // a source added after the ruling shares group g1 with source 1: still needs two groups overall, fine here;
        // but removing a source schedules it out, and after 72 hours it cannot be used
        w.as_("dao.t", 0);
        w.c.remove_source(3);
        w.bh += 1_000_000;
        w.as_("ch1.t", 0);
        refuses(std::panic::AssertUnwindSafe(|| { w.c.challenge(r, World::picks(&[1, 2, 3])); }), "source removed");
    }
    {
        // top of the ladder: a fourth challenge is refused
        let (mut w, r) = ruled();
        let c1 = w.challenge("ch1.t", r, &[1, 2, 3]);
        w.run(c1, Some("F"));
        let c2 = w.challenge("ch2.t", r, &[1, 2, 3, 4]);
        w.run(c2, Some("T"));
        let c3 = w.challenge("ch3.t", r, &[1, 2, 3, 4, 5]);
        w.run(c3, Some("F"));
        refuses(std::panic::AssertUnwindSafe(|| { w.challenge("ch1.t", r, &[1, 2, 3, 4, 5, 6]); }), "past the top");
        w.finalize(r);
        refuses(std::panic::AssertUnwindSafe(|| { w.challenge("ch1.t", r, &[1, 2, 3, 4, 5, 6]); }), "after the end");
    }
    {
        // more than five sources
        let (mut w, r) = ruled();
        refuses(std::panic::AssertUnwindSafe(|| { w.challenge("ch1.t", r, &[1, 2, 3, 4, 5, 6]); }), "six sources");
    }
}

#[test]
fn every_finalize_and_expire_and_bond_refusal() {
    {
        let mut w = funded_world(0);
        let r = w.open();
        refuses(std::panic::AssertUnwindSafe(|| w.finalize(r)), "finalize without a ruling");
        refuses(std::panic::AssertUnwindSafe(|| w.finalize(999)), "finalize unknown round");
    }
    {
        let mut w = funded_world(0);
        let r = w.open();
        w.run(r, Some("T"));
        refuses(std::panic::AssertUnwindSafe(|| w.finalize(r)), "window still open");
        let c1 = w.challenge("ch1.t", r, &[1, 2, 3]);
        w.advance(WINDOW + 1);
        refuses(std::panic::AssertUnwindSafe(|| w.finalize(r)), "re-run still open");
        w.run(c1, Some("T"));
        refuses(std::panic::AssertUnwindSafe(|| w.finalize(r)), "window restarted after the re-run");
        w.advance(WINDOW);
        w.finalize(r);
        refuses(std::panic::AssertUnwindSafe(|| w.finalize(r)), "finalize twice");
    }
    {
        let mut w = funded_world(0);
        let r = w.open();
        w.run(r, Some("T"));
        w.as_("anyone.t", 0);
        refuses(std::panic::AssertUnwindSafe(|| w.c.expire_round(r)), "expire a closed round");
        refuses(std::panic::AssertUnwindSafe(|| w.c.expire_round(999)), "expire unknown");
    }
    {
        let mut w = funded_world(0);
        let r = w.open();
        w.run(r, Some("T"));
        refuses(std::panic::AssertUnwindSafe(|| w.bond("w1.t", r, "is")), "bond after ruling");
        let r2 = w.open();
        w.bond("w1.t", r2, "is");
        refuses(std::panic::AssertUnwindSafe(|| w.bond("w1.t", r2, "likely")), "double bond");
        refuses(std::panic::AssertUnwindSafe(|| w.bond("w2.t", r2, "certain")), "bad word");
        refuses(std::panic::AssertUnwindSafe(|| w.bond("poor.t", r2, "is")), "no balance");
        // an audit or challenge round takes no word bonds
        let mut w = funded_world(10_000);
        let r = w.open();
        w.run(r, Some("T"));
        let a = w.c.get_case(r).unwrap().audit_round.unwrap();
        refuses(std::panic::AssertUnwindSafe(|| w.bond("w1.t", a, "is")), "bond on an audit");
    }
    {
        // the ladder settings are checked at start
        let r = std::panic::catch_unwind(|| World::new(10_001, 5));
        assert!(r.is_err(), "audit rate above 100%");
        let r = std::panic::catch_unwind(|| World::new(0, 4));
        assert!(r.is_err(), "4 top judges");
        let r = std::panic::catch_unwind(|| World::new(0, 2));
        assert!(r.is_err(), "2 top judges");
    }
}

// ---------------------------------------------------------------- property test for money

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

const WORDS: [&str; 4] = ["is", "very likely", "likely", "disputed"];
const LETTERS: [&str; 5] = ["T", "F", "U", "D", "X"];

use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
static CHALLENGES: AtomicU32 = AtomicU32::new(0);
static TOP_RUNGS: AtomicU32 = AtomicU32::new(0);
static AUDITS: AtomicU32 = AtomicU32::new(0);
static FINISHED: AtomicU32 = AtomicU32::new(0);

fn scenario(seed: u64) {
    let mut rng = Lcg(seed);
    let rate = [0u16, 10_000, 5000][rng.below(3) as usize];
    let mut w = World::new(rate, 5);
    for n in NAMES {
        w.fund(n, 5 * NEAR);
    }
    w.fund_reserve(rng.below(3) as u128 * NEAR / (1 + rng.below(50) as u128));
    let rounds: Vec<u64> = (0..1 + rng.below(3)).map(|_| w.open()).collect();
    for &r in &rounds {
        for who in ["w1.t", "w2.t", "w3.t"] {
            if rng.below(4) != 0 {
                let word = WORDS[rng.below(4) as usize];
                w.bond(who, r, word);
            }
        }
    }
    // run every original round with a random stand-in answer ("X" = no answer)
    let pick_answer = |rng: &mut Lcg| -> Option<&'static str> {
        let l = LETTERS[rng.below(5) as usize];
        if l == "X" { None } else { Some(l) }
    };
    for &r in &rounds {
        for _ in 0..3 {
            if w.c.get_round(r).unwrap().status != "open" {
                break;
            }
            let a = pick_answer(&mut rng);
            w.run(r, a);
        }
        w.assert_books();
    }
    // random challenges and audit runs
    for _ in 0..12 {
        let r = rounds[rng.below(rounds.len() as u64) as usize];
        let Some(case) = w.c.get_case(r) else { continue };
        if case.finalized {
            continue;
        }
        let last_id = case.rounds.last().copied().unwrap_or(r);
        let last = w.c.get_round(last_id).unwrap();
        if last.status == "open" {
            if rng.below(3) == 0 {
                w.advance(RUN_DEADLINE);
                w.as_("anyone.t", 0);
                w.c.expire_round(last_id);
            } else {
                let a = pick_answer(&mut rng);
                for _ in 0..3 {
                    if w.c.get_round(last_id).unwrap().status != "open" {
                        break;
                    }
                    w.run(last_id, a);
                }
            }
        } else if w.bh < case.window_ends && case.level < 3 {
            let level = case.level + 1;
            let bond = w.c.challenge_bond(level).unwrap().0;
            let who = NAMES[3 + rng.below(3) as usize];
            if w.bal(who) >= bond {
                let prev: Vec<u32> = serde_json::from_str::<serde_json::Value>(&last.input).unwrap()["src"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s["i"].as_u64().unwrap() as u32)
                    .collect();
                let mut next = prev.clone();
                next.push((1..=6).find(|i| !prev.contains(i)).unwrap());
                w.challenge(who, r, &next);
                CHALLENGES.fetch_add(1, Relaxed);
                if level == 3 {
                    TOP_RUNGS.fetch_add(1, Relaxed);
                }
            }
        } else {
            w.advance(rng.below(WINDOW));
        }
        w.assert_books();
    }
    // wind everything down: time out whatever is open, then finish every case
    w.advance(RUN_DEADLINE + WINDOW);
    for id in 1..=w.c.round_count() {
        if w.c.get_round(id).unwrap().status == "open" {
            w.as_("anyone.t", 0);
            // a pending run needs the pending timeout first, then the deadline, then expiry again
            for _ in 0..4 {
                if w.c.get_round(id).unwrap().status != "open" {
                    break;
                }
                w.as_("anyone.t", 0);
                w.c.expire_round(id);
                w.advance(RUN_DEADLINE);
            }
            assert_ne!(w.c.get_round(id).unwrap().status, "open", "round {id} stuck open, seed {seed}");
        }
    }
    w.advance(WINDOW + RUN_DEADLINE);
    for &r in &rounds {
        if let Some(case) = w.c.get_case(r) {
            if case.audit_round.is_some() {
                AUDITS.fetch_add(1, Relaxed);
            }
            if !case.finalized {
                w.finalize(r);
                FINISHED.fetch_add(1, Relaxed);
            }
        }
    }
    w.assert_books();
    assert_eq!(w.c.accounting().1 .0, 0, "money still locked after everything ended, seed {seed}");
    for id in 1..=w.c.round_count() {
        let r = w.c.get_round(id).unwrap();
        assert_ne!(r.status, "open", "seed {seed}");
        if r.role == "original" {
            for b in &r.bonds {
                assert!(b.payout.is_some(), "a bond was never paid, seed {seed}");
            }
        }
    }
}

#[test]
fn money_is_never_created_or_lost_and_nothing_stays_locked() {
    // 60 scenarios by default (about a minute); set SCENARIOS=400 for the long run
    let n: u64 = std::env::var("SCENARIOS").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
    for seed in 0..n {
        scenario(seed * 7919 + 13);
    }
    let (c, t, a, f) = (CHALLENGES.load(Relaxed), TOP_RUNGS.load(Relaxed), AUDITS.load(Relaxed), FINISHED.load(Relaxed));
    eprintln!("scenarios {n}: challenges {c}, top rungs {t}, audits {a}, cases finished {f}");
    assert!(c > 10 && a > 5 && f > 10, "the random scenarios must really use challenges, audits and finishing");
}
