//! Judge implementations. The real one (NEAR AI Cloud) is in net.rs.
//! The stand-in returns the exact signed shape the real judges return, signed with a fixed test key.

#[cfg(any(feature = "standin", test))]
pub mod standin {
    use crate::run::{Judge, JudgeReply};
    use crate::sig::standin_ed as key;
    use crate::util::sha256_hex;

    pub struct StandIn {
        /// what each judge says, for example "TRUE" or "FALSE\nsome copied sentence"
        pub answers: Vec<String>,
        pub calls: usize,
    }

    impl StandIn {
        pub fn new(answers: Vec<String>) -> Self {
            StandIn { answers, calls: 0 }
        }
    }

    impl Judge for StandIn {
        fn ask(&mut self, idx: usize, model: &str, req: &[u8], _timeout_s: u64) -> Result<JudgeReply, String> {
            self.calls += 1;
            let word = self.answers.get(idx).cloned().unwrap_or_else(|| "TRUE".to_string());
            let id = format!("chatcmpl-standin-{}", idx);
            let resp = serde_json::to_vec(&serde_json::json!({
                "id": id,
                "object": "chat.completion",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": word}, "finish_reason": "stop"}],
                "model": model
            }))
            .unwrap();
            let text = format!("{}:{}:{}", model, sha256_hex(req), sha256_hex(&resp));
            let sig = key::sign(&text);
            Ok(JudgeReply { resp, content: word, text, sig, key: key::public(), kind: "provider_tee".to_string() })
        }
    }
}
