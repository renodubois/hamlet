//! Single source of truth for runtime configuration.
//!
//! `Config::try_from_env` is called once in `main`. Every other module receives
//! the values it needs through `web::Data` or function arguments — there
//! should be no `std::env::var` calls outside this file.

use std::path::{Path, PathBuf};

use thiserror::Error;
use url::Url;

use crate::voice::VoiceConfig;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3030";
const DATABASE_URL_ENV: &str = "HAMLET_DATABASE_URL";
const DATA_DIR_ENV: &str = "HAMLET_DATA_DIR";
const DEFAULT_DATABASE_FILE_NAME: &str = "hamlet.db";
#[cfg(any(target_os = "windows", target_os = "macos"))]
const DEFAULT_DATA_DIR_NAME: &str = "Hamlet";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const DEFAULT_DATA_DIR_NAME: &str = "hamlet";
const DEFAULT_LOG_FILTER: &str = "info";
const SENTRY_DSN_ENV: &str = "HAMLET_SENTRY_DSN";
const ACCOUNT_REGISTRATION_ENV: &str = "HAMLET_ACCOUNT_REGISTRATION_ENABLED";
const ACCOUNT_REGISTRATION_DEFAULT: bool = false;
const DEFAULT_UPLOADS_DIR: &str = "./uploads";
const DEFAULT_MESSAGE_ATTACHMENTS_DIR: &str = "./private-uploads/message-attachments";
const DEFAULT_BOOTSTRAP_DEFAULT_CHANNELS: bool = true;
const SEED_DEV_DATA_ENV: &str = "HAMLET_SEED_DEV_DATA";
const ALLOWED_ORIGINS_ENV: &str = "HAMLET_ALLOWED_ORIGINS";
const COOKIE_SECURE_ENV: &str = "HAMLET_COOKIE_SECURE";
const COOKIE_SAME_SITE_ENV: &str = "HAMLET_COOKIE_SAME_SITE";
const DEFAULT_COOKIE_SECURE: bool = false;
const DEFAULT_COOKIE_SAME_SITE: CookieSameSite = CookieSameSite::Lax;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: String,
    pub database_url: String,
    pub log_filter: String,
    pub account_registration_enabled: bool,
    /// Credentialed browser origins allowed by CORS.
    pub cors: CorsConfig,
    /// Policy applied when setting or clearing the session cookie.
    pub cookie: CookieConfig,
    /// Optional Sentry DSN. When set, server error-level tracing events are
    /// reported to Sentry in addition to the normal formatted logs.
    pub sentry_dsn: Option<String>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorsConfig {
    /// Exact origins allowed for credentialed browser requests, serialized as
    /// `scheme://host[:port]` with no path/query/fragment.
    pub allowed_origins: Vec<String>,
    /// Local development keeps the Electron/Vite localhost workflow working
    /// without requiring every checkout to set `HAMLET_ALLOWED_ORIGINS`.
    pub allow_localhost_origins: bool,
}

impl CorsConfig {
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        self.allowed_origins.iter().any(|allowed| allowed == origin)
            || (self.allow_localhost_origins && is_localhost_origin(origin))
    }
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: Vec::new(),
            allow_localhost_origins: default_allow_localhost_origins(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CookieConfig {
    pub secure: bool,
    pub same_site: CookieSameSite,
}

impl CookieConfig {
    pub fn new(secure: bool, same_site: CookieSameSite) -> Result<Self, ConfigError> {
        if same_site == CookieSameSite::None && !secure {
            return Err(invalid_env(
                COOKIE_SAME_SITE_ENV,
                "SameSite=None requires HAMLET_COOKIE_SECURE=true",
            ));
        }

        Ok(Self { secure, same_site })
    }
}

impl Default for CookieConfig {
    fn default() -> Self {
        Self {
            secure: DEFAULT_COOKIE_SECURE,
            same_site: DEFAULT_COOKIE_SAME_SITE,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CookieSameSite {
    Lax,
    Strict,
    None,
}

impl From<CookieSameSite> for actix_web::cookie::SameSite {
    fn from(value: CookieSameSite) -> Self {
        match value {
            CookieSameSite::Lax => Self::Lax,
            CookieSameSite::Strict => Self::Strict,
            CookieSameSite::None => Self::None,
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ConfigError {
    #[error("invalid {env}: {message}")]
    InvalidEnv { env: &'static str, message: String },
}

impl Config {
    /// Production-shaped config: read every value from the environment with
    /// sane defaults. `cargo run` and Docker Compose both go through here.
    pub fn try_from_env() -> Result<Self, ConfigError> {
        let data_dir = local_app_data_dir_from_env();
        let database_url = std::env::var(DATABASE_URL_ENV).ok();
        let sentry_dsn_override = std::env::var(SENTRY_DSN_ENV).ok();
        let sentry_dsn = sentry_dsn_from_env_value(sentry_dsn_override.as_deref());
        let account_registration_enabled_override = std::env::var(ACCOUNT_REGISTRATION_ENV).ok();
        let account_registration_enabled = account_registration_enabled_from_env_value(
            account_registration_enabled_override.as_deref(),
        );
        let allowed_origins_override = std::env::var(ALLOWED_ORIGINS_ENV).ok();
        let cookie_secure_override = std::env::var(COOKIE_SECURE_ENV).ok();
        let cookie_same_site_override = std::env::var(COOKIE_SAME_SITE_ENV).ok();

        Ok(Self {
            account_registration_enabled,
            bind_addr: env_or(DEFAULT_BIND_ADDR, "HAMLET_BIND_ADDR"),
            database_url: database_url_from_env_value(database_url.as_deref(), &data_dir),
            log_filter: env_or(DEFAULT_LOG_FILTER, "RUST_LOG"),
            cors: CorsConfig {
                allowed_origins: allowed_origins_from_env_value(
                    allowed_origins_override.as_deref(),
                )?,
                allow_localhost_origins: default_allow_localhost_origins(),
            },
            cookie: cookie_config_from_env_values(
                cookie_secure_override.as_deref(),
                cookie_same_site_override.as_deref(),
            )?,
            sentry_dsn,
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
        })
    }

    pub fn from_env() -> Self {
        Self::try_from_env()
            .unwrap_or_else(|error| panic!("invalid runtime configuration: {error}"))
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

fn sentry_dsn_from_env_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn account_registration_enabled_from_env_value(value: Option<&str>) -> bool {
    if let Some(val) = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        && (val == "true" || val == "false")
    {
        val == "true"
    } else {
        ACCOUNT_REGISTRATION_DEFAULT
    }
}

fn allowed_origins_from_env_value(value: Option<&str>) -> Result<Vec<String>, ConfigError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };

    let mut origins = Vec::new();
    for origin in value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let origin = parse_allowed_origin(origin)?;
        if !origins.contains(&origin) {
            origins.push(origin);
        }
    }

    Ok(origins)
}

fn parse_allowed_origin(value: &str) -> Result<String, ConfigError> {
    let url = Url::parse(value).map_err(|error| {
        invalid_env(
            ALLOWED_ORIGINS_ENV,
            format!("{value:?} is not a valid origin URL: {error}"),
        )
    })?;

    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(invalid_env(
            ALLOWED_ORIGINS_ENV,
            format!("{value:?} must use http or https"),
        ));
    }
    if url.host_str().is_none() {
        return Err(invalid_env(
            ALLOWED_ORIGINS_ENV,
            format!("{value:?} must include a host"),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid_env(
            ALLOWED_ORIGINS_ENV,
            format!("{value:?} must not include credentials"),
        ));
    }
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err(invalid_env(
            ALLOWED_ORIGINS_ENV,
            format!("{value:?} must be an origin without path, query, or fragment"),
        ));
    }

    Ok(url.origin().ascii_serialization())
}

fn cookie_config_from_env_values(
    secure_value: Option<&str>,
    same_site_value: Option<&str>,
) -> Result<CookieConfig, ConfigError> {
    let secure = env_bool_value_strict(DEFAULT_COOKIE_SECURE, COOKIE_SECURE_ENV, secure_value)?;
    let same_site = cookie_same_site_from_env_value(same_site_value)?;
    CookieConfig::new(secure, same_site)
}

fn cookie_same_site_from_env_value(value: Option<&str>) -> Result<CookieSameSite, ConfigError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(DEFAULT_COOKIE_SAME_SITE);
    };

    match value.to_ascii_lowercase().as_str() {
        "lax" => Ok(CookieSameSite::Lax),
        "strict" => Ok(CookieSameSite::Strict),
        "none" => Ok(CookieSameSite::None),
        _ => Err(invalid_env(
            COOKIE_SAME_SITE_ENV,
            format!("expected lax, strict, or none; got {value:?}"),
        )),
    }
}

fn is_localhost_origin(origin: &str) -> bool {
    let Ok(url) = Url::parse(origin) else {
        return false;
    };

    url.scheme() == "http"
        && url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
}

fn default_allow_localhost_origins() -> bool {
    cfg!(debug_assertions)
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

fn env_bool_value_strict(
    default: bool,
    env: &'static str,
    value: Option<&str>,
) -> Result<bool, ConfigError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(default);
    };

    parse_bool_flag(value)
        .ok_or_else(|| invalid_env(env, format!("expected a boolean flag; got {value:?}")))
}

fn parse_bool_flag(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "t" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "f" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

fn invalid_env(env: &'static str, message: impl Into<String>) -> ConfigError {
    ConfigError::InvalidEnv {
        env,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

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
    fn account_registration_can_be_set_or_defaults() {
        assert!(account_registration_enabled_from_env_value(Some("true")));
        assert!(!account_registration_enabled_from_env_value(Some("false")));

        for value in [None, Some(""), Some("TRUE"), Some("yes")] {
            assert!(
                !account_registration_enabled_from_env_value(value),
                "{value:?}"
            );
        }
    }

    #[test]
    fn allowed_origins_env_parses_and_canonicalizes_exact_origins() {
        assert_eq!(
            allowed_origins_from_env_value(None).unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            allowed_origins_from_env_value(Some("   ")).unwrap(),
            Vec::<String>::new()
        );

        assert_eq!(
            allowed_origins_from_env_value(Some(
                " https://CHAT.example.com, http://localhost:1422/, https://chat.example.com:443 "
            ))
            .unwrap(),
            vec!["https://chat.example.com", "http://localhost:1422"]
        );
    }

    #[test]
    fn allowed_origins_env_rejects_non_origins() {
        for value in [
            "*",
            "ftp://chat.example.com",
            "https://chat.example.com/path",
            "https://chat.example.com?debug=true",
            "https://user:pass@chat.example.com",
        ] {
            assert!(
                allowed_origins_from_env_value(Some(value)).is_err(),
                "{value:?}"
            );
        }
    }

    #[test]
    fn cors_config_allows_exact_configured_origins() {
        let config = CorsConfig {
            allowed_origins: vec!["https://chat.example.com".to_owned()],
            allow_localhost_origins: false,
        };

        assert!(config.is_origin_allowed("https://chat.example.com"));
        assert!(!config.is_origin_allowed("https://evil.example.com"));
        assert!(!config.is_origin_allowed("https://chat.example.com.evil"));
        assert!(!config.is_origin_allowed("http://localhost:1422"));
    }

    #[test]
    fn cors_config_can_allow_localhost_for_development() {
        let config = CorsConfig {
            allowed_origins: Vec::new(),
            allow_localhost_origins: true,
        };

        assert!(config.is_origin_allowed("http://localhost:1422"));
        assert!(config.is_origin_allowed("http://127.0.0.1:1422"));
        assert!(!config.is_origin_allowed("http://localhost.evil:1422"));
        assert!(!config.is_origin_allowed("https://localhost:1422"));
    }

    #[test]
    fn cookie_secure_env_is_strict_boolean_with_default() {
        assert!(!env_bool_value_strict(false, COOKIE_SECURE_ENV, None).unwrap());
        assert!(!env_bool_value_strict(false, COOKIE_SECURE_ENV, Some(" ")).unwrap());
        assert!(env_bool_value_strict(false, COOKIE_SECURE_ENV, Some("true")).unwrap());
        assert!(!env_bool_value_strict(true, COOKIE_SECURE_ENV, Some("off")).unwrap());
        assert!(env_bool_value_strict(false, COOKIE_SECURE_ENV, Some("sometimes")).is_err());
    }

    #[test]
    fn cookie_same_site_env_parses_lax_strict_none() {
        assert_eq!(
            cookie_same_site_from_env_value(None).unwrap(),
            CookieSameSite::Lax
        );
        assert_eq!(
            cookie_same_site_from_env_value(Some(" strict ")).unwrap(),
            CookieSameSite::Strict
        );
        assert_eq!(
            cookie_same_site_from_env_value(Some("NONE")).unwrap(),
            CookieSameSite::None
        );
        assert!(cookie_same_site_from_env_value(Some("wide-open")).is_err());
    }

    #[test]
    fn cookie_config_rejects_same_site_none_without_secure() {
        assert!(CookieConfig::new(false, CookieSameSite::None).is_err());
        assert_eq!(
            CookieConfig::new(true, CookieSameSite::None).unwrap(),
            CookieConfig {
                secure: true,
                same_site: CookieSameSite::None,
            }
        );
        assert!(cookie_config_from_env_values(None, Some("none")).is_err());
        assert_eq!(
            cookie_config_from_env_values(Some("true"), Some("none")).unwrap(),
            CookieConfig {
                secure: true,
                same_site: CookieSameSite::None,
            }
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
