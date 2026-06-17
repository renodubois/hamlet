//! Single source of truth for runtime configuration.
//!
//! `Config::from_env` is called once in `main`. Every other module receives
//! the values it needs through `web::Data` or function arguments — there
//! should be no `std::env::var` calls outside this file.

use std::path::{Path, PathBuf};

use crate::voice::VoiceConfig;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3030";
const DATABASE_URL_ENV: &str = "DATABASE_URL";
const DATA_DIR_ENV: &str = "HAMLET_DATA_DIR";
const DEFAULT_DATABASE_FILE_NAME: &str = "hamlet.db";
#[cfg(any(target_os = "windows", target_os = "macos"))]
const DEFAULT_DATA_DIR_NAME: &str = "Hamlet";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const DEFAULT_DATA_DIR_NAME: &str = "hamlet";
const DEFAULT_LOG_FILTER: &str = "info";
const DEFAULT_UPLOADS_DIR: &str = "./uploads";
const DEFAULT_MESSAGE_ATTACHMENTS_DIR: &str = "./private-uploads/message-attachments";
const DEFAULT_BOOTSTRAP_DEFAULT_CHANNELS: bool = true;
const SEED_DEV_DATA_ENV: &str = "HAMLET_SEED_DEV_DATA";

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: String,
    pub database_url: String,
    pub log_filter: String,
    pub uploads_dir: PathBuf,
    pub message_attachments_dir: PathBuf,
    /// `None` when LiveKit env vars are missing — voice endpoints respond 503.
    pub voice: Option<VoiceConfig>,
    /// Whether outbound embed fetches happen on message create/update.
    /// `true` in `cargo run`; tests pass `false` so the suite is hermetic.
    pub embed_fetcher_enabled: bool,
    /// Whether to create the starter `general` text channel and `voice` voice
    /// channel when the channel table is empty after database initialization.
    pub bootstrap_default_channels: bool,
    /// Whether to seed local development users, credentials, the quick-login
    /// session token, and the placeholder avatar on startup. Defaults on for
    /// debug/local builds and off for release builds unless explicitly set.
    pub seed_dev_data: bool,
}

impl Config {
    /// Production-shaped config: read every value from the environment with
    /// sane defaults. `cargo run` and Docker Compose both go through here.
    pub fn from_env() -> Self {
        let data_dir = local_app_data_dir_from_env();
        let database_url = std::env::var(DATABASE_URL_ENV).ok();

        Self {
            bind_addr: env_or(DEFAULT_BIND_ADDR, "HAMLET_BIND_ADDR"),
            database_url: database_url_from_env_value(database_url.as_deref(), &data_dir),
            log_filter: env_or(DEFAULT_LOG_FILTER, "RUST_LOG"),
            uploads_dir: PathBuf::from(env_or(DEFAULT_UPLOADS_DIR, "HAMLET_UPLOADS_DIR")),
            message_attachments_dir: PathBuf::from(env_or(
                DEFAULT_MESSAGE_ATTACHMENTS_DIR,
                "HAMLET_MESSAGE_ATTACHMENTS_DIR",
            )),
            voice: VoiceConfig::from_env(),
            embed_fetcher_enabled: true,
            bootstrap_default_channels: env_bool(
                DEFAULT_BOOTSTRAP_DEFAULT_CHANNELS,
                "HAMLET_BOOTSTRAP_DEFAULT_CHANNELS",
            ),
            seed_dev_data: env_bool(default_seed_dev_data(), SEED_DEV_DATA_ENV),
        }
    }

    /// Build the same file-backed SQLite URL used by default local startup,
    /// but rooted at an explicit data directory for tests and smoke harnesses.
    pub fn default_database_url_for_data_dir(data_dir: impl AsRef<Path>) -> String {
        sqlite_file_database_url(&data_dir.as_ref().join(DEFAULT_DATABASE_FILE_NAME))
    }
}

fn default_seed_dev_data() -> bool {
    cfg!(debug_assertions)
}

fn database_url_from_env_value(value: Option<&str>, data_dir: &Path) -> String {
    non_empty_env_value(value)
        .map(str::to_owned)
        .unwrap_or_else(|| Config::default_database_url_for_data_dir(data_dir))
}

fn local_app_data_dir_from_env() -> PathBuf {
    let hamlet_data_dir = std::env::var(DATA_DIR_ENV).ok();
    let xdg_data_home = std::env::var("XDG_DATA_HOME").ok();
    let local_app_data = std::env::var("LOCALAPPDATA").ok();
    let home = std::env::var("HOME").ok();

    local_app_data_dir_from_values(
        hamlet_data_dir.as_deref(),
        xdg_data_home.as_deref(),
        local_app_data.as_deref(),
        home.as_deref(),
    )
}

fn local_app_data_dir_from_values(
    hamlet_data_dir: Option<&str>,
    xdg_data_home: Option<&str>,
    local_app_data: Option<&str>,
    home: Option<&str>,
) -> PathBuf {
    if let Some(path) = non_empty_env_value(hamlet_data_dir) {
        return PathBuf::from(path);
    }

    platform_local_app_data_dir_from_values(xdg_data_home, local_app_data, home)
}

#[cfg(target_os = "windows")]
fn platform_local_app_data_dir_from_values(
    _xdg_data_home: Option<&str>,
    local_app_data: Option<&str>,
    home: Option<&str>,
) -> PathBuf {
    if let Some(path) = non_empty_env_value(local_app_data) {
        return PathBuf::from(path).join(DEFAULT_DATA_DIR_NAME);
    }

    if let Some(path) = non_empty_env_value(home) {
        return PathBuf::from(path)
            .join("AppData")
            .join("Local")
            .join(DEFAULT_DATA_DIR_NAME);
    }

    fallback_data_dir()
}

#[cfg(target_os = "macos")]
fn platform_local_app_data_dir_from_values(
    _xdg_data_home: Option<&str>,
    _local_app_data: Option<&str>,
    home: Option<&str>,
) -> PathBuf {
    if let Some(path) = non_empty_env_value(home) {
        return PathBuf::from(path)
            .join("Library")
            .join("Application Support")
            .join(DEFAULT_DATA_DIR_NAME);
    }

    fallback_data_dir()
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_local_app_data_dir_from_values(
    xdg_data_home: Option<&str>,
    _local_app_data: Option<&str>,
    home: Option<&str>,
) -> PathBuf {
    if let Some(path) = non_empty_env_value(xdg_data_home) {
        return PathBuf::from(path).join(DEFAULT_DATA_DIR_NAME);
    }

    if let Some(path) = non_empty_env_value(home) {
        return PathBuf::from(path)
            .join(".local")
            .join("share")
            .join(DEFAULT_DATA_DIR_NAME);
    }

    fallback_data_dir()
}

fn fallback_data_dir() -> PathBuf {
    PathBuf::from(".hamlet-data")
}

fn sqlite_file_database_url(path: &Path) -> String {
    format!("sqlite://{}?mode=rwc", path.display())
}

fn non_empty_env_value(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

fn env_or(default: &str, key: &str) -> String {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => v,
        _ => default.to_owned(),
    }
}

fn env_bool(default: bool, key: &str) -> bool {
    env_bool_value(default, std::env::var(key).ok().as_deref())
}

fn env_bool_value(default: bool, value: Option<&str>) -> bool {
    value.and_then(parse_bool_flag).unwrap_or(default)
}

fn parse_bool_flag(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "t" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "f" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_database_url_is_file_backed_under_the_data_dir() {
        let data_dir = PathBuf::from("local-data");
        let expected_path = data_dir.join(DEFAULT_DATABASE_FILE_NAME);
        let database_url = Config::default_database_url_for_data_dir(&data_dir);

        assert!(database_url.starts_with("sqlite:"));
        assert!(database_url.ends_with("?mode=rwc"));
        assert!(database_url.contains(&expected_path.display().to_string()));
        assert!(!database_url.to_ascii_lowercase().contains(":memory:"));
    }

    #[test]
    fn database_url_env_override_preserves_existing_sqlite_forms() {
        for database_url in [
            "sqlite:file::memory:?cache=shared",
            "sqlite::memory:",
            "sqlite://relative-hamlet.db?mode=rwc",
            "sqlite:///tmp/hamlet.db?mode=rwc",
            "sqlite:file:hamlet_clean_room?mode=memory&cache=shared",
        ] {
            assert_eq!(
                database_url_from_env_value(Some(database_url), Path::new("ignored")),
                database_url
            );
        }
    }

    #[test]
    fn empty_database_url_env_uses_file_backed_default() {
        let data_dir = PathBuf::from("fallback-data");

        assert_eq!(
            database_url_from_env_value(Some(""), &data_dir),
            Config::default_database_url_for_data_dir(&data_dir)
        );
        assert_eq!(
            database_url_from_env_value(None, &data_dir),
            Config::default_database_url_for_data_dir(&data_dir)
        );
    }

    #[test]
    fn hamlet_data_dir_override_wins_over_platform_defaults() {
        assert_eq!(
            local_app_data_dir_from_values(
                Some("custom-data"),
                Some("xdg-data"),
                Some("local-app-data"),
                Some("home"),
            ),
            PathBuf::from("custom-data")
        );
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    #[test]
    fn unix_local_app_data_dir_uses_xdg_then_home() {
        assert_eq!(
            local_app_data_dir_from_values(None, Some("/xdg-data"), None, Some("/home/alice")),
            PathBuf::from("/xdg-data").join(DEFAULT_DATA_DIR_NAME)
        );
        assert_eq!(
            local_app_data_dir_from_values(None, Some(""), None, Some("/home/alice")),
            PathBuf::from("/home/alice")
                .join(".local")
                .join("share")
                .join(DEFAULT_DATA_DIR_NAME)
        );
    }

    #[test]
    fn bool_flags_accept_common_true_forms() {
        for value in ["1", "true", "TRUE", "t", "yes", "Y", "on", " on "] {
            assert!(env_bool_value(false, Some(value)), "{value:?}");
        }
    }

    #[test]
    fn bool_flags_accept_common_false_forms() {
        for value in ["0", "false", "FALSE", "f", "no", "N", "off", " off "] {
            assert!(!env_bool_value(true, Some(value)), "{value:?}");
        }
    }

    #[test]
    fn bool_flags_use_default_for_missing_empty_or_invalid_values() {
        for value in [None, Some(""), Some("   "), Some("definitely")] {
            assert!(env_bool_value(true, value));
            assert!(!env_bool_value(false, value));
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn development_seed_defaults_enabled_for_debug_builds() {
        assert!(default_seed_dev_data());
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn development_seed_defaults_disabled_for_release_builds() {
        assert!(!default_seed_dev_data());
    }

    #[test]
    fn development_seed_flag_overrides_profile_default_and_handles_empty_values() {
        let default = default_seed_dev_data();

        assert!(env_bool_value(false, Some("yes")));
        assert!(!env_bool_value(true, Some("no")));
        assert_eq!(env_bool_value(default, Some("")), default);
        assert_eq!(env_bool_value(default, Some("   ")), default);
    }
}
