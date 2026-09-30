//! Token-free configuration format and synchronous file mechanics.
//! Only the parent storage worker commits changes; this module schedules no work.
use super::Selection;
use serde::{Deserialize, Serialize};
use std::{ffi::OsString, path::PathBuf};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub server: Option<String>,
    pub saved: Option<Selection>,
    #[serde(default)]
    pub pending_deletions: Vec<Selection>,
}

#[cfg(not(test))]
pub(super) fn path() -> Option<PathBuf> {
    path_from_environment(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

pub(super) fn path_from_environment(
    xdg: Option<OsString>,
    home: Option<OsString>,
) -> Option<PathBuf> {
    let home = xdg
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|s| PathBuf::from(s).join(".config")))?;
    Some(home.join("hamlet").join("session.json"))
}

#[cfg(not(test))]
pub fn load() -> Config {
    load_at(path().as_deref())
}
pub(crate) fn load_at(path: Option<&std::path::Path>) -> Config {
    let Some(path) = path else {
        return Config::default();
    };
    let Ok(bytes) = std::fs::read(path) else {
        return Config::default();
    };
    let Ok(config): Result<Config, _> = serde_json::from_slice(&bytes) else {
        return Config::default();
    };
    if config
        .server
        .as_deref()
        .is_some_and(|s| crate::api::validate_server(s).is_ok())
        && config.saved.as_ref().is_none_or(|saved| {
            crate::api::validate_server(&saved.server).is_ok()
                && !saved.user.id.is_empty()
                && !saved.user.username.is_empty()
        })
        && config.pending_deletions.iter().all(|s| {
            crate::api::validate_server(&s.server).is_ok()
                && !s.user.id.is_empty()
                && !s.user.username.is_empty()
        })
    {
        config
    } else {
        Config::default()
    }
}

pub(super) fn write_config(path: Option<&std::path::Path>, config: &Config) -> Result<(), ()> {
    let path = path.ok_or(())?;
    std::fs::create_dir_all(path.parent().ok_or(())?).map_err(|_| ())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(config).map_err(|_| ())?).map_err(|_| ())?;
    std::fs::rename(tmp, path).map_err(|_| ())
}
