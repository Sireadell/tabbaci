use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct Src {
    pub i: u32,
    pub u: String,
    #[serde(default)]
    pub x: String,
    #[serde(default)]
    pub tol: f64,
    pub g: String,
    /// private source only: the host the access key may be sent to
    #[serde(default)]
    pub h: String,
    /// private source only: the NAME of the secret that holds the access key (never the key itself)
    #[serde(default)]
    pub sk: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Input {
    pub v: u32,
    pub rd: u64,
    /// "data" (a number sentence), "prose", or "private" (a number sentence about a private page)
    pub kind: String,
    /// sha256 hex of the claim text
    pub th: String,
    pub claim: String,
    /// the number the sentence states (data and private)
    pub cv: Option<String>,
    /// private only: how the page's number must compare with cv: gt, ge, lt, le or eq
    #[serde(default)]
    pub op: String,
    pub src: Vec<Src>,
    pub m: Vec<String>,
    pub seed: u64,
    pub asof: u64,
    /// key terms for the excerpt window (prose only)
    #[serde(default)]
    pub terms: Vec<String>,
}

pub const MAX_BYTES: usize = 2_000_000;
pub const SOURCE_SECS: u64 = 8;
pub const JUDGE_SECS: u64 = 20;
pub const SELF_STOP_SECS: u64 = 75;
pub const OUT_MAX: usize = 6000;

// Reason codes
pub const RC_OK: u8 = 0;
pub const RC_SRC_DISAGREE: u8 = 1;
pub const RC_FETCH: u8 = 2;
pub const RC_TIMEOUT: u8 = 3;
pub const RC_UNSIGNED: u8 = 4;
pub const RC_SIZE: u8 = 5;
pub const RC_BAD_INPUT: u8 = 6;
/// the verdict would have carried a secret in some encoding, so nothing but this code leaves
pub const RC_LEAK: u8 = 7;
/// the secret the private source names is not available to this program build
pub const RC_NO_KEY: u8 = 8;

/// Version tag of the input and the output. 1 is the first courtroom, 2 is court-v2, 3 is this private court,
/// so no contract accepts another version's program.
pub const VERSION: u32 = 3;

/// The only hosts an access key may ever be sent to. Fixed in the program build, so no contract, list or input
/// can point the key anywhere else.
pub const KEY_HOSTS: &[&str] = &["api.github.com"];

/// The header and prefix the key is sent with.
pub const KEY_HEADER: &str = "Authorization";
pub const KEY_PREFIX: &str = "Bearer ";

/// A private source may only name a secret whose name starts with this, so the NEAR AI key can never be sent to a source.
pub const KEY_ENV_PREFIX: &str = "PRIVATE_";
pub const KEY_ENV_DEFAULT: &str = "PRIVATE_SOURCE_KEY";

/// True for a secret name a private source may use: PRIVATE_ then 1 to 40 of A-Z, 0-9, _.
pub fn key_env_ok(name: &str) -> bool {
    match name.strip_prefix(KEY_ENV_PREFIX) {
        Some(rest) => {
            !rest.is_empty() && rest.len() <= 40 && rest.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        }
        None => false,
    }
}

/// The comparison words the judges read, for each op.
pub fn op_words(op: &str) -> Option<&'static str> {
    Some(match op {
        "gt" => "more than",
        "ge" => "at least",
        "lt" => "fewer than",
        "le" => "at most",
        "eq" => "exactly",
        _ => return None,
    })
}

/// Does the page's number meet the sentence's comparison?
pub fn op_holds(op: &str, page: f64, cv: f64, tol: f64) -> bool {
    match op {
        "gt" => page > cv,
        "ge" => page >= cv,
        "lt" => page < cv,
        "le" => page <= cv,
        _ => (page - cv).abs() <= tol,
    }
}
