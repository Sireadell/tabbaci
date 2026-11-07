use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};
use wasi_http_client::Client;

#[derive(Deserialize)]
struct Input {
    url: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = String::new();
    io::stdin().read_to_string(&mut s)?;
    let input: Input = serde_json::from_str(&s)?;
    let resp = Client::new().get(&input.url).send()?;
    let status = resp.status();
    let body = resp.body()?;
    let hash = Sha256::digest(&body);
    let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
    let out = serde_json::json!({"status": status, "bytes": body.len(), "sha256": hex});
    print!("{}", out);
    io::stdout().flush()?;
    Ok(())
}
