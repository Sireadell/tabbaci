//! Pure rules for a round. No blockchain calls in here, so every rule is tested on a normal computer.
pub mod ladder;
use serde_json::Value;

pub const MAX_ATTEMPTS: usize = 3;
pub const OUT_MAX: usize = 2000;

#[derive(Debug, PartialEq, Clone)]
pub enum Judged {
    /// A valid ruling. Letter is T, F, U or D.
    Final(String),
    /// The sources disagreed, so the round is VOID right away.
    SourcesDisagree,
    /// The attempt did not produce a usable ruling. The reason is published with the attempt.
    Failed(String),
}

/// Check one callback against the stored round. `out` is None when OutLayer returned nothing.
pub fn judge_output(round_id: u64, input_hash: &str, out: Option<&str>) -> Judged {
    let Some(text) = out else { return Judged::Failed("NULL".into()) };
    if text.len() > OUT_MAX {
        return Judged::Failed("BAD_OUTPUT_SIZE".into());
    }
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Judged::Failed("BAD_OUTPUT".into()) };
    if v["v"].as_u64() != Some(1) || v["rd"].as_u64() != Some(round_id) || v["ih"].as_str() != Some(input_hash) {
        return Judged::Failed("BAD_OUTPUT".into());
    }
    match v["o"].as_str() {
        Some(o @ ("T" | "F" | "U" | "D")) => Judged::Final(o.to_string()),
        Some("V") => Judged::SourcesDisagree,
        Some("X") => Judged::Failed(format!("BOX_ERROR_{}", v["rc"].as_u64().unwrap_or(99))),
        _ => Judged::Failed("BAD_OUTPUT".into()),
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Status {
    Open,
    Settled,
    Void,
}

#[derive(Debug, Clone)]
pub struct State {
    pub status: Status,
    pub failures: usize,
    pub attempts: usize,
    pub pending: bool,
    pub outcome: Option<String>,
    pub void_reason: Option<String>,
}

impl State {
    pub fn new() -> Self {
        State { status: Status::Open, failures: 0, attempts: 0, pending: false, outcome: None, void_reason: None }
    }

    /// R2: one live attempt at a time. R6: no attempts after settled or void.
    pub fn can_start(&self) -> Result<(), &'static str> {
        if self.status != Status::Open {
            return Err("round is closed");
        }
        if self.pending {
            return Err("an attempt is already running");
        }
        if self.attempts >= MAX_ATTEMPTS {
            return Err("no attempts left");
        }
        Ok(())
    }

    pub fn start(&mut self) {
        self.pending = true;
        self.attempts += 1;
    }

    /// Apply a callback. R5: the first valid ruling closes the round; later callbacks change nothing.
    pub fn finish(&mut self, j: Judged) {
        self.pending = false;
        if self.status != Status::Open {
            return;
        }
        match j {
            Judged::Final(o) => {
                self.status = Status::Settled;
                self.outcome = Some(o);
            }
            Judged::SourcesDisagree => {
                self.status = Status::Void;
                self.void_reason = Some("SRC_DISAGREE".into());
            }
            Judged::Failed(_) => {
                self.failures += 1;
                if self.failures >= MAX_ATTEMPTS {
                    self.status = Status::Void;
                    self.void_reason = Some("3_FAILED_RUNS".into());
                }
            }
        }
    }
}

pub fn is_hex_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// True exactly when Rust's `str::parse::<f64>` accepts the text: an optional sign, then inf, infinity or nan
/// (any case), or digits with at most one dot and at least one digit, then an optional exponent with digits.
pub fn is_number(s: &str) -> bool {
    let b = s.as_bytes();
    let b = match b.first() {
        Some(b'+' | b'-') => &b[1..],
        _ => b,
    };
    for word in ["inf", "infinity", "nan"] {
        if b.eq_ignore_ascii_case(word.as_bytes()) {
            return true;
        }
    }
    let digits = |i: &mut usize| {
        let start = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - start
    };
    let mut i = 0;
    let mut n = digits(&mut i);
    if i < b.len() && b[i] == b'.' {
        i += 1;
        n += digits(&mut i);
    }
    if n == 0 {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

/// Lowercase hex of some bytes.
pub fn hex(d: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut o = String::with_capacity(d.len() * 2);
    for x in d {
        o.push(H[(x >> 4) as usize] as char);
        o.push(H[(x & 15) as usize] as char);
    }
    o
}

/// Base64 with the URL-safe alphabet and no padding.
pub fn base64url(d: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut o = String::with_capacity((d.len() * 4).div_ceil(3));
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..=c.len() {
            o.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_number_matches_rust_float_parsing() {
        let parts = ["", "+", "-", "0", "7", "12", ".", "e", "E", "e+", "e-", "inf", "INF", "Infinity", "nan", "NaN", "x", " ", "_", "1_0", "0x1"];
        let mut n = 0;
        for a in parts {
            for b in parts {
                for c in parts {
                    for d in ["", "3", ".", "e5", "E-2"] {
                        let t = format!("{a}{b}{c}{d}");
                        assert_eq!(is_number(&t), t.parse::<f64>().is_ok(), "{t:?}");
                        n += 1;
                    }
                }
            }
        }
        assert!(n > 40_000);
        for t in ["1e400", "-0", "+.5", "5.", "1.5e10", "00012.000", "infinit", "infinityy", "+-1", "1e5.0", "\u{661}", "1\u{661}"] {
            assert_eq!(is_number(t), t.parse::<f64>().is_ok(), "{t:?}");
        }
    }

    #[test]
    fn hex_and_base64url_encode_like_the_usual_crates() {
        assert_eq!(hex(&[0, 1, 0xab, 0xff]), "0001abff");
        // RFC 4648 test vectors, URL-safe alphabet, no padding
        for (i, o) in [("", ""), ("f", "Zg"), ("fo", "Zm8"), ("foo", "Zm9v"), ("foob", "Zm9vYg"), ("fooba", "Zm9vYmE"), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64url(i.as_bytes()), o);
        }
        assert_eq!(base64url(&[0xfb, 0xff, 0xbf]), "-_-_");
    }

    fn good(rd: u64, ih: &str, o: &str) -> String {
        format!(r#"{{"v":1,"rd":{},"ih":"{}","o":"{}","rc":0,"t":1,"s":[],"j":[]}}"#, rd, ih, o)
    }

    #[test]
    fn valid_rulings_are_final() {
        for o in ["T", "F", "U", "D"] {
            assert_eq!(judge_output(7, "abc", Some(&good(7, "abc", o))), Judged::Final(o.into()));
        }
    }

    #[test]
    fn wrong_round_or_input_hash_is_a_failed_attempt() {
        assert!(matches!(judge_output(7, "abc", Some(&good(8, "abc", "T"))), Judged::Failed(_)));
        assert!(matches!(judge_output(7, "abc", Some(&good(7, "xyz", "T"))), Judged::Failed(_)));
    }

    #[test]
    fn null_garbage_and_oversize_fail() {
        assert!(matches!(judge_output(1, "a", None), Judged::Failed(_)));
        assert!(matches!(judge_output(1, "a", Some("not json")), Judged::Failed(_)));
        let big = format!("{}{}", good(1, "a", "T"), " ".repeat(OUT_MAX));
        assert!(matches!(judge_output(1, "a", Some(&big)), Judged::Failed(_)));
        assert!(matches!(judge_output(1, "a", Some(&good(1, "a", "Q"))), Judged::Failed(_)));
    }

    #[test]
    fn source_disagreement_voids_immediately() {
        let mut s = State::new();
        s.start();
        s.finish(judge_output(1, "a", Some(&good(1, "a", "V"))));
        assert_eq!(s.status, Status::Void);
        assert_eq!(s.void_reason.as_deref(), Some("SRC_DISAGREE"));
    }

    #[test]
    fn void_after_three_failures_and_not_before() {
        let mut s = State::new();
        for n in 1..=3 {
            assert!(s.can_start().is_ok());
            s.start();
            assert_eq!(s.attempts, n);
            s.finish(Judged::Failed("NULL".into()));
            if n < 3 {
                assert_eq!(s.status, Status::Open);
            }
        }
        assert_eq!(s.status, Status::Void);
        assert_eq!(s.void_reason.as_deref(), Some("3_FAILED_RUNS"));
        assert!(s.can_start().is_err());
    }

    #[test]
    fn no_parallel_attempts() {
        let mut s = State::new();
        s.start();
        assert!(s.can_start().is_err());
        s.finish(Judged::Failed("NULL".into()));
        assert!(s.can_start().is_ok());
    }

    #[test]
    fn first_success_closes_and_later_callbacks_are_ignored() {
        let mut s = State::new();
        s.start();
        s.finish(Judged::Final("T".into()));
        assert_eq!(s.status, Status::Settled);
        s.pending = true;
        s.finish(Judged::Final("F".into()));
        assert_eq!(s.outcome.as_deref(), Some("T"));
        assert!(s.can_start().is_err());
    }

    #[test]
    fn a_success_after_two_failures_still_settles() {
        let mut s = State::new();
        for _ in 0..2 {
            s.start();
            s.finish(Judged::Failed("NULL".into()));
        }
        s.start();
        s.finish(Judged::Final("F".into()));
        assert_eq!(s.status, Status::Settled);
    }

    #[test]
    fn hash_shape() {
        assert!(is_hex_sha256(&"a".repeat(64)));
        assert!(!is_hex_sha256("zz"));
    }
}
