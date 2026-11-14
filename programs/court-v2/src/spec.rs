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
}

#[derive(Deserialize, Clone, Debug)]
pub struct Input {
    pub v: u32,
    pub rd: u64,
    /// "data" (a number sentence) or "prose"
    pub kind: String,
    /// sha256 hex of the claim text
    pub th: String,
    pub claim: String,
    /// the number the sentence states (data only)
    pub cv: Option<String>,
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
/// Version tag of the input and the output. Version 1 belongs to the first courtroom, so neither contract accepts the other's program.
pub const VERSION: u32 = 2;
