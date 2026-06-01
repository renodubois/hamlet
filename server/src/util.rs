//! Crate-shared helpers: ID generation and Unix timestamps.

use std::time::{SystemTime, UNIX_EPOCH};

use rand::Rng;

// 15 digits caps values at 10^15-1, which is below JS Number.MAX_SAFE_INTEGER
// (2^53-1 ≈ 9.007×10^15). The browser parses response JSON into f64, so any
// 16-digit id above that threshold loses precision and corrupts every
// follow-up request keyed on it.
const ID_LENGTH: u32 = 15;

pub fn generate_id() -> i64 {
    let min = 10_i64.pow(ID_LENGTH - 1);
    let max = 10_i64.pow(ID_LENGTH);
    rand::rng().random_range(min..max)
}

pub fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn now_unix_micros() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0)
}
