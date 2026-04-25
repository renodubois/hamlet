//! Single source of truth for runtime configuration.
//!
//! `Config::from_env` is called once in `main`. Every other module receives
//! the values it needs through `web::Data` or function arguments — there
//! should be no `std::env::var` calls outside this file.

use std::path::PathBuf;

use crate::voice::VoiceConfig;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3030";
const DEFAULT_DATABASE_URL: &str = "sqlite:file::memory:?cache=shared";
const DEFAULT_LOG_FILTER: &str = "info";
const DEFAULT_UPLOADS_DIR: &str = "./uploads";

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: String,
    pub database_url: String,
    pub log_filter: String,
    pub uploads_dir: PathBuf,
    /// `None` when LiveKit env vars are missing — voice endpoints respond 503.
    pub voice: Option<VoiceConfig>,
    /// Whether outbound embed fetches happen on message create/update.
    /// `true` in `cargo run`; tests pass `false` so the suite is hermetic.
    pub embed_fetcher_enabled: bool,
    /// Whether to seed the in-memory dev fixtures + dev session token on
    /// startup. Wants to flip off once the DB is persistent.
    pub seed_dev_data: bool,
}

impl Config {
    /// Production-shaped config: read every value from the environment with
    /// sane defaults. `cargo run` and Docker Compose both go through here.
    pub fn from_env() -> Self {
        Self {
            bind_addr: env_or(DEFAULT_BIND_ADDR, "HAMLET_BIND_ADDR"),
            database_url: env_or(DEFAULT_DATABASE_URL, "DATABASE_URL"),
            log_filter: env_or(DEFAULT_LOG_FILTER, "RUST_LOG"),
            uploads_dir: PathBuf::from(env_or(DEFAULT_UPLOADS_DIR, "HAMLET_UPLOADS_DIR")),
            voice: VoiceConfig::from_env(),
            embed_fetcher_enabled: true,
            seed_dev_data: true,
        }
    }
}

fn env_or(default: &str, key: &str) -> String {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => v,
        _ => default.to_owned(),
    }
}
