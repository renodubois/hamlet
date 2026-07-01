//! Single source of truth for runtime configuration.
//!
//! `Config::from_env` is called once in `main`. Every other module receives
//! the values it needs through `web::Data` or function arguments — there
//! should be no `std::env::var` calls outside this file.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::voice::VoiceConfig;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3030";
const DATABASE_URL_ENV: &str = "DATABASE_URL";
const DATA_DIR_ENV: &str = "HAMLET_DATA_DIR";
const CONFIG_FILE_ENV: &str = "HAMLET_CONFIG_FILE";
const DEFAULT_DATABASE_FILE_NAME: &str = "hamlet.db";
const DEFAULT_CONFIG_FILE_NAME: &str = "server-config.json";
#[cfg(any(target_os = "windows", target_os = "macos"))]
const DEFAULT_DATA_DIR_NAME: &str = "Hamlet";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const DEFAULT_DATA_DIR_NAME: &str = "hamlet";
const DEFAULT_LOG_FILTER: &str = "info";
const SENTRY_DSN_ENV: &str = "HAMLET_SENTRY_DSN";
const DEFAULT_UPLOADS_DIR: &str = "./uploads";
const DEFAULT_MESSAGE_ATTACHMENTS_DIR: &str = "./private-uploads/message-attachments";
const DEFAULT_BOOTSTRAP_DEFAULT_CHANNELS: bool = true;
const SEED_DEV_DATA_ENV: &str = "HAMLET_SEED_DEV_DATA";

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: String,
    pub database_url: String,
    pub log_filter: String,
    /// Optional Sentry DSN. When set, server error-level tracing events are
    /// reported to Sentry in addition to the normal formatted logs.
    pub sentry_dsn: Option<String>,
    pub uploads_dir: PathBuf,
    pub message_attachments_dir: PathBuf,
    /// Disk-backed server settings loaded at startup.
    pub server_settings: ServerSettings,
    /// The file path used for disk-backed server settings.
    pub settings_file: PathBuf,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ServerSettings {
    pub account_registration_enabled: bool,
    /// Optional Sentry DSN loaded from the disk-backed server config file.
    /// `HAMLET_SENTRY_DSN` can override this value at process start.
    pub sentry_dsn: Option<String>,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            account_registration_enabled: true,
            sentry_dsn: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read server config file {path:?}: {source}")]
    ReadServerSettings { path: PathBuf, source: io::Error },
    #[error("failed to write default server config file {path:?}: {source}")]
    WriteDefaultServerSettings { path: PathBuf, source: io::Error },
    #[error("failed to serialize default server config: {0}")]
    SerializeDefaultServerSettings(serde_json::Error),
    #[error("failed to parse server config file {path:?}: {source}")]
    ParseServerSettings {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl Config {
    /// Production-shaped config: read every value from the environment with
    /// sane defaults. `cargo run` and Docker Compose both go through here.
    pub fn from_env() -> Result<Self, ConfigError> {
        let data_dir = local_app_data_dir_from_env();
        let database_url = std::env::var(DATABASE_URL_ENV).ok();
        let settings_file_override = std::env::var(CONFIG_FILE_ENV).ok();
        let settings_file =
            server_settings_path_from_env_value(settings_file_override.as_deref(), &data_dir);
        let server_settings = load_server_settings(&settings_file)?;
        let sentry_dsn_override = std::env::var(SENTRY_DSN_ENV).ok();
        let sentry_dsn = sentry_dsn_from_env_value(sentry_dsn_override.as_deref())
            .or_else(|| sentry_dsn_from_env_value(server_settings.sentry_dsn.as_deref()));

        Ok(Self {
            bind_addr: env_or(DEFAULT_BIND_ADDR, "HAMLET_BIND_ADDR"),
            database_url: database_url_from_env_value(database_url.as_deref(), &data_dir),
            log_filter: env_or(DEFAULT_LOG_FILTER, "RUST_LOG"),
            sentry_dsn,
            uploads_dir: PathBuf::from(env_or(DEFAULT_UPLOADS_DIR, "HAMLET_UPLOADS_DIR")),
            message_attachments_dir: PathBuf::from(env_or(
                DEFAULT_MESSAGE_ATTACHMENTS_DIR,
                "HAMLET_MESSAGE_ATTACHMENTS_DIR",
            )),
            server_settings,
            settings_file,
            voice: VoiceConfig::from_env(),
            embed_fetcher_enabled: true,
            bootstrap_default_channels: env_bool(
                DEFAULT_BOOTSTRAP_DEFAULT_CHANNELS,
                "HAMLET_BOOTSTRAP_DEFAULT_CHANNELS",
            ),
            seed_dev_data: env_bool(default_seed_dev_data(), SEED_DEV_DATA_ENV),
        })
    }

    /// Build the same file-backed SQLite URL used by default local startup,
    /// but rooted at an explicit data directory for tests and smoke harnesses.
    pub fn default_database_url_for_data_dir(data_dir: impl AsRef<Path>) -> String {
        sqlite_file_database_url(&data_dir.as_ref().join(DEFAULT_DATABASE_FILE_NAME))
    }

    /// Build the default server settings path for an explicit data directory.
    pub fn default_settings_file_for_data_dir(data_dir: impl AsRef<Path>) -> PathBuf {
        data_dir.as_ref().join(DEFAULT_CONFIG_FILE_NAME)
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

fn server_settings_path_from_env_value(value: Option<&str>, data_dir: &Path) -> PathBuf {
    non_empty_env_value(value)
        .map(PathBuf::from)
        .unwrap_or_else(|| Config::default_settings_file_for_data_dir(data_dir))
}

fn load_server_settings(path: &Path) -> Result<ServerSettings, ConfigError> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            serde_json::from_str(&contents).map_err(|source| ConfigError::ParseServerSettings {
                path: path.to_path_buf(),
                source,
            })
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            write_default_server_settings(path)?;
            Ok(ServerSettings::default())
        }
        Err(source) => Err(ConfigError::ReadServerSettings {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn write_default_server_settings(path: &Path) -> Result<(), ConfigError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|source| ConfigError::WriteDefaultServerSettings {
            path: path.to_path_buf(),
            source,
        })?;
    }

    let body = serde_json::to_string_pretty(&ServerSettings::default())
        .map_err(ConfigError::SerializeDefaultServerSettings)?;
    fs::write(path, format!("{body}\n")).map_err(|source| ConfigError::WriteDefaultServerSettings {
        path: path.to_path_buf(),
        source,
    })
}

fn sentry_dsn_from_env_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
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
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn default_server_settings_enable_registration() {
        let settings = ServerSettings::default();

        assert!(settings.account_registration_enabled);
        assert_eq!(settings.sentry_dsn, None);
    }

    #[test]
    fn server_settings_path_defaults_to_data_dir_and_accepts_override() {
        let data_dir = PathBuf::from("local-data");

        assert_eq!(
            server_settings_path_from_env_value(None, &data_dir),
            data_dir.join(DEFAULT_CONFIG_FILE_NAME)
        );
        assert_eq!(
            server_settings_path_from_env_value(Some(""), &data_dir),
            data_dir.join(DEFAULT_CONFIG_FILE_NAME)
        );
        assert_eq!(
            server_settings_path_from_env_value(Some("custom/config.json"), &data_dir),
            PathBuf::from("custom/config.json")
        );
    }

    #[test]
    fn sentry_dsn_env_is_optional() {
        assert_eq!(sentry_dsn_from_env_value(None), None);
        assert_eq!(sentry_dsn_from_env_value(Some("")), None);
        assert_eq!(sentry_dsn_from_env_value(Some("   ")), None);
    }

    #[test]
    fn sentry_dsn_env_uses_trimmed_non_empty_value() {
        assert_eq!(
            sentry_dsn_from_env_value(Some(" https://public@example.com/1 ")),
            Some("https://public@example.com/1".to_owned())
        );
    }

    #[test]
    fn missing_server_settings_file_is_created_with_default_enabled() {
        let path = unique_tmp_config_path("server-config.json");
        let parent = path
            .parent()
            .expect("config path should have parent")
            .to_path_buf();

        let settings = load_server_settings(&path).expect("missing config should be created");

        assert!(settings.account_registration_enabled);
        let persisted = fs::read_to_string(&path).expect("default config should be written");
        assert!(persisted.contains("\"account_registration_enabled\": true"));
        fs::remove_dir_all(parent).expect("tmp config dir should be removable");
    }

    #[test]
    fn server_settings_file_can_disable_registration() {
        let path = unique_tmp_config_path("server-config.json");
        let parent = path.parent().expect("config path should have parent");
        fs::create_dir_all(parent).expect("tmp config dir should be creatable");
        fs::write(&path, "{\"account_registration_enabled\": false}\n")
            .expect("config should be writable");

        let settings = load_server_settings(&path).expect("config should parse");

        assert!(!settings.account_registration_enabled);
        fs::remove_dir_all(parent).expect("tmp config dir should be removable");
    }

    #[test]
    fn server_settings_file_can_set_sentry_dsn() {
        let path = unique_tmp_config_path("server-config.json");
        let parent = path.parent().expect("config path should have parent");
        fs::create_dir_all(parent).expect("tmp config dir should be creatable");
        fs::write(
            &path,
            "{\"sentry_dsn\": \"https://public@example.com/1\"}\n",
        )
        .expect("config should be writable");

        let settings = load_server_settings(&path).expect("config should parse");

        assert_eq!(
            settings.sentry_dsn,
            Some("https://public@example.com/1".to_owned())
        );
        fs::remove_dir_all(parent).expect("tmp config dir should be removable");
    }

    #[test]
    fn server_settings_missing_fields_use_registration_enabled_default() {
        let path = unique_tmp_config_path("server-config.json");
        let parent = path.parent().expect("config path should have parent");
        fs::create_dir_all(parent).expect("tmp config dir should be creatable");
        fs::write(&path, "{}\n").expect("config should be writable");

        let settings = load_server_settings(&path).expect("config should parse");

        assert!(settings.account_registration_enabled);
        assert_eq!(settings.sentry_dsn, None);
        fs::remove_dir_all(parent).expect("tmp config dir should be removable");
    }

    #[test]
    fn invalid_server_settings_file_returns_parse_error() {
        let path = unique_tmp_config_path("server-config.json");
        let parent = path.parent().expect("config path should have parent");
        fs::create_dir_all(parent).expect("tmp config dir should be creatable");
        fs::write(&path, "not json\n").expect("config should be writable");

        let error = load_server_settings(&path).expect_err("invalid config should fail");

        assert!(matches!(error, ConfigError::ParseServerSettings { .. }));
        fs::remove_dir_all(parent).expect("tmp config dir should be removable");
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

    fn unique_tmp_config_path(file_name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("hamlet-config-test-{}-{nanos}", std::process::id()))
            .join(file_name)
    }
}
