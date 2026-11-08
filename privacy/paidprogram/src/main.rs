//! Reads a data source that needs a paid access key. Runs inside the OutLayer locked chip.
//!
//! The key (`PAID_API_KEY`) is handed to this exact build only. The program uses it to fetch the
//! page and then prints ONLY the status, the size and the SHA-256 of the body. The body and the key
//! never leave the chip. A court can sign "I read data with this fingerprint" without publishing
//! the licensed data itself.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};
use wasi_http_client::Client;

#[derive(Deserialize)]
struct Input {
    url: String,
    /// "bearer" (default) sends Authorization: Bearer <key>
    #[serde(default)]
    send_key: Option<bool>,
}

fn fail(msg: &str) -> ! {
    print!("{}", serde_json::json!({"error": msg}));
    let _ = io::stdout().flush();
    std::process::exit(0);
}

fn main() {
    let mut s = String::new();
    io::stdin().read_to_string(&mut s).unwrap_or_default();
    let input: Input = match serde_json::from_str(&s) {
        Ok(i) => i,
        Err(_) => fail("bad input"),
    };
    let key = std::env::var("PAID_API_KEY").ok();
    let use_key = input.send_key.unwrap_or(true);
    let mut req = Client::new().get(&input.url);
    if use_key {
        match &key {
            Some(k) if !k.is_empty() => req = req.header("Authorization", format!("Bearer {k}").as_str()),
            _ => fail("access key not available"),
        }
    }
    let resp = match req.send() {
        Ok(r) => r,
        Err(e) => fail(&format!("fetch failed: {e}")),
    };
    let status = resp.status();
    let body = resp.body().unwrap_or_default();
    let digest = hex::encode(Sha256::digest(&body));
    print!(
        "{}",
        serde_json::json!({"url": input.url, "status": status, "len": body.len(), "body_sha256": digest, "used_key": use_key})
    );
    let _ = io::stdout().flush();
}
