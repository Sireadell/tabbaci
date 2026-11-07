use serde_json::Value;

/// Extractor strings:
///   json:$.a.b[0].c      read a value out of JSON
///   find:PREFIX|SUFFIX   the text between the first PREFIX and the next SUFFIX
pub fn extract(body: &str, ex: &str) -> Option<String> {
    if let Some(path) = ex.strip_prefix("json:") {
        let v: Value = serde_json::from_str(body).ok()?;
        let got = json_path(&v, path)?;
        return Some(match got {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => return None,
        });
    }
    if let Some(rest) = ex.strip_prefix("find:") {
        let (pre, suf) = rest.split_once('|')?;
        let start = body.find(pre)? + pre.len();
        let end = body[start..].find(suf)? + start;
        let t = body[start..end].trim();
        if t.is_empty() || t.len() > 200 {
            return None;
        }
        return Some(t.to_string());
    }
    None
}

fn json_path<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    let p = path.strip_prefix('$')?;
    let mut cur = v;
    let mut rest = p;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('.') {
            let end = r.find(|c| c == '.' || c == '[').unwrap_or(r.len());
            let key = &r[..end];
            if key.is_empty() {
                return None;
            }
            cur = cur.get(key)?;
            rest = &r[end..];
        } else if let Some(r) = rest.strip_prefix('[') {
            let end = r.find(']')?;
            let idx: usize = r[..end].parse().ok()?;
            cur = cur.get(idx)?;
            rest = &r[end + 1..];
        } else {
            return None;
        }
    }
    Some(cur)
}

/// A window of at most `width` characters around the first key term found.
pub fn window(body: &str, terms: &[String], width: usize) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    for t in terms {
        if let Some(pos) = lower.find(&t.to_ascii_lowercase()) {
            let half = width / 2;
            let mut s = pos.saturating_sub(half);
            let mut e = (pos + t.len() + half).min(body.len());
            while s > 0 && !body.is_char_boundary(s) {
                s -= 1;
            }
            while e < body.len() && !body.is_char_boundary(e) {
                e += 1;
            }
            return Some(body[s..e].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_paths() {
        let b = r#"{"a":{"b":[{"c":42},{"c":"x"}]},"n":1.5}"#;
        assert_eq!(extract(b, "json:$.a.b[0].c").unwrap(), "42");
        assert_eq!(extract(b, "json:$.a.b[1].c").unwrap(), "x");
        assert_eq!(extract(b, "json:$.n").unwrap(), "1.5");
        assert!(extract(b, "json:$.zzz").is_none());
        assert!(extract(b, "json:a.b").is_none());
    }
    #[test]
    fn find_text() {
        assert_eq!(extract("<h1>Example Domain</h1>", "find:<h1>|</h1>").unwrap(), "Example Domain");
        assert!(extract("abc", "find:<h1>|</h1>").is_none());
    }
    #[test]
    fn window_works() {
        let w = window("aaaa NEAR Protocol launched in 2020 bbbb", &["launched".into()], 20).unwrap();
        assert!(w.contains("launched"));
    }
}
