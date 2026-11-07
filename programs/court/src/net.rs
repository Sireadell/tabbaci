//! Network pieces. Only built for the sealed box (wasm32-wasip2).
use crate::run::{Fetcher, JudgeReply};
use std::time::{Duration, Instant};
use wasi_http_client::Client;

pub struct WasiFetcher;

impl Fetcher for WasiFetcher {
    fn get(&self, url: &str, max_bytes: usize, timeout_s: u64) -> Result<(u16, Vec<u8>), String> {
        let start = Instant::now();
        let resp = Client::new()
            .get(url)
            .connect_timeout(Duration::from_secs(timeout_s))
            .send()
            .map_err(|e| format!("send: {e}"))?;
        let status = resp.status();
        let mut out = Vec::new();
        loop {
            if start.elapsed().as_secs() > timeout_s {
                return Err("timeout".into());
            }
            match resp.chunk(65536).map_err(|e| format!("chunk: {e}"))? {
                Some(c) if !c.is_empty() => {
                    out.extend_from_slice(&c);
                    if out.len() > max_bytes {
                        return Err("too big".into());
                    }
                }
                _ => break,
            }
        }
        Ok((status, out))
    }
}

/// NEAR AI Cloud judge. Written from the published docs; not yet run against the real service.
pub struct NearAiJudge {
    pub key: String,
}

impl crate::run::Judge for NearAiJudge {
    fn ask(&mut self, _idx: usize, _model: &str, req: &[u8], timeout_s: u64) -> Result<JudgeReply, String> {
        let start = Instant::now();
        let resp = Client::new()
            .post("https://cloud-api.near.ai/v1/chat/completions")
            .connect_timeout(Duration::from_secs(timeout_s))
            .header("Authorization", format!("Bearer {}", self.key))
            .header("Content-Type", "application/json")
            .header("Accept-Encoding", "identity")
            .header("x-no-aliasing", "true")
            .body(req)
            .send()
            .map_err(|e| format!("send: {e}"))?;
        if resp.status() != 200 {
            return Err(format!("status {}", resp.status()));
        }
        let bytes = resp.body().map_err(|e| format!("body: {e}"))?;
        let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let id = v["id"].as_str().ok_or("no id")?.to_string();
        let content = v["choices"][0]["message"]["content"].as_str().ok_or("no content")?.to_string();
        // The signature may take a moment to appear. Try a few times inside the time budget.
        let mut last = String::from("no signature");
        for _ in 0..4 {
            if start.elapsed().as_secs() >= timeout_s {
                break;
            }
            let url = format!("https://cloud-api.near.ai/v1/signature/{}?signing_algo=ecdsa", id);
            let r = Client::new()
                .get(&url)
                .connect_timeout(Duration::from_secs(5))
                .header("Authorization", format!("Bearer {}", self.key))
                .header("Accept", "application/json")
                .send();
            match r {
                Ok(r) if r.status() == 200 => {
                    let b = r.body().map_err(|e| e.to_string())?;
                    let s: serde_json::Value = serde_json::from_slice(&b).map_err(|e| e.to_string())?;
                    if s.get("error_code").is_some() {
                        last = "unavailable".into();
                        continue;
                    }
                    let text = s["text"].as_str().ok_or("no text")?.to_string();
                    let sig_hex = s["signature"].as_str().ok_or("no signature")?.trim_start_matches("0x");
                    let addr_hex = s["signing_address"].as_str().ok_or("no address")?.trim_start_matches("0x");
                    let sig = hex::decode(sig_hex).map_err(|e| e.to_string())?;
                    let a = hex::decode(addr_hex).map_err(|e| e.to_string())?;
                    if a.len() != 20 {
                        return Err("bad address".into());
                    }
                    let mut addr = [0u8; 20];
                    addr.copy_from_slice(&a);
                    let kind = s["signature_kind"].as_str().unwrap_or("").to_string();
                    return Ok(JudgeReply { resp: bytes, content, text, sig, addr, kind });
                }
                Ok(r) => last = format!("sig status {}", r.status()),
                Err(e) => last = format!("sig: {e}"),
            }
        }
        Err(last)
    }
}
