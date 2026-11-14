// Proof of concept: can the sealed program read a secret that is not the AI key and use it
// to fetch an https page? Reads PRIVATE_SOURCE_KEY, sends it as a bearer header to the host
// named in the input, and prints only the status, size, page fingerprint, and whether the key
// was visible. Never the key and never the page.
use sha2::{Digest, Sha256};
use std::io::Read;

#[cfg(target_os = "wasi")]
fn fetch(url: &str, key: &str) -> Result<(u16, Vec<u8>), String> {
    let resp = wasi_http_client::Client::new()
        .get(url)
        .connect_timeout(std::time::Duration::from_secs(15))
        .header("User-Agent", "tabbaci-probe/0.1")
        .header("Authorization", format!("Bearer {}", key))
        .send()
        .map_err(|e| format!("send: {e}"))?;
    let status = resp.status();
    let body = resp.body().map_err(|e| format!("body: {e}"))?;
    Ok((status, body))
}
#[cfg(not(target_os = "wasi"))]
fn fetch(_: &str, _: &str) -> Result<(u16, Vec<u8>), String> {
    Err("runs only in the sealed box".into())
}

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok();
    let v: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    let url = v["url"].as_str().unwrap_or("https://httpbin.org/bearer");
    let key = std::env::var("PRIVATE_SOURCE_KEY").unwrap_or_default();
    if key.is_empty() {
        println!("{{\"key_visible\":false}}");
        return;
    }
    let kfp = hex::encode(&Sha256::digest(key.as_bytes())[..4]);
    match fetch(url, &key) {
        Ok((status, body)) => {
            let text = String::from_utf8_lossy(&body);
            let echoed = text.contains(&key);
            let fp = hex::encode(&Sha256::digest(&body)[..8]);
            println!("{{\"key_visible\":true,\"key_len\":{},\"key_fp\":\"{}\",\"status\":{},\"bytes\":{},\"page_fp\":\"{}\",\"page_had_key\":{}}}", key.len(), kfp, status, body.len(), fp, echoed);
        }
        Err(e) => println!("{{\"key_visible\":true,\"key_fp\":\"{}\",\"error\":\"{}\"}}", kfp, e.replace(&key, "***")),
    }
}
