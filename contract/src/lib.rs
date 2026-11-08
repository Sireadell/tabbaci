//! Sealed batch bets, first test.
//!
//! One batch has one hidden number `k` made inside the OutLayer locked chip. The chip hands
//! out only the public lock `k*G` when the batch opens. Traders seal orders to that lock.
//! After the close block the contract asks the chip for `k`, checks `k*G == lock` with
//! NEAR's built-in curve maths, and opens every order itself.
use near_sdk::borsh;
use near_sdk::{env, near, AccountId, Gas, GasWeight, NearToken, PanicOnDefault};

const OUTLAYER: &str = "outlayer.testnet";
const MAX_ORDERS: usize = 100;
const MAX_KEY_ATTEMPTS: u8 = 3;
const CALLBACK_GAS: Gas = Gas::from_tgas(30);
const OVERHEAD_GAS: Gas = Gas::from_tgas(15);

/// The curve's base point G = (1, 2), in the little-endian layout NEAR's host function wants.
const G_LE: [u8; 64] = {
    let mut g = [0u8; 64];
    g[0] = 1;
    g[32] = 2;
    g
};

#[near(serializers = [borsh])]
#[derive(Clone, PartialEq)]
pub enum State {
    WaitingForLock,
    Open,
    Opened,
    Void,
}

#[near(serializers = [borsh])]
#[derive(Clone)]
pub struct Sealed {
    pub from: AccountId,
    /// C1 = r*G, 64 bytes little-endian
    pub c1: Vec<u8>,
    /// the scrambled 32-byte order
    pub ct: Vec<u8>,
    pub deposit: NearToken,
}

#[near(serializers = [borsh])]
#[derive(Clone)]
pub struct Opened {
    pub from: AccountId,
    /// 0 = fine, 1 = bad format (deposit refunded)
    pub status: u8,
    pub side: u8,
    pub price_bp: u16,
    pub amount: NearToken,
}

#[near(serializers = [borsh])]
#[derive(Clone)]
pub struct Batch {
    pub close_height: u64,
    pub state: State,
    pub lock: Vec<u8>,
    pub key_attempts: u8,
    pub orders: Vec<Sealed>,
    pub opened: Vec<Opened>,
}

/// A writer's page, scrambled to the batch lock. Kept on chain so the text can never go missing.
#[near(serializers = [borsh])]
#[derive(Clone)]
pub struct Words {
    pub from: AccountId,
    pub c1: Vec<u8>,
    pub ct: Vec<u8>,
    /// keccak256 of the plain text, fixed before the key exists
    pub text_hash: Vec<u8>,
}

const MAX_WORDS: usize = 20;
const MAX_WORDS_BYTES: usize = 8192;
/// storage price per byte, in yoctoNEAR (1e19), a little above the real price
const BYTE_COST: u128 = 10_000_000_000_000_000_000;

fn wkey(id: u64) -> Vec<u8> {
    let mut k = b"w".to_vec();
    k.extend_from_slice(&id.to_le_bytes());
    k
}

fn load_words(id: u64) -> Vec<Words> {
    env::storage_read(&wkey(id)).map(|v| borsh::from_slice(&v).expect("bad words")).unwrap_or_default()
}

fn words_json(w: &[Words]) -> String {
    let mut o = String::from("[");
    for (i, x) in w.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str(&format!(
            "{{\"from\":{},\"c1\":\"{}\",\"ct\":\"{}\",\"text_hash\":\"{}\"}}",
            jstr(x.from.as_str()),
            hex_encode(&x.c1),
            hex_encode(&x.ct),
            hex_encode(&x.text_hash)
        ));
    }
    o.push(']');
    o
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Sealbox {
    pub program_url: String,
    pub program_hash: String,
    pub secrets_account: AccountId,
    pub secrets_profile: String,
}

/// Same output as the old json!() version (keys sorted), built by hand to keep the wasm small.
fn event(name: &str, data: String) {
    env::log_str(&format!(
        "EVENT_JSON:{{\"data\":{},\"event\":\"{}\",\"standard\":\"sealbox\",\"version\":\"1.0.0\"}}",
        data, name
    ));
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// JSON string literal with quotes, escaped like serde_json does.
fn jstr(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                o.push_str("\\u00");
                o.push(HEX[(c as usize) >> 4] as char);
                o.push(HEX[(c as usize) & 15] as char);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn hex_encode(b: &[u8]) -> String {
    let mut o = String::with_capacity(b.len() * 2);
    for x in b {
        o.push(HEX[(x >> 4) as usize] as char);
        o.push(HEX[(x & 15) as usize] as char);
    }
    o
}

fn nib(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if b.len() % 2 != 0 {
        return None;
    }
    let mut o = Vec::with_capacity(b.len() / 2);
    for p in b.chunks(2) {
        o.push((nib(p[0])? << 4) | nib(p[1])?);
    }
    Some(o)
}

/// Pull a string field out of a flat JSON object like {"lock":"ab12..."}.
fn json_str_field(s: &str, name: &str) -> Option<String> {
    let pat = format!("\"{}\"", name);
    let mut rest = &s[s.find(&pat)? + pat.len()..];
    rest = rest.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

/// Is k*G equal to the lock? One built-in curve call.
fn key_matches_lock(key_le: &[u8], lock_le: &[u8]) -> bool {
    let mut input = Vec::with_capacity(96);
    input.extend_from_slice(&G_LE);
    input.extend_from_slice(key_le);
    env::alt_bn128_g1_multiexp(&input) == lock_le
}

/// Open one order: S = k*C1, pad = keccak("order-v1" || S big-endian), order = ct XOR pad.
pub fn open_order(key_le: &[u8], c1_le: &[u8], ct: &[u8]) -> [u8; 32] {
    let mut input = Vec::with_capacity(96);
    input.extend_from_slice(c1_le);
    input.extend_from_slice(key_le);
    let s_le = env::alt_bn128_g1_multiexp(&input);
    let mut pre = Vec::with_capacity(72);
    pre.extend_from_slice(b"order-v1");
    for half in [&s_le[..32], &s_le[32..]] {
        pre.extend(half.iter().rev());
    }
    let pad = env::keccak256_array(&pre);
    let mut o = [0u8; 32];
    for i in 0..32 {
        o[i] = ct[i] ^ pad[i];
    }
    o
}

/// Order layout (32 bytes): [0]=version 1, [1]=side 0/1, [2..4]=price in basis points (1..9999),
/// [4..20]=amount in yoctoNEAR (u128 big-endian), [20..32]=random filler.
fn parse_order(o: &[u8; 32]) -> Option<(u8, u16, u128)> {
    if o[0] != 1 || o[1] > 1 {
        return None;
    }
    let price = u16::from_be_bytes([o[2], o[3]]);
    if price == 0 || price >= 10_000 {
        return None;
    }
    let mut a = [0u8; 16];
    a.copy_from_slice(&o[4..20]);
    Some((o[1], price, u128::from_be_bytes(a)))
}


fn bytes_json(b: &[u8]) -> String {
    let mut o = String::from("[");
    for (i, x) in b.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str(&x.to_string());
    }
    o.push(']');
    o
}

fn batch_json(b: &Batch) -> String {
    let state = match b.state {
        State::WaitingForLock => "WaitingForLock",
        State::Open => "Open",
        State::Opened => "Opened",
        State::Void => "Void",
    };
    let mut orders = String::from("[");
    for (i, o) in b.orders.iter().enumerate() {
        if i > 0 {
            orders.push(',');
        }
        orders.push_str(&format!(
            "{{\"from\":{},\"c1\":{},\"ct\":{},\"deposit\":\"{}\"}}",
            jstr(o.from.as_str()),
            bytes_json(&o.c1),
            bytes_json(&o.ct),
            o.deposit.as_yoctonear()
        ));
    }
    orders.push(']');
    let mut opened = String::from("[");
    for (i, o) in b.opened.iter().enumerate() {
        if i > 0 {
            opened.push(',');
        }
        opened.push_str(&format!(
            "{{\"from\":{},\"status\":{},\"side\":{},\"price_bp\":{},\"amount\":\"{}\"}}",
            jstr(o.from.as_str()),
            o.status,
            o.side,
            o.price_bp,
            o.amount.as_yoctonear()
        ));
    }
    opened.push(']');
    format!(
        "{{\"close_height\":{},\"state\":\"{}\",\"lock\":{},\"key_attempts\":{},\"orders\":{},\"opened\":{}}}",
        b.close_height,
        state,
        bytes_json(&b.lock),
        b.key_attempts,
        orders,
        opened
    )
}

fn bkey(id: u64) -> Vec<u8> {
    let mut k = b"b".to_vec();
    k.extend_from_slice(&id.to_le_bytes());
    k
}

fn load(id: u64) -> Option<Batch> {
    env::storage_read(&bkey(id)).map(|v| borsh::from_slice(&v).expect("bad batch"))
}

fn save(id: u64, b: &Batch) {
    env::storage_write(&bkey(id), &borsh::to_vec(b).unwrap());
}

fn die(m: &str) -> ! {
    env::panic_str(m)
}

fn must<T>(o: Option<T>, m: &str) -> T {
    match o {
        Some(v) => v,
        None => die(m),
    }
}

fn nodeposit(name: &str) {
    if env::attached_deposit().as_yoctonear() != 0 {
        die(&format!("Method {} doesn't accept deposit", name));
    }
}

fn private(name: &str) {
    if env::current_account_id() != env::predecessor_account_id() {
        die(&format!("Method {} is private", name));
    }
}

fn state() -> Sealbox {
    must(env::state_read::<Sealbox>(), "The contract has not been initialized")
}

/// Decode a JSON string body (after the opening quote). Returns the text and the index after the closing quote.
fn json_unescape(b: &[u8], mut i: usize) -> Option<(String, usize)> {
    let mut out: Vec<u8> = Vec::new();
    loop {
        let c = *b.get(i)?;
        i += 1;
        match c {
            0x22 => return Some((String::from_utf8(out).ok()?, i)),
            0x5c => {
                let e = *b.get(i)?;
                i += 1;
                let ch = match e {
                    0x22 => '"',
                    0x5c => '\\',
                    0x2f => '/',
                    0x62 => '\u{8}',
                    0x66 => '\u{c}',
                    0x6e => '\n',
                    0x72 => '\r',
                    0x74 => '\t',
                    0x75 => {
                        let mut u = 0u32;
                        for _ in 0..4 {
                            u = (u << 4) | nib(*b.get(i)?)? as u32;
                            i += 1;
                        }
                        if (0xD800..0xDC00).contains(&u) {
                            if b.get(i) != Some(&0x5c) || b.get(i + 1) != Some(&0x75) {
                                return None;
                            }
                            i += 2;
                            let mut l = 0u32;
                            for _ in 0..4 {
                                l = (l << 4) | nib(*b.get(i)?)? as u32;
                                i += 1;
                            }
                            if !(0xDC00..0xE000).contains(&l) {
                                return None;
                            }
                            u = 0x10000 + ((u - 0xD800) << 10) + (l - 0xDC00);
                        }
                        char::from_u32(u)?
                    }
                    _ => return None,
                };
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
            c if c < 0x20 => return None,
            c => out.push(c),
        }
    }
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && matches!(b[i], 0x20 | 0x0a | 0x0d | 0x09) {
        i += 1;
    }
    i
}

/// Flat JSON object of method arguments: (key, value text, value was a string).
struct Args(Vec<(String, String, bool)>);

impl Args {
    fn read() -> Args {
        let raw = must(env::input(), "Expected input since method has arguments.");
        let b = &raw[..];
        let bad = "Failed to deserialize input from JSON.";
        let mut i = skip_ws(b, 0);
        if b.get(i) != Some(&0x7b) {
            die(bad);
        }
        i = skip_ws(b, i + 1);
        let mut v = Vec::new();
        if b.get(i) == Some(&0x7d) {
            return Args(v);
        }
        loop {
            if b.get(i) != Some(&0x22) {
                die(bad);
            }
            let (k, j) = must(json_unescape(b, i + 1), bad);
            i = skip_ws(b, j);
            if b.get(i) != Some(&0x3a) {
                die(bad);
            }
            i = skip_ws(b, i + 1);
            if b.get(i) == Some(&0x22) {
                let (s, j) = must(json_unescape(b, i + 1), bad);
                v.push((k, s, true));
                i = j;
            } else {
                let st = i;
                while i < b.len() && !matches!(b[i], 0x2c | 0x7d | 0x20 | 0x0a | 0x0d | 0x09) {
                    i += 1;
                }
                v.push((k, String::from_utf8_lossy(&b[st..i]).into_owned(), false));
            }
            i = skip_ws(b, i);
            match b.get(i) {
                Some(0x2c) => i = skip_ws(b, i + 1),
                Some(0x7d) => return Args(v),
                _ => die(bad),
            }
        }
    }

    fn str(&self, name: &str) -> String {
        match self.0.iter().find(|(k, _, _)| k == name) {
            Some((_, v, true)) => v.clone(),
            _ => die(&format!("Failed to deserialize input from JSON: missing string field `{}`", name)),
        }
    }

    fn u64(&self, name: &str) -> u64 {
        match self.0.iter().find(|(k, _, _)| k == name) {
            Some((_, v, false)) => must(v.parse::<u64>().ok(), "Failed to deserialize input from JSON: bad number"),
            _ => die(&format!("Failed to deserialize input from JSON: missing number field `{}`", name)),
        }
    }
}

/// Result of the promise this callback waits on: Some(text) only when it succeeded with a JSON string.
fn callback_text() -> Option<String> {
    let data = env::promise_result_checked(0, usize::MAX).ok()?;
    let i = skip_ws(&data, 0);
    if data[i..].starts_with(b"null") {
        return None;
    }
    if data.get(i) != Some(&0x22) {
        die("Failed to deserialize callback using JSON");
    }
    Some(must(json_unescape(&data, i + 1), "Failed to deserialize callback using JSON").0)
}

fn field(result: Option<String>, name: &str) -> Option<Vec<u8>> {
    result.and_then(|s| json_str_field(&s, name)).and_then(|h| hex_decode(&h))
}

fn refund(o: &Sealed) {
    let p = env::promise_batch_create(&o.from);
    env::promise_batch_action_transfer(p, o.deposit);
}

impl Sealbox {
    fn request_run(&self, input: String, deposit: NearToken) -> near_sdk::PromiseIndex {
        let args = format!(
            "{{\"input_data\":{},\"payer_account_id\":{},\"resource_limits\":{{\"max_execution_seconds\":60,\"max_instructions\":1000000000,\"max_memory_mb\":128}},\"response_format\":\"Text\",\"secrets_ref\":{{\"account_id\":{},\"profile\":{}}},\"source\":{{\"WasmUrl\":{{\"build_target\":\"wasm32-wasip2\",\"hash\":{},\"url\":{}}}}}}}",
            jstr(&input),
            jstr(env::predecessor_account_id().as_str()),
            jstr(self.secrets_account.as_str()),
            jstr(&self.secrets_profile),
            jstr(&self.program_hash),
            jstr(&self.program_url),
        );
        let gas = env::prepaid_gas().saturating_sub(CALLBACK_GAS).saturating_sub(OVERHEAD_GAS);
        let p = env::promise_batch_create(&OUTLAYER.parse::<AccountId>().unwrap());
        env::promise_batch_action_function_call_weight(
            p,
            "request_execution",
            args.as_bytes(),
            deposit,
            gas,
            GasWeight(1),
        );
        p
    }

    /// Run the chip request, then call `method` on this contract with the batch id, and return that promise.
    fn run_then(&self, input: String, deposit: NearToken, method: &str, batch_id: u64) {
        let p = self.request_run(input, deposit);
        let me = env::current_account_id();
        let q = env::promise_batch_then(p, &me);
        env::promise_batch_action_function_call(
            q,
            method,
            format!("{{\"batch_id\":{}}}", batch_id).as_bytes(),
            NearToken::from_yoctonear(0),
            CALLBACK_GAS,
        );
        env::promise_return(q);
    }

    /// Start a batch: ask the chip for the public lock.
    fn open_batch(&mut self, batch_id: u64, close_height: u64) {
        if load(batch_id).is_some() {
            die("batch id already used");
        }
        if close_height <= env::block_height() + 5 {
            die("close height too soon");
        }
        save(
            batch_id,
            &Batch {
                close_height,
                state: State::WaitingForLock,
                lock: vec![],
                key_attempts: 0,
                orders: vec![],
                opened: vec![],
            },
        );
        let input = format!("{{\"batch_id\":{},\"mode\":\"lock\"}}", batch_id);
        self.run_then(input, env::attached_deposit(), "on_lock", batch_id);
    }

    fn on_lock(&mut self, batch_id: u64, result: Option<String>) {
        let mut b = must(load(batch_id), "unknown batch");
        match field(result, "lock").filter(|l| l.len() == 64) {
            Some(l) => {
                b.lock = l.clone();
                b.state = State::Open;
                event(
                    "batch_open",
                    format!(
                        "{{\"batch\":{},\"close_height\":{},\"lock\":\"{}\"}}",
                        batch_id,
                        b.close_height,
                        hex_encode(&l)
                    ),
                );
            }
            None => {
                b.state = State::Void;
                event("batch_void", format!("{{\"batch\":{},\"why\":\"no lock\"}}", batch_id));
            }
        }
        save(batch_id, &b);
    }

    /// Seal one order. `c1` and `ct` are hex. The deposit must cover the amount inside the order.
    fn seal(&mut self, batch_id: u64, c1: String, ct: String) {
        let mut b = must(load(batch_id), "unknown batch");
        if b.state != State::Open {
            die("batch not open");
        }
        if env::block_height() >= b.close_height {
            die("batch closed");
        }
        if b.orders.len() >= MAX_ORDERS {
            die("batch full");
        }
        let c1 = must(hex_decode(&c1), "c1 hex");
        let ct = must(hex_decode(&ct), "ct hex");
        if !(c1.len() == 64 && ct.len() == 32) {
            die("bad sizes");
        }
        b.orders.push(Sealed { from: env::predecessor_account_id(), c1, ct, deposit: env::attached_deposit() });
        let n = b.orders.len();
        save(batch_id, &b);
        event("sealed", format!("{{\"batch\":{},\"n\":{}}}", batch_id, n));
    }

    /// Seal a writer's page. `c1` and `ct` are hex, `text_hash` is the keccak256 of the plain text.
    /// The attached deposit pays for the storage and is kept.
    fn seal_words(&mut self, batch_id: u64, c1: String, ct: String, text_hash: String) {
        let b = must(load(batch_id), "unknown batch");
        if b.state != State::Open {
            die("batch not open");
        }
        if env::block_height() >= b.close_height {
            die("batch closed");
        }
        let c1 = must(hex_decode(&c1), "c1 hex");
        let ct = must(hex_decode(&ct), "ct hex");
        let text_hash = must(hex_decode(&text_hash), "text_hash hex");
        if !(c1.len() == 64 && text_hash.len() == 32 && !ct.is_empty() && ct.len() <= MAX_WORDS_BYTES) {
            die("bad sizes");
        }
        if env::attached_deposit().as_yoctonear() < (ct.len() as u128 + 200) * BYTE_COST {
            die("deposit too small for storage");
        }
        let mut w = load_words(batch_id);
        if w.len() >= MAX_WORDS {
            die("too many pages");
        }
        w.push(Words { from: env::predecessor_account_id(), c1, ct, text_hash });
        let n = w.len();
        env::storage_write(&wkey(batch_id), &borsh::to_vec(&w).unwrap());
        event("words_sealed", format!("{{\"batch\":{},\"n\":{}}}", batch_id, n));
    }

    /// After the close block: ask the chip for the key. Anyone may call.
    fn release(&mut self, batch_id: u64) {
        let b = must(load(batch_id), "unknown batch");
        if b.state != State::Open {
            die("batch not open");
        }
        if env::block_height() < b.close_height {
            die("batch not closed yet");
        }
        let input = format!(
            "{{\"batch_id\":{},\"contract\":\"{}\",\"mode\":\"key\"}}",
            batch_id,
            env::current_account_id()
        );
        self.run_then(input, env::attached_deposit(), "on_key", batch_id);
    }

    fn on_key(&mut self, batch_id: u64, result: Option<String>) {
        let mut b = must(load(batch_id), "unknown batch");
        let start = env::used_gas();
        let key = field(result, "key").filter(|k| k.len() == 32 && key_matches_lock(k, &b.lock));
        let Some(key) = key else {
            b.key_attempts += 1;
            if b.key_attempts >= MAX_KEY_ATTEMPTS {
                b.state = State::Void;
                event(
                    "batch_void",
                    format!("{{\"batch\":{},\"why\":\"key withheld or wrong 3 times\"}}", batch_id),
                );
                for o in &b.orders {
                    refund(o);
                }
            } else {
                event("key_failed", format!("{{\"attempt\":{},\"batch\":{}}}", b.key_attempts, batch_id));
            }
            save(batch_id, &b);
            return;
        };
        let after_check = env::used_gas();
        let mut opened = vec![];
        for o in &b.orders {
            let plain = open_order(&key, &o.c1, &o.ct);
            match parse_order(&plain) {
                Some((side, price, amt)) if amt <= o.deposit.as_yoctonear() => {
                    opened.push(Opened {
                        from: o.from.clone(),
                        status: 0,
                        side,
                        price_bp: price,
                        amount: NearToken::from_yoctonear(amt),
                    });
                }
                _ => {
                    // bad format or amount above deposit: refund the whole deposit
                    refund(o);
                    opened.push(Opened {
                        from: o.from.clone(),
                        status: 1,
                        side: 0,
                        price_bp: 0,
                        amount: NearToken::from_yoctonear(0),
                    });
                }
            }
        }
        let end = env::used_gas();
        b.state = State::Opened;
        b.opened = opened;
        event(
            "batch_opened",
            format!(
                "{{\"batch\":{},\"gas_key_check\":{},\"gas_open_all\":{},\"key\":\"{}\",\"orders\":{}}}",
                batch_id,
                after_check.as_gas() - start.as_gas(),
                end.as_gas() - after_check.as_gas(),
                hex_encode(&key),
                b.orders.len()
            ),
        );
        save(batch_id, &b);
    }
}

/// Contract entry points, written by hand (no serde) to keep the wasm small. Method names and
/// JSON shapes are the same as before.
#[cfg(target_arch = "wasm32")]
mod exports {
    use super::*;

    #[no_mangle]
    pub extern "C" fn new() {
        nodeposit("new");
        if env::state_exists() {
            die("The contract has already been initialized");
        }
        let a = Args::read();
        let secrets_account = must(a.str("secrets_account").parse::<AccountId>().ok(), "bad secrets_account");
        let s = Sealbox {
            program_url: a.str("program_url"),
            program_hash: a.str("program_hash"),
            secrets_account,
            secrets_profile: a.str("secrets_profile"),
        };
        env::state_write(&s);
    }

    #[no_mangle]
    pub extern "C" fn open_batch() {
        let a = Args::read();
        let (id, ch) = (a.u64("batch_id"), a.u64("close_height"));
        state().open_batch(id, ch);
    }

    #[no_mangle]
    pub extern "C" fn on_lock() {
        nodeposit("on_lock");
        private("on_lock");
        let a = Args::read();
        let id = a.u64("batch_id");
        state().on_lock(id, callback_text());
    }

    #[no_mangle]
    pub extern "C" fn seal() {
        let a = Args::read();
        let (id, c1, ct) = (a.u64("batch_id"), a.str("c1"), a.str("ct"));
        state().seal(id, c1, ct);
    }

    #[no_mangle]
    pub extern "C" fn seal_words() {
        let a = Args::read();
        let (id, c1, ct, h) = (a.u64("batch_id"), a.str("c1"), a.str("ct"), a.str("text_hash"));
        state().seal_words(id, c1, ct, h);
    }

    #[no_mangle]
    pub extern "C" fn get_words() {
        nodeposit("get_words");
        let a = Args::read();
        let id = a.u64("batch_id");
        let _ = state();
        env::value_return(words_json(&load_words(id)).as_bytes());
    }

    #[no_mangle]
    pub extern "C" fn release() {
        let a = Args::read();
        let id = a.u64("batch_id");
        state().release(id);
    }

    #[no_mangle]
    pub extern "C" fn on_key() {
        nodeposit("on_key");
        private("on_key");
        let a = Args::read();
        let id = a.u64("batch_id");
        state().on_key(id, callback_text());
    }

    #[no_mangle]
    pub extern "C" fn get_batch() {
        nodeposit("get_batch");
        let a = Args::read();
        let id = a.u64("batch_id");
        let _ = state();
        let out = match load(id) {
            None => "null".to_string(),
            Some(b) => batch_json(&b),
        };
        env::value_return(out.as_bytes());
    }

    /// Used by the chip to double-check that a batch has really closed. Public data only.
    #[no_mangle]
    pub extern "C" fn close_height() {
        nodeposit("close_height");
        let a = Args::read();
        let id = a.u64("batch_id");
        let _ = state();
        let out = match load(id) {
            None => "null".to_string(),
            Some(b) => b.close_height.to_string(),
        };
        env::value_return(out.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;
    use sealcore::*;

    fn acct(n: &str) -> AccountId {
        n.parse().unwrap()
    }

    fn order(side: u8, price: u16, amount: u128, fill: u8) -> [u8; 32] {
        let mut o = [fill; 32];
        o[0] = 1;
        o[1] = side;
        o[2..4].copy_from_slice(&price.to_be_bytes());
        o[4..20].copy_from_slice(&amount.to_be_bytes());
        o
    }

    #[test]
    fn host_curve_maths_matches_reference_and_gas_is_low() {
        let ctx = VMContextBuilder::new().current_account_id(acct("box.testnet")).block_height(100).build();
        testing_env!(ctx);
        let k = batch_scalar(b"unit-master", 1);
        let lock_le = lock_le(&k);
        let key_le = scalar_to_le(&k);
        assert!(key_matches_lock(&key_le, &lock_le));
        let wrong = scalar_to_le(&batch_scalar(b"unit-master", 2));
        assert!(!key_matches_lock(&wrong, &lock_le));
        let r = batch_scalar(b"trader", 9);
        let o = order(1, 6500, 3_000_000_000_000_000_000_000_000u128, 0x5a);
        let (c1, ct) = seal(&lock_be(&k), &r, &o);
        let before = env::used_gas();
        let got = open_order(&key_le, &c1, &ct);
        let per_order = env::used_gas().as_gas() - before.as_gas();
        assert_eq!(got, o);
        println!("GAS per opened order: {} gas = {:.3} Tgas", per_order, per_order as f64 / 1e12);
        assert!(per_order < 2_000_000_000_000, "over 2 Tgas");
        let b2 = env::used_gas();
        assert!(key_matches_lock(&key_le, &lock_le));
        let chk = env::used_gas().as_gas() - b2.as_gas();
        println!("GAS key check: {} gas = {:.3} Tgas", chk, chk as f64 / 1e12);
        let junk = open_order(&wrong, &c1, &ct);
        assert_ne!(junk, o);
    }

    #[test]
    fn batch_of_100_fits_one_call() {
        let ctx = VMContextBuilder::new().current_account_id(acct("box.testnet")).block_height(100).build();
        testing_env!(ctx);
        let k = batch_scalar(b"unit-master", 5);
        let (lock_be_b, key_le) = (lock_be(&k), scalar_to_le(&k));
        let mut sealed = vec![];
        for i in 0..100u64 {
            let r = batch_scalar(b"trader", 1000 + i);
            let o = order((i % 2) as u8, 5000 + i as u16, 1_000_000u128 * (i as u128 + 1), i as u8);
            sealed.push((o, seal(&lock_be_b, &r, &o)));
        }
        let before = env::used_gas();
        for (o, (c1, ct)) in &sealed {
            assert_eq!(open_order(&key_le, c1, ct), *o);
        }
        let total = env::used_gas().as_gas() - before.as_gas();
        println!("GAS open 100 orders: {:.2} Tgas ({:.3} Tgas each)", total as f64 / 1e12, total as f64 / 1e14);
        assert!(total < 250_000_000_000_000, "over 250 Tgas for 100 orders");
    }

    #[test]
    fn order_format_checks() {
        assert!(parse_order(&order(0, 5000, 10, 0)).is_some());
        assert!(parse_order(&order(2, 5000, 10, 0)).is_none());
        assert!(parse_order(&order(0, 0, 10, 0)).is_none());
        assert!(parse_order(&order(0, 10_000, 10, 0)).is_none());
    }
}
