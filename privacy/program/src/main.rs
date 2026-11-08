//! Batch lock and key maker. Runs inside the OutLayer locked chip.
//!
//! The hidden master secret (`PROTECTED_BATCH_SECRET`) is created inside the chip and is
//! only handed to this exact build when called by the sealbox contract. From it and a batch
//! id this program derives the batch number `k`.
//!   mode "lock": prints the public lock k*G (always safe to publish).
//!   mode "key" : prints k, but only after checking on chain that the batch's close block
//!                has passed. The check reads the contract itself, so a caller cannot lie.
use serde::Deserialize;
use std::io::{self, Read, Write};
use wasi_http_client::Client;

const RPC: &str = "https://rpc.testnet.near.org";
/// The only contract this build will release keys for.
const CONTRACT: &str = match option_env!("SEALBOX_CONTRACT") {
    Some(c) => c,
    None => "privacy.sireadell.testnet",
};

#[derive(Deserialize)]
struct Input {
    mode: String,
    #[serde(default)]
    batch_id: u64,
    #[serde(default)]
    url: String,
}

fn fail(msg: &str) -> ! {
    print!("{}", serde_json::json!({"error": msg}));
    let _ = io::stdout().flush();
    std::process::exit(0);
}

fn master() -> Vec<u8> {
    match std::env::var("PROTECTED_BATCH_SECRET") {
        Ok(h) => match hex::decode(&h) {
            Ok(b) if b.len() >= 16 => b,
            _ => fail("secret has a bad shape"),
        },
        Err(_) => fail("secret not available"),
    }
}

/// Ask a NEAR node for the contract's close height of this batch, and the block it answered at.
fn chain_close_height(batch_id: u64) -> Result<(Option<u64>, u64), String> {
    let args = serde_json::json!({"batch_id": batch_id}).to_string();
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "query",
        "params": {
            "request_type": "call_function", "finality": "final", "account_id": CONTRACT,
            "method_name": "close_height", "args_base64": b64(args.as_bytes()),
        }
    });
    let resp = Client::new()
        .post(RPC)
        .header("Content-Type", "application/json")
        .body(body.to_string().as_bytes())
        .send()
        .map_err(|e| format!("rpc: {e}"))?;
    let bytes = resp.body().map_err(|e| format!("rpc body: {e}"))?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| format!("rpc json: {e}"))?;
    let res = v.get("result").ok_or("rpc gave no result")?;
    let height = res.get("block_height").and_then(|x| x.as_u64()).ok_or("rpc gave no block height")?;
    let raw: Vec<u8> = res
        .get("result")
        .and_then(|x| x.as_array())
        .ok_or("rpc gave no data")?
        .iter()
        .map(|b| b.as_u64().unwrap_or(0) as u8)
        .collect();
    let close: Option<u64> = serde_json::from_slice(&raw).map_err(|e| format!("close json: {e}"))?;
    Ok((close, height))
}

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

fn main() {
    let mut s = String::new();
    io::stdin().read_to_string(&mut s).unwrap_or_default();
    let input: Input = match serde_json::from_str(&s) {
        Ok(i) => i,
        Err(_) => fail("bad input"),
    };
    let out = match input.mode.as_str() {
        "lock" => {
            let k = sealcore::batch_scalar(&master(), input.batch_id);
            serde_json::json!({"lock": hex::encode(sealcore::lock_le(&k)), "batch_id": input.batch_id})
        }
        "key" => {
            let (close, now) = match chain_close_height(input.batch_id) {
                Ok(x) => x,
                Err(e) => fail(&e),
            };
            let Some(close) = close else { fail("unknown batch") };
            if now < close {
                fail("batch not closed yet");
            }
            let k = sealcore::batch_scalar(&master(), input.batch_id);
            serde_json::json!({"key": hex::encode(sealcore::scalar_to_le(&k)), "checked_at_block": now, "close_height": close})
        }
        // test helper: shows the run's public context (never any secret value)
        "env" => {
            let mut m = serde_json::Map::new();
            for (k, v) in std::env::vars() {
                if k.starts_with("NEAR_") || k.starts_with("OUTLAYER_") {
                    if !k.contains("PRIVATE") && !k.contains("KEY") {
                        m.insert(k, serde_json::Value::String(v));
                    }
                }
            }
            m.insert("secret_present".into(), std::env::var("PROTECTED_BATCH_SECRET").is_ok().into());
            serde_json::Value::Object(m)
        }
        // test helper: fetch a web address and report only success or the failure reason
        "tls" => match Client::new().get(&input.url).send() {
            Ok(r) => serde_json::json!({"url": input.url, "fetched": true, "status": r.status()}),
            Err(e) => serde_json::json!({"url": input.url, "fetched": false, "reason": format!("{e}")}),
        },
        _ => fail("unknown mode"),
    };
    print!("{}", out);
    let _ = io::stdout().flush();
}
