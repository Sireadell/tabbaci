//! The courtroom contract.
//! No upgrade method and no access keys: once deployed and its keys are deleted, nobody can change it.
//! Rules live in the courtroom-rules crate and are tested without a blockchain.
use courtroom_rules as logic;

mod store;
use store::{IterableMap, LookupMap};

use logic::ladder::{self, AppealIn, Side, Verdict};
use logic::{judge_output, Judged, State, Status};
use bond_payout::{audit_hit, audit_preimage, settle, word_bps, Entry, Outcome};
use near_sdk::json_types::U128;
use near_sdk::{env, ext_contract, near, serde_json, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError};

#[cfg(not(feature = "mainnet"))]
const OUTLAYER: &str = "outlayer.testnet";
#[cfg(feature = "mainnet")]
const OUTLAYER: &str = "outlayer.near";
const CALLBACK_GAS: Gas = Gas::from_tgas(15);
const OVERHEAD_GAS: Gas = Gas::from_tgas(10);
/// A removed source stops working 72 hours after the DAO asks.
const REMOVE_DELAY_NS: u64 = 72 * 3600 * 1_000_000_000;
const MAX_CLAIM: usize = 400;
const MAX_BONDS: usize = 20;
const MAX_TAG: usize = 32;

#[ext_contract(ext_outlayer)]
trait OutLayer {
    fn request_execution(
        &mut self,
        source: serde_json::Value,
        resource_limits: serde_json::Value,
        input_data: String,
        secrets_ref: Option<serde_json::Value>,
        response_format: String,
        payer_account_id: Option<AccountId>,
    );
}

#[ext_contract(ext_self)]
trait SelfCallback {
    fn on_attempt(&mut self, round: u64, n: u8);
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Source {
    pub id: u32,
    /// every URL a round uses for this source must start with this text
    pub prefix: String,
    /// independence group: two sources in one group do not count as two
    pub group: String,
    /// when set, the time (ns) after which the source can no longer be used
    pub removed_at: Option<u64>,
}

#[near(serializers = [json])]
pub struct Pick {
    pub id: u32,
    pub url: String,
    /// extractor, for example json:$.count
    pub x: String,
    pub tol: f64,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Attempt {
    pub n: u8,
    pub start_block: u64,
    pub end_block: u64,
    /// "running", "ok" or the failure reason
    pub result: String,
    pub output: String,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Bond {
    pub writer: AccountId,
    pub word: String,
    pub chance_bps: u16,
    pub stake: U128,
    /// filled when the round settles
    pub payout: Option<U128>,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Round {
    /// short label such as lie_attempt, set by whoever opens the round
    pub tag: String,
    pub bonds: Vec<Bond>,
    /// the reserve at the moment the bonds were settled, so anyone can recompute the payouts
    pub reserve_before: Option<U128>,
    pub claim: String,
    /// the exact bytes every attempt sends
    pub input: String,
    /// base64url sha256 of input, the same value the box puts in its verdict
    pub ih: String,
    pub opener: AccountId,
    pub opened_block: u64,
    /// open, settled or void
    pub status: String,
    pub outcome: Option<String>,
    pub void_reason: Option<String>,
    pub failures: u8,
    pub pending: bool,
    pub attempts: Vec<Attempt>,
    pub verdict: Option<String>,
    /// original, audit or challenge
    pub role: String,
    /// the original round this one belongs to (itself for an original round)
    pub root: u64,
    /// challenge number 1 to 3 for a challenge, 0 otherwise
    pub level: u8,
    pub challenger: Option<AccountId>,
    /// the challenge bond, 0 for an original round or an audit
    pub bond: U128,
    /// the ruling that stood when this re-run was opened (T or F)
    pub standing_before: Option<String>,
    /// flipped, upheld or no_result, filled when the re-run closes
    pub appeal_verdict: Option<String>,
}

/// One ruling and everything that happened to it afterwards.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Case {
    pub root: u64,
    /// the ruling that stands right now (T or F)
    pub standing: String,
    /// challenges are accepted until this block; the case can be finished from this block on
    pub window_ends: u64,
    /// challenges used so far (0 to 3)
    pub level: u8,
    /// every re-run in order: the audit (if drawn) and each challenge
    pub rounds: Vec<u64>,
    pub audit_round: Option<u64>,
    pub finalized: bool,
    pub final_outcome: Option<String>,
}

#[near(serializers = [json])]
pub struct LadderConfig {
    /// judges for the top rung: 5 in production
    pub top_models: Vec<String>,
    /// the first challenge bond, in yoctoNEAR; it doubles each rung
    pub challenge_base: U128,
    pub window_blocks: u64,
    /// an open round nobody runs is voided after this many blocks
    pub run_deadline_blocks: u64,
    /// a run that never called back is counted as failed after this many blocks
    pub pending_timeout_blocks: u64,
    /// share of rulings that get a random audit, in basis points
    pub audit_rate_bps: u16,
}

impl Round {
    fn state(&self) -> State {
        State {
            status: match self.status.as_str() {
                "settled" => Status::Settled,
                "void" => Status::Void,
                _ => Status::Open,
            },
            failures: self.failures as usize,
            attempts: self.attempts.len(),
            pending: self.pending,
            outcome: self.outcome.clone(),
            void_reason: self.void_reason.clone(),
        }
    }
    fn save(&mut self, s: &State) {
        self.status = match s.status {
            Status::Open => "open",
            Status::Settled => "settled",
            Status::Void => "void",
        }
        .to_string();
        self.failures = s.failures as u8;
        self.pending = s.pending;
        self.outcome = s.outcome.clone();
        self.void_reason = s.void_reason.clone();
    }
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Courtroom {
    /// the only account that may add or remove sources
    dao: AccountId,
    courtroom_url: String,
    courtroom_hash: String,
    models: Vec<String>,
    secrets_account: Option<AccountId>,
    secrets_profile: Option<String>,
    /// the fixed stake every bond uses, in yoctoNEAR
    stake: u128,
    reserve: u128,
    /// money held inside open bonds and challenges
    locked: u128,
    top_models: Vec<String>,
    challenge_base: u128,
    window_blocks: u64,
    run_deadline_blocks: u64,
    pending_timeout_blocks: u64,
    audit_rate_bps: u16,
    cases: LookupMap<u64, Case>,
    balances: LookupMap<AccountId, u128>,
    next_round: u64,
    next_source: u32,
    sources: IterableMap<Source>,
    rounds: LookupMap<u64, Round>,
}

fn event(name: &str, body: serde_json::Value) {
    env::log_str(&format!(
        "EVENT_JSON:{}",
        serde_json::json!({"standard":"courtroom","version":"1","event":name,"data":body})
    ));
}

#[near]
impl Courtroom {
    #[init]
    pub fn new(
        dao: AccountId,
        courtroom_url: String,
        courtroom_hash: String,
        models: Vec<String>,
        secrets_account: Option<AccountId>,
        secrets_profile: Option<String>,
        stake: U128,
        ladder: LadderConfig,
    ) -> Self {
        assert!(logic::is_hex_sha256(&courtroom_hash), "courtroom_hash must be a sha256 in hex");
        assert!(models.len() == ladder::JUDGES_NORMAL, "exactly 3 judge models");
        assert!(
            ladder.top_models.len() == ladder::JUDGES_TOP || ladder.top_models.len() == ladder::JUDGES_NORMAL,
            "the top rung needs 5 judge models (3 only for a test deployment)"
        );
        assert!(ladder.challenge_base.0 > 0, "challenge bond must be above zero");
        assert!(ladder.window_blocks > 0 && ladder.run_deadline_blocks > 0 && ladder.pending_timeout_blocks > 0, "deadlines must be above zero");
        assert!(ladder.audit_rate_bps <= 10_000, "audit rate is at most 10000 basis points");
        Self {
            dao,
            courtroom_url,
            courtroom_hash,
            models,
            secrets_account,
            secrets_profile,
            stake: stake.0,
            reserve: 0,
            locked: 0,
            top_models: ladder.top_models,
            challenge_base: ladder.challenge_base.0,
            window_blocks: ladder.window_blocks,
            run_deadline_blocks: ladder.run_deadline_blocks,
            pending_timeout_blocks: ladder.pending_timeout_blocks,
            audit_rate_bps: ladder.audit_rate_bps,
            cases: LookupMap::new(b"c"),
            balances: LookupMap::new(b"b"),
            next_round: 1,
            next_source: 1,
            sources: IterableMap::new(b"s"),
            rounds: LookupMap::new(b"r"),
        }
    }

    // ---------- approved source list (S1 to S3) ----------

    pub fn add_source(&mut self, prefix: String, group: String) -> u32 {
        assert_eq!(env::predecessor_account_id(), self.dao, "only the DAO can add sources");
        assert!(prefix.starts_with("https://"), "https only");
        let id = self.next_source;
        self.next_source += 1;
        self.sources.insert(id, Source { id, prefix, group, removed_at: None });
        event("source_added", serde_json::json!({"id": id}));
        id
    }

    pub fn remove_source(&mut self, id: u32) {
        assert_eq!(env::predecessor_account_id(), self.dao, "only the DAO can remove sources");
        let mut s = self.sources.get(id).expect("unknown source");
        s.removed_at = Some(env::block_timestamp() + REMOVE_DELAY_NS);
        self.sources.insert(id, s);
        event("source_removal_scheduled", serde_json::json!({"id": id}));
    }

    pub fn sources(&self) -> Vec<Source> {
        self.sources.values()
    }

    fn live_source(&self, id: u32) -> Source {
        let s = self.sources.get(id).expect("source is not on the approved list");
        if let Some(t) = s.removed_at {
            assert!(env::block_timestamp() < t, "source has been removed");
        }
        s
    }

    // ---------- rounds (R1) ----------

    /// Check a list of sources against the approved list. Returns the source frames for the request.
    fn validate_picks(&self, kind: &str, picks: &[Pick]) -> Vec<serde_json::Value> {
        let mut src = vec![];
        let mut groups: Vec<String> = vec![];
        for p in picks {
            let s = self.live_source(p.id);
            assert!(p.url.starts_with(&s.prefix), "url is outside the approved source");
            assert!(p.url.starts_with("https://") && !p.url.contains('@'), "https only");
            assert!(p.tol >= 0.0, "tolerance cannot be negative");
            if kind == "data" {
                assert!(p.x.starts_with("json:") || p.x.starts_with("find:"), "bad extractor");
            }
            if !groups.contains(&s.group) {
                groups.push(s.group.clone());
            }
            src.push(serde_json::json!({"i": p.id, "u": p.url, "x": p.x, "tol": p.tol, "g": s.group}));
        }
        assert!(groups.len() >= 2, "need two independent source groups");
        src
    }

    fn seed_for(rd: u64) -> u64 {
        let b = env::sha256(format!("round-{}", rd).as_bytes());
        u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as u64
    }

    /// Copy a stored request for a re-run: new round number, new seed, new block, and optionally new sources and judges.
    fn rerun_input(
        &self,
        root_input: &str,
        rd: u64,
        src: Option<Vec<serde_json::Value>>,
        models: Option<Vec<String>>,
    ) -> (String, String) {
        let mut v: serde_json::Value = serde_json::from_str(root_input).expect("stored input");
        v["rd"] = rd.into();
        v["seed"] = Self::seed_for(rd).into();
        v["asof"] = env::block_height().into();
        if let Some(s) = src {
            v["src"] = s.into();
        }
        if let Some(m) = models {
            v["m"] = m.into();
        }
        let input = v.to_string();
        let ih = logic::base64url(&env::sha256(input.as_bytes()));
        (input, ih)
    }

    #[allow(clippy::too_many_arguments)]
    fn rerun_round(
        &self,
        root: &Round,
        role: &str,
        level: u8,
        challenger: Option<AccountId>,
        bond: u128,
        standing: &str,
        input: String,
        ih: String,
    ) -> Round {
        Round {
            tag: role.to_string(),
            bonds: vec![],
            reserve_before: None,
            claim: root.claim.clone(),
            input,
            ih,
            opener: env::predecessor_account_id(),
            opened_block: env::block_height(),
            status: "open".into(),
            outcome: None,
            void_reason: None,
            failures: 0,
            pending: false,
            attempts: vec![],
            verdict: None,
            role: role.to_string(),
            root: root.root,
            level,
            challenger,
            bond: U128(bond),
            standing_before: Some(standing.to_string()),
            appeal_verdict: None,
        }
    }

    /// Open a round. The contract builds the exact request bytes once and stores them.
    /// Every attempt sends those same bytes (R1). Anyone may open a round.
    pub fn open_round(
        &mut self,
        claim: String,
        kind: String,
        cv: Option<String>,
        picks: Vec<Pick>,
        terms: Vec<String>,
        tag: Option<String>,
    ) -> u64 {
        let tag = tag.unwrap_or_default();
        assert!(tag.len() <= MAX_TAG, "tag is at most 32 characters");
        assert!(!claim.is_empty() && claim.len() <= MAX_CLAIM, "claim must be 1 to 400 characters");
        assert!(kind == "data" || kind == "prose", "kind must be data or prose");
        if kind == "data" {
            let n = cv.as_ref().expect("a number sentence needs cv");
            assert!(logic::is_number(n), "cv must be a number");
        }
        assert!(picks.len() >= 2 && picks.len() <= 3, "2 or 3 sources");
        let src = self.validate_picks(&kind, &picks);

        let rd = self.next_round;
        self.next_round += 1;
        let th = logic::hex(&env::sha256(claim.as_bytes()));
        let seed_bytes = env::sha256(format!("round-{}", rd).as_bytes());
        let seed = u32::from_be_bytes([seed_bytes[0], seed_bytes[1], seed_bytes[2], seed_bytes[3]]) as u64;
        let mut input = serde_json::json!({
            "v": 1, "rd": rd, "kind": kind, "th": th, "claim": claim,
            "src": src, "m": self.models, "seed": seed, "asof": env::block_height(),
        });
        if let Some(c) = cv {
            input["cv"] = c.into();
        }
        if !terms.is_empty() {
            input["terms"] = serde_json::json!(terms);
        }
        let input = input.to_string();
        let ih = logic::base64url(&env::sha256(input.as_bytes()));
        self.rounds.insert(
            rd,
            Round {
                tag,
                bonds: vec![],
                reserve_before: None,
                claim,
                input,
                ih,
                opener: env::predecessor_account_id(),
                opened_block: env::block_height(),
                status: "open".into(),
                outcome: None,
                void_reason: None,
                failures: 0,
                pending: false,
                attempts: vec![],
                verdict: None,
                role: "original".into(),
                root: rd,
                level: 0,
                challenger: None,
                bond: U128(0),
                standing_before: None,
                appeal_verdict: None,
            },
        );
        event("round_opened", serde_json::json!({"round": rd, "block": env::block_height()}));
        rd
    }

    // ---------- attempts (R2 to R6) ----------

    /// Ask the sealed box to rule on a round. Anyone may call this and pay the run fee.
    /// OutLayer refunds whatever it does not use to the caller.
    #[payable]
    pub fn attempt(&mut self, round: u64) -> Promise {
        let (input, n) = self.begin_attempt(round);

        let secrets_ref = match (&self.secrets_account, &self.secrets_profile) {
            (Some(a), Some(p)) => Some(serde_json::json!({"profile": p, "account_id": a})),
            _ => None,
        };
        let source = serde_json::json!({"WasmUrl": {
            "url": self.courtroom_url, "hash": self.courtroom_hash, "build_target": "wasm32-wasip2"}});
        let limits =
            serde_json::json!({"max_instructions": 1_000_000_000u64, "max_memory_mb": 128, "max_execution_seconds": 90});
        let gas = env::prepaid_gas().saturating_sub(CALLBACK_GAS).saturating_sub(OVERHEAD_GAS);
        ext_outlayer::ext(OUTLAYER.parse().unwrap())
            .with_attached_deposit(env::attached_deposit())
            .with_static_gas(gas)
            .with_unused_gas_weight(1)
            .request_execution(source, limits, input, secrets_ref, "Text".into(), Some(env::predecessor_account_id()))
            .then(ext_self::ext(env::current_account_id()).with_static_gas(CALLBACK_GAS).on_attempt(round, n))
    }

    /// The state change of an attempt: one live attempt at a time, at most three. Returns the stored request bytes and the attempt number.
    fn begin_attempt(&mut self, round: u64) -> (String, u8) {
        let mut r = self.rounds.get(&round).expect("unknown round");
        let mut st = r.state();
        if let Err(e) = st.can_start() {
            env::panic_str(e);
        }
        st.start();
        let n = st.attempts as u8;
        r.save(&st);
        r.attempts.push(Attempt {
            n,
            start_block: env::block_height(),
            end_block: 0,
            result: "running".into(),
            output: String::new(),
        });
        let input = r.input.clone();
        self.rounds.insert(round, r);
        event("attempt_started", serde_json::json!({"round": round, "n": n, "block": env::block_height()}));

        (input, n)
    }

    #[private]
    pub fn on_attempt(&mut self, round: u64, n: u8, #[callback_result] result: Result<Option<String>, PromiseError>) {
        let mut r = self.rounds.get(&round).expect("unknown round");
        if !r.attempts.iter().any(|a| a.n == n && a.result == "running") {
            // The attempt was already timed out by expire_round, so a late answer changes nothing.
            event("attempt_ignored", serde_json::json!({"round": round, "n": n}));
            return;
        }
        let out: Option<String> = result.unwrap_or(None);
        let judged = judge_output(round, &r.ih, out.as_deref());
        let mut st = r.state();
        let was_open = st.status == Status::Open;
        st.finish(judged.clone());
        r.save(&st);
        let label = match &judged {
            Judged::Final(_) => "ok".to_string(),
            Judged::SourcesDisagree => "SRC_DISAGREE".to_string(),
            Judged::Failed(why) => why.clone(),
        };
        if let Some(a) = r.attempts.iter_mut().find(|a| a.n == n) {
            a.end_block = env::block_height();
            a.result = label.clone();
            a.output = out.clone().unwrap_or_default();
        }
        // R5: only the first valid ruling is kept as the verdict.
        if was_open && matches!(judged, Judged::Final(_) | Judged::SourcesDisagree) {
            r.verdict = out;
        }
        if was_open && r.status != "open" {
            self.round_closed(&mut r, round);
        }
        event(
            "attempt_done",
            serde_json::json!({"round": round, "n": n, "result": label, "status": r.status, "block": env::block_height()}),
        );
        self.rounds.insert(round, r);
    }

    // ---------- bonds and money ----------

    /// Put NEAR on your ledger balance. Bonds are paid from this balance.
    #[payable]
    pub fn deposit(&mut self) {
        let who = env::predecessor_account_id();
        let cur = self.balances.get(&who).unwrap_or(0);
        self.balances.insert(who, cur + env::attached_deposit().as_yoctonear());
    }

    /// Take NEAR out of your ledger balance.
    pub fn withdraw(&mut self, amount: U128) -> Promise {
        let who = env::predecessor_account_id();
        let cur = self.balances.get(&who).unwrap_or(0);
        assert!(cur >= amount.0 && amount.0 > 0, "not enough balance");
        self.balances.insert(who.clone(), cur - amount.0);
        Promise::new(who).transfer(NearToken::from_yoctonear(amount.0))
    }

    /// Anyone may add to the reserve that pays winners when losers' forfeits are not enough.
    #[payable]
    pub fn fund_reserve(&mut self) {
        self.reserve += env::attached_deposit().as_yoctonear();
    }

    /// Bond the fixed stake on a sentence with one word: is, very likely, likely or disputed.
    /// Only before any attempt has started, so nobody can bond once a ruling is possible.
    pub fn bond(&mut self, round: u64, word: String) {
        let chance = word_bps(&word).expect("word must be: is, very likely, likely or disputed");
        let mut r = self.rounds.get(&round).expect("unknown round");
        assert!(r.role == "original", "bonds go on the original ruling only");
        assert!(r.status == "open" && r.attempts.is_empty(), "bonds close when the first attempt starts");
        assert!(r.bonds.len() < MAX_BONDS, "this sentence already has the most bonds allowed");
        let who = env::predecessor_account_id();
        assert!(r.bonds.iter().all(|b| b.writer != who), "you already bonded this sentence");
        let cur = self.balances.get(&who).unwrap_or(0);
        assert!(cur >= self.stake, "deposit first: not enough balance for the stake");
        self.balances.insert(who.clone(), cur - self.stake);
        self.locked += self.stake;
        r.bonds.push(Bond { writer: who.clone(), word, chance_bps: chance, stake: U128(self.stake), payout: None });
        event("bonded", serde_json::json!({"round": round, "writer": who}));
        self.rounds.insert(round, r);
    }

    fn settle_bonds(&mut self, r: &mut Round, outcome: Option<Outcome>) {
        if r.bonds.is_empty() {
            return;
        }
        let entries: Vec<Entry> = r.bonds.iter().map(|b| Entry { stake: b.stake.0, chance_bps: b.chance_bps }).collect();
        r.reserve_before = Some(U128(self.reserve));
        let s = settle(&entries, outcome, self.reserve);
        let total_stake: u128 = entries.iter().map(|e| e.stake).sum();
        self.locked -= total_stake;
        self.reserve = s.reserve_after;
        for (b, p) in r.bonds.iter_mut().zip(s.payouts.iter()) {
            let cur = self.balances.get(&b.writer).unwrap_or(0);
            self.balances.insert(b.writer.clone(), cur + p);
            b.payout = Some(U128(*p));
        }
        event("bonds_settled", serde_json::json!({"bonds": r.bonds.len(), "reserve": self.reserve.to_string()}));
    }

    // ---------- challenges, audits, finishing a case ----------

    /// Called when a round closes (ruled, void or timed out). Originals open a case; re-runs update it.
    fn round_closed(&mut self, r: &mut Round, round: u64) {
        if r.role == "original" {
            match r.outcome.as_deref() {
                Some(o @ ("T" | "F")) => {
                    let mut case = Case {
                        root: round,
                        standing: o.to_string(),
                        window_ends: env::block_height() + self.window_blocks,
                        level: 0,
                        rounds: vec![],
                        audit_round: None,
                        finalized: false,
                        final_outcome: None,
                    };
                    let mut seed = [0u8; 32];
                    seed.copy_from_slice(&env::random_seed());
                    if audit_hit(&env::sha256(&audit_preimage(&seed, round, round)), self.audit_rate_bps) {
                        let id = self.next_round;
                        self.next_round += 1;
                        let (input, ih) = self.rerun_input(&r.input, id, None, None);
                        let a = self.rerun_round(r, "audit", 0, None, 0, o, input, ih);
                        self.rounds.insert(id, a);
                        case.rounds.push(id);
                        case.audit_round = Some(id);
                        event("audit_drawn", serde_json::json!({"root": round, "audit_round": id, "block": env::block_height()}));
                    }
                    self.cases.insert(round, case);
                    event("case_opened", serde_json::json!({"root": round, "standing": o}));
                }
                // unsure, disputed by the judges, or void: nothing to appeal, every stake goes back now
                _ => self.settle_bonds(r, None),
            }
            return;
        }
        let mut case = self.cases.get(&r.root).expect("case");
        let standing = Side::from_letter(&case.standing).expect("standing");
        let v = ladder::appeal_verdict(standing, r.outcome.as_deref());
        r.appeal_verdict = Some(v.word().to_string());
        if v == Verdict::Flipped {
            case.standing = r.outcome.clone().unwrap_or_default();
        }
        let top = r.role == "challenge" && r.level >= ladder::MAX_LEVEL;
        case.window_ends = if top { env::block_height() } else { env::block_height() + self.window_blocks };
        event(
            "rerun_closed",
            serde_json::json!({"root": r.root, "round": round, "role": r.role, "verdict": v.word(), "standing": case.standing}),
        );
        self.cases.insert(r.root, case);
    }

    /// Challenge the ruling that stands. Posts the next bond from your ledger balance (base, then double, then four times).
    /// The picks must keep every earlier source and add at least one. The top rung asks five judges.
    pub fn challenge(&mut self, round: u64, picks: Vec<Pick>) -> u64 {
        let base_round = self.rounds.get(&round).expect("unknown round");
        let root = base_round.root;
        let root_round = self.rounds.get(&root).expect("unknown round");
        let mut case = self.cases.get(&root).expect("this round has no ruling to challenge");
        assert!(!case.finalized, "this case is already finished");
        let last_id = case.rounds.last().copied().unwrap_or(root);
        let last = self.rounds.get(&last_id).expect("round");
        assert!(last.status != "open", "a re-run is already open on this case");
        assert!(env::block_height() < case.window_ends, "the challenge window is closed");
        assert!(case.level < ladder::MAX_LEVEL, "the top of the ladder is reached");
        let level = case.level + 1;
        let bond = ladder::challenge_bond(self.challenge_base, level).expect("bond");
        let who = env::predecessor_account_id();
        let cur = self.balances.get(&who).unwrap_or(0);
        assert!(cur >= bond, "deposit first: not enough balance for the challenge bond");

        let kind = serde_json::from_str::<serde_json::Value>(&root_round.input).expect("stored input")["kind"]
            .as_str()
            .unwrap_or("data")
            .to_string();
        let prev: Vec<u32> = serde_json::from_str::<serde_json::Value>(&last.input).expect("stored input")["src"]
            .as_array()
            .map(|a| a.iter().filter_map(|s| s["i"].as_u64().map(|x| x as u32)).collect())
            .unwrap_or_default();
        let next: Vec<u32> = picks.iter().map(|p| p.id).collect();
        if let Err(e) = ladder::adds_a_source(&prev, &next) {
            env::panic_str(e);
        }
        let src = self.validate_picks(&kind, &picks);
        let models = if ladder::judge_count(level) == ladder::JUDGES_TOP { self.top_models.clone() } else { self.models.clone() };

        self.balances.insert(who.clone(), cur - bond);
        self.locked += bond;
        let id = self.next_round;
        self.next_round += 1;
        let (input, ih) = self.rerun_input(&root_round.input, id, Some(src), Some(models));
        let rr = self.rerun_round(&root_round, "challenge", level, Some(who.clone()), bond, &case.standing, input, ih);
        self.rounds.insert(id, rr);
        case.level = level;
        case.rounds.push(id);
        self.cases.insert(root, case);
        event(
            "challenged",
            serde_json::json!({"root": root, "round": id, "level": level, "challenger": who, "bond": bond.to_string()}),
        );
        id
    }

    /// Finish a case: pay the sentence bonds on the ruling that stands and settle every challenge bond. Anyone may call this.
    /// It works once the window has closed and no re-run is open, so a pending audit must be run (or time out) first.
    pub fn finalize(&mut self, round: u64) {
        let root = self.rounds.get(&round).expect("unknown round").root;
        let mut case = self.cases.get(&root).expect("this round has no ruling to finish");
        assert!(!case.finalized, "this case is already finished");
        let last_id = case.rounds.last().copied().unwrap_or(root);
        let last = self.rounds.get(&last_id).expect("round");
        assert!(last.status != "open", "a re-run is still open: run it or let it time out");
        assert!(env::block_height() >= case.window_ends, "the challenge window is still open");

        let side = Side::from_letter(&case.standing).expect("standing");
        let outcome = if side == Side::True { Outcome::True } else { Outcome::False };
        let mut ins = vec![];
        let mut reruns = vec![];
        for id in &case.rounds {
            let a = self.rounds.get(id).expect("round");
            ins.push(AppealIn {
                bond: a.bond.0,
                standing_before: Side::from_letter(a.standing_before.as_deref().unwrap_or("T")).unwrap_or(Side::True),
                verdict: match a.appeal_verdict.as_deref() {
                    Some("flipped") => Verdict::Flipped,
                    Some("upheld") => Verdict::Upheld,
                    _ => Verdict::NoResult,
                },
            });
            reruns.push(a);
        }
        let cs = ladder::settle_challengers(&ins, side);
        let total_bonds: u128 = ins.iter().map(|i| i.bond).sum();
        for (a, c) in reruns.iter().zip(cs.credits.iter()) {
            if let (Some(who), true) = (&a.challenger, *c > 0) {
                let cur = self.balances.get(who).unwrap_or(0);
                self.balances.insert(who.clone(), cur + c);
            }
        }
        self.locked -= total_bonds;
        self.reserve += cs.to_reserve;
        // Lost challenge bonds are in the reserve first, so they can pay the writers who were right.
        let mut r0 = self.rounds.get(&root).expect("root");
        self.settle_bonds(&mut r0, Some(outcome));
        self.rounds.insert(root, r0);
        case.finalized = true;
        case.final_outcome = Some(case.standing.clone());
        event(
            "case_finished",
            serde_json::json!({"root": root, "outcome": case.standing, "challenge_bonds": total_bonds.to_string(), "to_reserve": cs.to_reserve.to_string()}),
        );
        self.cases.insert(root, case);
    }

    /// Nothing gets stuck. Anyone may call this on a round that nobody ran in time (it becomes void and every bond goes back),
    /// or whose run never called back (that run is counted as failed).
    pub fn expire_round(&mut self, round: u64) {
        let mut r = self.rounds.get(&round).expect("unknown round");
        assert!(r.status == "open", "round is already closed");
        let mut st = r.state();
        let now = env::block_height();
        if r.pending {
            let last = r.attempts.last_mut().expect("attempt");
            assert!(now >= last.start_block + self.pending_timeout_blocks, "the run may still finish");
            last.end_block = now;
            last.result = "TIMED_OUT".into();
            st.finish(Judged::Failed("TIMED_OUT".into()));
        } else {
            let since = r.attempts.last().map(|a| a.end_block).unwrap_or(0).max(r.opened_block);
            assert!(now >= since + self.run_deadline_blocks, "this round has not been waiting long enough");
            st.status = Status::Void;
            st.void_reason = Some("TIMED_OUT".into());
        }
        r.save(&st);
        event("round_expired", serde_json::json!({"round": round, "status": r.status}));
        if r.status != "open" {
            self.round_closed(&mut r, round);
        }
        self.rounds.insert(round, r);
    }

    // ---------- views ----------

    pub fn get_case(&self, round: u64) -> Option<Case> {
        let root = self.rounds.get(&round)?.root;
        self.cases.get(&root)
    }

    /// (reserve, money locked in open bonds and challenges)
    pub fn accounting(&self) -> (U128, U128) {
        (U128(self.reserve), U128(self.locked))
    }

    pub fn challenge_bond(&self, level: u8) -> Option<U128> {
        ladder::challenge_bond(self.challenge_base, level).map(U128)
    }

    pub fn top_models(&self) -> Vec<String> {
        self.top_models.clone()
    }

    pub fn balance(&self, account: AccountId) -> U128 {
        U128(self.balances.get(&account).unwrap_or(0))
    }

    pub fn reserve(&self) -> U128 {
        U128(self.reserve)
    }

    pub fn stake(&self) -> U128 {
        U128(self.stake)
    }

    pub fn get_round(&self, round: u64) -> Option<Round> {
        self.rounds.get(&round)
    }

    pub fn courtroom(&self) -> (String, String) {
        (self.courtroom_url.clone(), self.courtroom_hash.clone())
    }

    pub fn dao(&self) -> AccountId {
        self.dao.clone()
    }

    pub fn models(&self) -> Vec<String> {
        self.models.clone()
    }

    pub fn round_count(&self) -> u64 {
        self.next_round - 1
    }
}

#[cfg(test)]
mod tests;
