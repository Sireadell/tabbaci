//! Runs inside the OutLayer locked chip. Reads the hidden instructions from the secret
//! WRITER_PROMPT and the model key from NEAR_AI_KEY, writes one page, prints the page and the
//! two fingerprints. Not yet run against the real service (no model credit).
use std::io::{Read, Write};

#[cfg(target_os = "wasi")]
struct Chat {
    key: String,
    model: String,
}

#[cfg(target_os = "wasi")]
impl brain::Llm for Chat {
    fn complete(&mut self, system: &str, user: &str) -> Result<String, String> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
            "stream": false
        })
        .to_string();
        let resp = wasi_http_client::Client::new()
            .post("https://cloud-api.near.ai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.key).as_str())
            .header("Content-Type", "application/json")
            .header("x-no-aliasing", "true")
            .body(body.as_bytes())
            .send()
            .map_err(|e| e.to_string())?;
        if resp.status() != 200 {
            return Err(format!("status {}", resp.status()));
        }
        let v: serde_json::Value = serde_json::from_slice(&resp.body().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        v["choices"][0]["message"]["content"].as_str().map(|s| s.to_string()).ok_or_else(|| "no content".to_string())
    }
}

fn main() {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).unwrap_or_default();
    let input: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
    let task = input["task"].as_str().unwrap_or("").to_string();
    let prompt = std::env::var("WRITER_PROMPT").unwrap_or_default();
    #[cfg(target_os = "wasi")]
    let out = {
        let mut chat = Chat {
            key: std::env::var("NEAR_AI_KEY").unwrap_or_default(),
            model: input["model"].as_str().unwrap_or("").to_string(),
        };
        brain::write_page(&prompt, &task, &mut chat)
    };
    #[cfg(not(target_os = "wasi"))]
    let out: Result<brain::Out, String> = Err(format!("this build only runs inside the locked chip ({} bytes of task)", task.len()));
    match out {
        Ok(o) => print!("{}", serde_json::to_string(&o).unwrap()),
        Err(e) => print!("{}", serde_json::json!({"error": e})),
    }
    let _ = std::io::stdout().flush();
}
