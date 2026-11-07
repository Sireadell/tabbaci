//! The courtroom contract.
//! No upgrade method and no access keys: once deployed and its keys are deleted, nobody can change it.
//! Rules live in the courtroom-rules crate and are tested without a blockchain.
use courtroom_rules as logic;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use logic::{judge_output, Judged, State, Status};
use bond_payout::{settle, word_bps, Entry, Outcome};
use near_sdk::json_types::U128;
use near_sdk::store::{IterableMap, LookupMap};
use near_sdk::{env, ext_contract, near, serde_json, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError};

const OUTLAYER: &str = "outlayer.testnet";
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
    balances: LookupMap<AccountId, u128>,
    next_round: u64,
    next_source: u32,
    sources: IterableMap<u32, Source>,
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
    ) -> Self {
        assert!(logic::is_hex_sha256(&courtroom_hash), "courtroom_hash must be a sha256 in hex");
        assert!(models.len() == 3, "exactly 3 judge models");
        Self {
            dao,
            courtroom_url,
            courtroom_hash,
            models,
            secrets_account,
            secrets_profile,
            stake: stake.0,
            reserve: 0,
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
        let mut s = self.sources.get(&id).cloned().expect("unknown source");
        s.removed_at = Some(env::block_timestamp() + REMOVE_DELAY_NS);
        self.sources.insert(id, s);
        event("source_removal_scheduled", serde_json::json!({"id": id}));
    }

    pub fn sources(&self) -> Vec<Source> {
        self.sources.iter().map(|(_, s)| s.clone()).collect()
    }

    fn live_source(&self, id: u32) -> Source {
        let s = self.sources.get(&id).cloned().expect("source is not on the approved list");
        if let Some(t) = s.removed_at {
            assert!(env::block_timestamp() < t, "source has been removed");
        }
        s
    }

    // ---------- rounds (R1) ----------

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
            assert!(n.parse::<f64>().is_ok(), "cv must be a number");
        }
        assert!(picks.len() >= 2 && picks.len() <= 3, "2 or 3 sources");
        let mut src = vec![];
        let mut groups: Vec<String> = vec![];
        for p in &picks {
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

        let rd = self.next_round;
        self.next_round += 1;
        let th = hex::encode(env::sha256(claim.as_bytes()));
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
        let ih = URL_SAFE_NO_PAD.encode(env::sha256(input.as_bytes()));
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
        let mut r = self.rounds.get(&round).cloned().expect("unknown round");
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

    #[private]
    pub fn on_attempt(&mut self, round: u64, n: u8, #[callback_result] result: Result<Option<String>, PromiseError>) {
        let mut r = self.rounds.get(&round).cloned().expect("unknown round");
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
        // Money: when the round closes, pay or return every bond with the fixed rule.
        if was_open && r.status != "open" {
            let outcome = match r.outcome.as_deref() {
                Some("T") => Some(Outcome::True),
                Some("F") => Some(Outcome::False),
                _ => None,
            };
            self.settle_bonds(&mut r, outcome);
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
        let cur = self.balances.get(&who).copied().unwrap_or(0);
        self.balances.insert(who, cur + env::attached_deposit().as_yoctonear());
    }

    /// Take NEAR out of your ledger balance.
    pub fn withdraw(&mut self, amount: U128) -> Promise {
        let who = env::predecessor_account_id();
        let cur = self.balances.get(&who).copied().unwrap_or(0);
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
        let mut r = self.rounds.get(&round).cloned().expect("unknown round");
        assert!(r.status == "open" && r.attempts.is_empty(), "bonds close when the first attempt starts");
        assert!(r.bonds.len() < MAX_BONDS, "this sentence already has the most bonds allowed");
        let who = env::predecessor_account_id();
        assert!(r.bonds.iter().all(|b| b.writer != who), "you already bonded this sentence");
        let cur = self.balances.get(&who).copied().unwrap_or(0);
        assert!(cur >= self.stake, "deposit first: not enough balance for the stake");
        self.balances.insert(who.clone(), cur - self.stake);
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
        self.reserve = s.reserve_after;
        for (b, p) in r.bonds.iter_mut().zip(s.payouts.iter()) {
            let cur = self.balances.get(&b.writer).copied().unwrap_or(0);
            self.balances.insert(b.writer.clone(), cur + p);
            b.payout = Some(U128(*p));
        }
        event("bonds_settled", serde_json::json!({"bonds": r.bonds.len(), "reserve": self.reserve.to_string()}));
    }

    // ---------- views ----------

    pub fn balance(&self, account: AccountId) -> U128 {
        U128(self.balances.get(&account).copied().unwrap_or(0))
    }

    pub fn reserve(&self) -> U128 {
        U128(self.reserve)
    }

    pub fn stake(&self) -> U128 {
        U128(self.stake)
    }

    pub fn get_round(&self, round: u64) -> Option<Round> {
        self.rounds.get(&round).cloned()
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
