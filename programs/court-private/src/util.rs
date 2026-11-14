use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(s: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(s).ok()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(sha256(bytes))
}

/// Short canonical text for a number: no trailing zeros, at most 6 decimals.
pub fn canon_number(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{:.6}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Quote matching text. Runs of space, tab, CR, LF and no-break space become one space, the ends are
/// trimmed, and ASCII capitals become lowercase. The contract and the checkers use these exact rules.
pub fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut gap = false;
    for c in s.chars() {
        if c == ' ' || c == '\t' || c == '\r' || c == '\n' || c == '\u{a0}' {
            gap = true;
        } else {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

/// True when `quote` (not empty after normalising) is a piece of `text`, both normalised.
pub fn quote_in(text: &str, quote: &str) -> bool {
    let q = norm(quote);
    !q.is_empty() && norm(text).contains(&q)
}

/// A judge's words: line 1 the verdict word, the rest the quote. A pair of straight double quotes around the quote is removed.
pub fn parse_reply(content: &str) -> (String, String) {
    let (first, rest) = match content.split_once('\n') {
        Some((a, b)) => (a, b),
        None => (content, ""),
    };
    let word = first.trim().to_string();
    let q = rest.trim();
    let q = q.strip_prefix('"').unwrap_or(q);
    let q = q.strip_suffix('"').unwrap_or(q);
    (word, q.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn norm_rules() {
        assert_eq!(norm("  The\tMoon \n\n is  Cheese.\u{a0}"), "the moon is cheese.");
        assert_eq!(norm("É a"), "É a");
        assert!(quote_in("x The   Moon is\nbig. y", "the moon IS big."));
        assert!(!quote_in("the moon", ""));
        assert!(!quote_in("the moon", "the sun"));
    }
    #[test]
    fn reply_parts() {
        assert_eq!(parse_reply("TRUE\nIt is \"red\"."), ("TRUE".into(), "It is \"red\".".into()));
        assert_eq!(parse_reply("FALSE\n\"A b.\""), ("FALSE".into(), "A b.".into()));
        assert_eq!(parse_reply("UNSURE\nNONE"), ("UNSURE".into(), "NONE".into()));
        assert_eq!(parse_reply("TRUE"), ("TRUE".into(), "".into()));
    }
}
