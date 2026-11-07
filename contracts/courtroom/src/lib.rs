use near_sdk::store::LookupMap;
use near_sdk::{
    env, ext_contract, near, serde_json, AccountId, Gas, NearToken, PanicOnDefault, Promise,
    PromiseError,
};

const OUTLAYER: &str = "outlayer.testnet";
const CALLBACK_GAS: Gas = Gas::from_tgas(10);
const OVERHEAD_GAS: Gas = Gas::from_tgas(10);

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
    fn on_result(&mut self, id: u64);
}

/// One recorded answer: what was asked, when, and what came back.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Record {
    pub asker: AccountId,
    pub input: String,
    pub start_block: u64,
    pub end_block: u64,
    pub ok: bool,
    pub output: String,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Courtroom {
    next_id: u64,
    records: LookupMap<u64, Record>,
}

#[near]
impl Courtroom {
    #[init]
    pub fn new() -> Self {
        Self { next_id: 0, records: LookupMap::new(b"r") }
    }

    /// Ask the sealed box to run a program on an input. Anyone can call.
    /// wasm_url and wasm_hash name the exact program; secrets_profile is optional.
    #[payable]
    pub fn ask(
        &mut self,
        wasm_url: String,
        wasm_hash: String,
        input: String,
        secrets_account: Option<AccountId>,
        secrets_profile: Option<String>,
        max_seconds: Option<u64>,
    ) -> Promise {
        let id = self.next_id;
        self.next_id += 1;
        self.records.insert(
            id,
            Record {
                asker: env::predecessor_account_id(),
                input: input.clone(),
                start_block: env::block_height(),
                end_block: 0,
                ok: false,
                output: String::new(),
            },
        );
        env::log_str(&format!("EVENT ask id={} block={}", id, env::block_height()));
        let secrets_ref = match (secrets_account, secrets_profile) {
            (Some(a), Some(p)) => Some(serde_json::json!({"profile": p, "account_id": a})),
            _ => None,
        };
        let source = serde_json::json!({"WasmUrl": {
            "url": wasm_url, "hash": wasm_hash, "build_target": "wasm32-wasip2"}});
        let limits = serde_json::json!({
            "max_instructions": 1_000_000_000u64,
            "max_memory_mb": 128,
            "max_execution_seconds": max_seconds.unwrap_or(60)});
        let gas = env::prepaid_gas()
            .saturating_sub(CALLBACK_GAS)
            .saturating_sub(OVERHEAD_GAS);
        ext_outlayer::ext(OUTLAYER.parse().unwrap())
            .with_attached_deposit(env::attached_deposit())
            .with_static_gas(gas)
            .with_unused_gas_weight(1)
            .request_execution(source, limits, input, secrets_ref, "Text".into(), None)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(CALLBACK_GAS)
                    .on_result(id),
            )
    }

    #[private]
    pub fn on_result(
        &mut self,
        id: u64,
        #[callback_result] result: Result<Option<String>, PromiseError>,
    ) {
        let mut rec = self.records.get(&id).cloned().expect("unknown id");
        rec.end_block = env::block_height();
        match result {
            Ok(Some(out)) => {
                rec.ok = true;
                rec.output = out;
            }
            Ok(None) => rec.output = "no output".to_string(),
            Err(_) => rec.output = "failed".to_string(),
        }
        env::log_str(&format!(
            "EVENT done id={} blocks={} ok={}",
            id,
            rec.end_block - rec.start_block,
            rec.ok
        ));
        self.records.insert(id, rec);
    }

    pub fn get(&self, id: u64) -> Option<Record> {
        self.records.get(&id).cloned()
    }

    pub fn count(&self) -> u64 {
        self.next_id
    }
}

#[allow(dead_code)]
fn _unused(_: NearToken) {}
