// Reports whether the secret named NEAR_AI_KEY is visible to this exact program build.
// Prints only whether it is there, its length and a short fingerprint. Never the value.
use sha2::{Digest, Sha256};

fn main() {
    match std::env::var("NEAR_AI_KEY") {
        Ok(v) if !v.is_empty() => {
            let fp = hex::encode(&Sha256::digest(v.as_bytes())[..4]);
            println!("{{\"key_visible\":true,\"length\":{},\"fingerprint\":\"{}\"}}", v.len(), fp);
        }
        _ => println!("{{\"key_visible\":false}}"),
    }
}
