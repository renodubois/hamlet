use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use url::Url;

pub const DEFAULT_SERVER_URL: &str = "http://localhost:3030";
const PREFERENCES_FILE_NAME: &str = "preferences.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    pub server_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    #[serde(default, skip_serializing_if = "VoiceDevicePreferences::is_empty")]
    pub voice: VoiceDevicePreferences,
}

impl Preferences {
    pub fn with_server_url(server_url: impl Into<String>) -> Result<Self, PreferenceError> {
        Self::with_server_url_and_session_token(server_url, None)
    }

    pub fn with_server_url_and_session_token(
        server_url: impl Into<String>,
        session_token: Option<String>,
    ) -> Result<Self, PreferenceError> {
        Self::with_server_url_session_token_and_voice(
            server_url,
            session_token,
            VoiceDevicePreferences::default(),
        )
    }

    pub fn with_server_url_session_token_and_voice(
        server_url: impl Into<String>,
        session_token: Option<String>,
        voice: VoiceDevicePreferences,
    ) -> Result<Self, PreferenceError> {
        let server_url = normalize_server_url(&server_url.into())?;

        Ok(Self {
            server_url,
            session_token: normalize_session_token(session_token),
            voice: voice.normalized(),
        })
    }

    pub fn with_session_token(
        &self,
        session_token: Option<String>,
    ) -> Result<Self, PreferenceError> {
        Self::with_server_url_session_token_and_voice(
            self.server_url.clone(),
            session_token,
            self.voice.clone(),
        )
    }

    pub fn with_voice(&self, voice: VoiceDevicePreferences) -> Result<Self, PreferenceError> {
        Self::with_server_url_session_token_and_voice(
            self.server_url.clone(),
            self.session_token.clone(),
            voice,
        )
    }

    pub fn without_session_token(&self) -> Result<Self, PreferenceError> {
        self.with_session_token(None)
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            server_url: DEFAULT_SERVER_URL.to_string(),
            session_token: None,
            voice: VoiceDevicePreferences::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceDevicePreferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub microphone_device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_device_id: Option<String>,
}

impl VoiceDevicePreferences {
    pub fn new(microphone_device_id: Option<String>, output_device_id: Option<String>) -> Self {
        Self {
            microphone_device_id,
            output_device_id,
        }
        .normalized()
    }

    pub fn is_empty(&self) -> bool {
        self.microphone_device_id.is_none() && self.output_device_id.is_none()
    }

    pub fn normalized(self) -> Self {
        Self {
            microphone_device_id: normalize_optional_string(self.microphone_device_id),
            output_device_id: normalize_optional_string(self.output_device_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PreferenceError {
    #[error("server URL cannot be empty")]
    EmptyServerUrl,
    #[error("server URL must include a valid scheme and host")]
    InvalidServerUrl,
    #[error("server URL must use http or https")]
    UnsupportedScheme,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StorageError {
    #[error("could not determine the native config directory")]
    ConfigDirectoryUnavailable,
    #[error("could not read preferences at {path}: {message}")]
    Read { path: PathBuf, message: String },
    #[error("could not write preferences at {path}: {message}")]
    Write { path: PathBuf, message: String },
    #[error("could not parse preferences at {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("stored preferences are invalid: {0}")]
    InvalidPreferences(#[from] PreferenceError),
    #[error("fake storage failure: {0}")]
    Fake(String),
}

pub trait Storage {
    fn load_preferences(&self) -> Result<Preferences, StorageError>;
    fn save_preferences(&self, preferences: &Preferences) -> Result<(), StorageError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStorage {
    path: PathBuf,
}

impl FileStorage {
    pub fn new() -> Result<Self, StorageError> {
        let project_dirs = ProjectDirs::from("works.earendil", "Hamlet", "Hamlet")
            .ok_or(StorageError::ConfigDirectoryUnavailable)?;

        Ok(Self::at_path(
            project_dirs.config_dir().join(PREFERENCES_FILE_NAME),
        ))
    }

    pub fn at_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Storage for FileStorage {
    fn load_preferences(&self) -> Result<Preferences, StorageError> {
        if !self.path.exists() {
            return Ok(Preferences::default());
        }

        let contents = fs::read_to_string(&self.path).map_err(|error| StorageError::Read {
            path: self.path.clone(),
            message: error.to_string(),
        })?;

        let raw: Preferences =
            serde_json::from_str(&contents).map_err(|error| StorageError::Parse {
                path: self.path.clone(),
                message: error.to_string(),
            })?;

        Preferences::with_server_url_session_token_and_voice(
            raw.server_url,
            raw.session_token,
            raw.voice,
        )
        .map_err(StorageError::InvalidPreferences)
    }

    fn save_preferences(&self, preferences: &Preferences) -> Result<(), StorageError> {
        let preferences = Preferences::with_server_url_session_token_and_voice(
            preferences.server_url.clone(),
            preferences.session_token.clone(),
            preferences.voice.clone(),
        )?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| StorageError::Write {
                path: self.path.clone(),
                message: error.to_string(),
            })?;
        }

        let contents =
            serde_json::to_string_pretty(&preferences).map_err(|error| StorageError::Write {
                path: self.path.clone(),
                message: error.to_string(),
            })?;

        fs::write(&self.path, contents).map_err(|error| StorageError::Write {
            path: self.path.clone(),
            message: error.to_string(),
        })
    }
}

fn normalize_server_url(value: &str) -> Result<String, PreferenceError> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return Err(PreferenceError::EmptyServerUrl);
    }

    let parsed = Url::parse(trimmed).map_err(|_| PreferenceError::InvalidServerUrl)?;

    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(PreferenceError::UnsupportedScheme),
    }

    if parsed.host_str().is_none() {
        return Err(PreferenceError::InvalidServerUrl);
    }

    Ok(trimmed.trim_end_matches('/').to_string())
}

fn normalize_session_token(session_token: Option<String>) -> Option<String> {
    normalize_optional_string(session_token)
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();

        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_server_urls() {
        assert_eq!(
            Preferences::with_server_url(" http://localhost:3030/ "),
            Ok(Preferences {
                server_url: "http://localhost:3030".to_string(),
                session_token: None,
                voice: VoiceDevicePreferences::default(),
            })
        );
        assert_eq!(
            Preferences::with_server_url("ftp://localhost"),
            Err(PreferenceError::UnsupportedScheme)
        );
        assert_eq!(
            Preferences::with_server_url("localhost:3030"),
            Err(PreferenceError::UnsupportedScheme)
        );
        assert_eq!(
            Preferences::with_server_url(""),
            Err(PreferenceError::EmptyServerUrl)
        );
    }

    #[test]
    fn normalizes_session_tokens() {
        assert_eq!(
            Preferences::with_server_url_and_session_token(
                "http://localhost:3030",
                Some(" token ".to_string())
            ),
            Ok(Preferences {
                server_url: "http://localhost:3030".to_string(),
                session_token: Some("token".to_string()),
                voice: VoiceDevicePreferences::default(),
            })
        );
        assert_eq!(
            Preferences::with_server_url_and_session_token(
                "http://localhost:3030",
                Some(" ".to_string())
            ),
            Ok(Preferences::default())
        );
    }
}
