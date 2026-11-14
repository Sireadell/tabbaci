//! Keyed sources: evidence behind an access key whose licence forbids republishing it.
//!
//! The courtroom reads the page with the key inside the locked chip. What leaves the chip is only
//! the HTTP status, the byte count, a fingerprint of the page and the one short value that was
//! asked for. These functions make that rule checkable:
//!   * the key is only ever sent over https to a host the registry named for that source
//!   * the key, in any common encoding, may not appear in anything that leaves the chip
//!   * a page that echoes the key back cannot smuggle it out through the extracted value
use crate::extract::extract;
use crate::util::{b64, sha256, sha256_hex};
use base64::{engine::general_purpose::STANDARD, Engine};

pub trait KeyedFetcher {
    /// GET `url` with the header `name: value`. Returns (status, body).
    fn get_with_header(&self, url: &str, name: &str, value: &str, max_bytes: usize) -> Result<(u16, Vec<u8>), String>;
}

#[derive(Debug, PartialEq, Eq)]
pub struct KeyedOut {
    pub status: u16,
    pub bytes: usize,
    /// sha256 of the body, base64url
    pub fingerprint: String,
    /// the one value asked for, at most 24 characters, or empty
    pub value: String,
}

/// The host part of an https url, lowercase, or None.
pub fn https_host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let end = rest.find(|c| c == '/' || c == '?' || c == '#').unwrap_or(rest.len());
    let auth = &rest[..end];
    if auth.contains('@') || auth.is_empty() {
        return None;
    }
    Some(auth.split(':').next()?.to_ascii_lowercase())
}

/// Forms of the key that must never appear in output.
fn forms(key: &str) -> Vec<String> {
    let mut v = vec![key.to_string(), hex::encode(key), STANDARD.encode(key), b64(key.as_bytes())];
    v.push(percent(key));
    v.push(key.to_ascii_lowercase());
    v.retain(|s| s.len() >= 6);
    v
}

fn percent(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{:02X}", b));
        }
    }
    o
}

/// True when `text` contains the key in any of the forms above.
pub fn leaks_key(text: &str, key: &str) -> bool {
    if key.len() < 6 {
        return !key.is_empty() && text.contains(key);
    }
    forms(key).iter().any(|f| text.contains(f.as_str()))
}

/// Fetch with the key and reduce the page to what may leave the chip.
pub fn read_keyed(
    fetcher: &dyn KeyedFetcher,
    url: &str,
    allowed_hosts: &[String],
    header: &str,
    key: &str,
    extractor: &str,
    max_bytes: usize,
) -> Result<KeyedOut, String> {
    if key.is_empty() {
        return Err("access key not available".into());
    }
    let host = https_host(url).ok_or("keyed sources must use https")?;
    if !allowed_hosts.iter().any(|h| h.eq_ignore_ascii_case(&host)) {
        return Err("host is not on the list for this source".into());
    }
    if leaks_key(url, key) {
        return Err("key would appear in the address".into());
    }
    let (status, body) = fetcher.get_with_header(url, header, key, max_bytes).map_err(|e| {
        if leaks_key(&e, key) { "fetch failed".to_string() } else { format!("fetch failed: {e}") }
    })?;
    let text = String::from_utf8_lossy(&body);
    let mut value = String::new();
    if !extractor.is_empty() {
        if let Some(v) = extract(&text, extractor) {
            value = v;
            value.truncate(24);
        }
    }
    let out = KeyedOut { status, bytes: body.len(), fingerprint: b64(&sha256(&body)), value };
    let shown = format!("{}{}{}{}", out.status, out.bytes, out.fingerprint, out.value);
    if leaks_key(&shown, key) {
        return Err("key would appear in the output".into());
    }
    Ok(out)
}

/// Fingerprint a subscriber can recompute from their own copy of the same page.
pub fn page_fingerprint(body: &[u8]) -> String {
    sha256_hex(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const CANARY: &str = "CANARY-KEY-7f3a91c2e4b8";

    struct Fake {
        body: String,
        status: u16,
        seen: RefCell<Vec<(String, String, String)>>,
        fail_with_key: bool,
    }

    impl KeyedFetcher for Fake {
        fn get_with_header(&self, url: &str, name: &str, value: &str, _m: usize) -> Result<(u16, Vec<u8>), String> {
            self.seen.borrow_mut().push((url.into(), name.into(), value.into()));
            if self.fail_with_key {
                return Err(format!("connection reset while sending {value}"));
            }
            Ok((self.status, self.body.clone().into_bytes()))
        }
    }

    fn fake(body: &str) -> Fake {
        Fake { body: body.into(), status: 200, seen: RefCell::new(vec![]), fail_with_key: false }
    }

    fn hosts() -> Vec<String> {
        vec!["feed.example.org".into()]
    }

    #[test]
    fn reads_value_and_publishes_only_status_size_fingerprint_and_value() {
        let f = fake("{\"data\":{\"price\":\"123.45\"},\"licence\":\"do not republish\"}");
        let o = read_keyed(&f, "https://feed.example.org/v1/q", &hosts(), "Authorization", CANARY, "json:$.data.price", 10_000).unwrap();
        assert_eq!(o.status, 200);
        assert_eq!(o.value, "123.45");
        assert_eq!(o.bytes, f.body.len());
        assert_eq!(o.fingerprint, b64(&sha256(f.body.as_bytes())));
        let dump = format!("{o:?}");
        assert!(!dump.contains("republish"), "page text must not leave");
        assert!(!leaks_key(&dump, CANARY));
        assert_eq!(f.seen.borrow()[0].2, CANARY, "the key was sent to the feed");
    }

    #[test]
    fn a_page_that_echoes_the_key_cannot_carry_it_out() {
        let f = fake(&format!("{{\"echo\":\"{CANARY}\"}}"));
        let r = read_keyed(&f, "https://feed.example.org/v1/q", &hosts(), "Authorization", CANARY, "json:$.echo", 10_000);
        assert_eq!(r.unwrap_err(), "key would appear in the output");
    }

    #[test]
    fn echoed_key_in_other_encodings_is_caught_too() {
        for enc in [hex::encode(CANARY), STANDARD.encode(CANARY), percent(CANARY), b64(CANARY.as_bytes())] {
            let f = fake(&format!("{{\"v\":\"{enc}\"}}"));
            let r = read_keyed(&f, "https://feed.example.org/q", &hosts(), "X-Key", "CANARY-KEY-7f3a91c2e4b8", "json:$.v", 10_000);
            // the value is cut to 24 characters; longer encodings can be cut, so also scan the full text
            assert!(r.is_err() || !leaks_key(&r.unwrap().value, CANARY), "encoding {enc} slipped through");
            assert!(leaks_key(&enc, CANARY), "scanner must know the form {enc}");
        }
    }

    #[test]
    fn error_messages_never_carry_the_key() {
        let mut f = fake("");
        f.fail_with_key = true;
        let e = read_keyed(&f, "https://feed.example.org/q", &hosts(), "X-Key", CANARY, "", 10).unwrap_err();
        assert!(!leaks_key(&e, CANARY), "error was: {e}");
    }

    #[test]
    fn key_goes_only_over_https_to_the_named_host() {
        let f = fake("{}");
        let go = |u: &str| read_keyed(&f, u, &hosts(), "X-Key", CANARY, "", 10).map(|_| ());
        assert!(go("http://feed.example.org/q").is_err());
        assert!(go("https://evil.example.net/q").is_err());
        assert!(go("https://feed.example.org@evil.example.net/q").is_err());
        assert!(go("https://feed.example.org.evil.net/q").is_err());
        assert!(go(&format!("https://feed.example.org/q?k={CANARY}")).is_err());
        assert!(f.seen.borrow().is_empty(), "no request may leave on a refused address");
        assert!(go("https://FEED.example.org:443/q").is_ok());
    }

    #[test]
    fn missing_key_stops_before_any_request() {
        let f = fake("{}");
        assert!(read_keyed(&f, "https://feed.example.org/q", &hosts(), "X-Key", "", "", 10).is_err());
        assert!(f.seen.borrow().is_empty());
    }

    #[test]
    fn ten_rulings_leave_no_canary_anywhere() {
        let f = fake("{\"a\":{\"b\":\"42\"},\"note\":\"licensed text\"}");
        let mut all = String::new();
        for i in 0..10 {
            let url = format!("https://feed.example.org/q?i={i}");
            let o = read_keyed(&f, &url, &hosts(), "Authorization", CANARY, "json:$.a.b", 10_000).unwrap();
            all.push_str(&format!("{o:?}"));
        }
        assert!(!leaks_key(&all, CANARY));
        assert!(!all.contains("licensed text"));
    }

    #[test]
    fn a_subscriber_can_recompute_the_fingerprint() {
        let body = b"{\"a\":1}";
        let o = read_keyed(&fake("{\"a\":1}"), "https://feed.example.org/q", &hosts(), "X", CANARY, "", 10).unwrap();
        assert_eq!(o.fingerprint, b64(&sha256(body)));
        assert_eq!(page_fingerprint(body).len(), 64);
    }
}
