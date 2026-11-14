#[cfg(target_os = "wasi")]
fn main() {
    use court::run::run;
    use std::io::{Read, Write};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).unwrap();
    let start = Instant::now();
    let elapsed = move || start.elapsed().as_secs();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let fetcher = court::net::WasiFetcher;

    #[cfg(feature = "standin")]
    let mut judge = {
        // Test build only: scripted answers may come from the input's "sa" list.
        let sa: Vec<String> = serde_json::from_slice::<serde_json::Value>(&input)
            .ok()
            .and_then(|v| v.get("sa").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        // Test build only: a claim containing "(lie)" is answered FALSE by all three stand-in judges.
        let sa = if sa.is_empty() {
            let lie = String::from_utf8_lossy(&input).contains("(lie)");
            vec![(if lie { "FALSE" } else { "TRUE" }).to_string(); 3]
        } else {
            sa
        };
        court::judges::standin::StandIn::new(sa)
    };
    #[cfg(not(feature = "standin"))]
    let mut judge = court::net::NearAiJudge { key: std::env::var("NEAR_AI_KEY").unwrap_or_default() };

    let out = run(&input, &fetcher, &mut judge, &elapsed, now);
    print!("{}", out);
    std::io::stdout().flush().unwrap();
}

#[cfg(not(target_os = "wasi"))]
fn main() {
    eprintln!("court runs inside the sealed box (wasm32-wasip2)");
}
