//! Judge implementations. The real one (NEAR AI Cloud, in net.rs) is untested until credit exists.
//! The stand-in returns the exact signed shape the real judges return, signed with a fixed test key.

#[cfg(any(feature = "standin", test))]
pub mod standin {
    use crate::run::{Judge, JudgeReply};
    use crate::sig::standin as key;
    use crate::util::sha256_hex;

    pub struct StandIn {
        pub answers: Vec<String>,
        /// pretend each judge takes this long (reported by the caller's clock, not slept)
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
            let resp = format!(
                "{{\"id\":\"{}\",\"object\":\"chat.completion\",\"model\":\"{}\",\"choices\":[{{\"index\":0,\"message\":{{\"role\":\"assistant\",\"content\":\"{}\"}},\"finish_reason\":\"stop\"}}]}}",
                id, model, word
            )
            .into_bytes();
            let text = format!("{}:{}:{}", model, sha256_hex(req), sha256_hex(&resp));
            let sig = key::sign(&text);
            Ok(JudgeReply { resp, content: word, text, sig, addr: key::address(), kind: "provider_tee".to_string() })
        }
    }
}
