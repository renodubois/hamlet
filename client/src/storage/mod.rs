//! One selected server/user. The config contains only public identity metadata; the bearer
//! credential lives exclusively in the Linux Secret Service. All Secret Service calls run on a
//! dedicated blocking worker, never the GPUI thread. FIFO ordering makes logout deletion follow
//! any in-flight save, including a save whose UI deadline has already passed.
mod credentials;
mod preferences;

#[cfg(not(test))]
use credentials::SecretService;
pub(crate) use credentials::{Store, account};
pub use preferences::Config;
#[cfg(not(test))]
pub use preferences::load;
pub(crate) use preferences::load_at;
#[cfg(not(test))]
use preferences::path;
use preferences::write_config;

use crate::api::User;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Selection {
    pub server: String,
    pub user: User,
    pub expires_at: i64,
}

#[derive(PartialEq, Eq)]
pub enum Outcome {
    Token(Option<String>),
    Saved,
    SavedWithCleanupWarning,
    Deleted,
    // UI must not describe an ambiguous result as success.
    Failed,
    Stale,
    Remembered,
}
impl std::fmt::Debug for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Token(Some(_)) => f.write_str("Token([redacted])"),
            Self::Token(None) => f.write_str("Token(None)"),
            Self::Saved => f.write_str("Saved"),
            Self::SavedWithCleanupWarning => f.write_str("SavedWithCleanupWarning"),
            Self::Deleted => f.write_str("Deleted"),
            Self::Failed => f.write_str("Failed"),
            Self::Stale => f.write_str("Stale"),
            Self::Remembered => f.write_str("Remembered"),
        }
    }
}
type Reply = async_channel::Receiver<Outcome>;
enum Command {
    Read(Selection),
    Save(Selection, String, u64),
    Delete(Selection),
    Remember(String),
}
struct Job(Command, async_channel::Sender<Outcome>);

pub struct Persistence {
    tx: std::sync::mpsc::SyncSender<Job>,
    epoch: Arc<AtomicU64>,
}
impl Persistence {
    #[cfg(not(test))]
    pub fn new() -> Self {
        Self::start(SecretService, path())
    }
    pub(crate) fn start(mut store: impl Store, config_path: Option<PathBuf>) -> Self {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Job>(16);
        let epoch = Arc::new(AtomicU64::new(0));
        let gate = epoch.clone();
        std::thread::spawn(move || {
            while let Ok(Job(command, reply)) = rx.recv() {
                let result = match command {
                    Command::Remember(server) => {
                        let mut config = load_at(config_path.as_deref());
                        config.server = Some(server);
                        if write_config(config_path.as_deref(), &config).is_ok() {
                            Outcome::Remembered
                        } else {
                            Outcome::Failed
                        }
                    }
                    Command::Read(s) => {
                        if load_at(config_path.as_deref()).saved.as_ref() != Some(&s) {
                            Outcome::Stale
                        } else {
                            store.get(&s).map(Outcome::Token).unwrap_or(Outcome::Failed)
                        }
                    }
                    Command::Save(s, token, version) => {
                        if gate.load(Ordering::SeqCst) != version {
                            Outcome::Stale
                        } else {
                            let mut config = load_at(config_path.as_deref());
                            let old = config.saved.clone();
                            config.server = Some(s.server.clone());
                            config.saved = Some(s.clone());
                            config
                                .pending_deletions
                                .retain(|p| account(p) != account(&s));
                            if let Some(old) = &old
                                && account(old) != account(&s)
                                && !config
                                    .pending_deletions
                                    .iter()
                                    .any(|p| account(p) == account(old))
                            {
                                config.pending_deletions.push(old.clone());
                            }
                            // Put first: failed replacement must not point metadata at a
                            // credential which was never saved. For same-account replacement,
                            // retain the old token so a failed metadata commit can undo it.
                            let same_account =
                                old.as_ref().is_some_and(|o| account(o) == account(&s));
                            let prior = if same_account {
                                store.get(&s)
                            } else {
                                Ok(None)
                            };
                            if let Ok(prior) = prior {
                                if store.put(&s, &token).is_err() {
                                    // A backend can fail after mutating its entry. Attempt to
                                    // restore the old token (or remove a new account's entry).
                                    let _ = match prior.as_deref() {
                                        Some(old_token) => store.put(&s, old_token),
                                        None => store.delete(&s),
                                    };
                                    Outcome::Failed
                                } else if gate.load(Ordering::SeqCst) != version
                                    || write_config(config_path.as_deref(), &config).is_err()
                                {
                                    // Best-effort rollback; failure is explicitly ambiguous, never
                                    // reported as a successful save or a confirmed deletion.
                                    let rollback = match prior.as_deref() {
                                        Some(old_token) => store.put(&s, old_token),
                                        None => store.delete(&s),
                                    };
                                    if rollback.is_err() {
                                        Outcome::Failed
                                    } else if gate.load(Ordering::SeqCst) != version {
                                        Outcome::Stale
                                    } else {
                                        Outcome::Failed
                                    }
                                } else if let Some(old) = old.filter(|o| account(o) != account(&s))
                                {
                                    if store.delete(&old).is_err() {
                                        Outcome::SavedWithCleanupWarning
                                    } else {
                                        config
                                            .pending_deletions
                                            .retain(|p| account(p) != account(&old));
                                        // A failed metadata write leaves a harmless retry identity.
                                        if write_config(config_path.as_deref(), &config).is_err() {
                                            Outcome::SavedWithCleanupWarning
                                        } else {
                                            Outcome::Saved
                                        }
                                    }
                                } else {
                                    Outcome::Saved
                                }
                            } else {
                                Outcome::Failed
                            }
                        }
                    }
                    Command::Delete(s) => {
                        let mut config = load_at(config_path.as_deref());
                        // Never delete an account that has since been saved again.
                        if config
                            .saved
                            .as_ref()
                            .is_some_and(|saved| account(saved) == account(&s) && saved != &s)
                        {
                            Outcome::Stale
                        } else {
                            if config.saved.as_ref() == Some(&s) {
                                config.saved = None;
                            }
                            if !config
                                .pending_deletions
                                .iter()
                                .any(|p| account(p) == account(&s))
                            {
                                config.pending_deletions.push(s.clone());
                            }
                            // Record the deletion intent before touching the store. A failed
                            // delete must not restore the credential on the next launch.
                            if write_config(config_path.as_deref(), &config).is_err()
                                || store.delete(&s).is_err()
                            {
                                Outcome::Failed
                            } else {
                                config
                                    .pending_deletions
                                    .retain(|p| account(p) != account(&s));
                                if write_config(config_path.as_deref(), &config).is_err() {
                                    Outcome::Failed
                                } else {
                                    Outcome::Deleted
                                }
                            }
                        }
                    }
                };
                let _ = reply.try_send(result);
            }
        });
        Self { tx, epoch }
    }
    pub fn invalidate(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }
    fn dispatch(&self, cmd: Command) -> Reply {
        let (tx, rx) = async_channel::bounded(1);
        if let Err(err) = self.tx.try_send(Job(cmd, tx)) {
            let job = match err {
                std::sync::mpsc::TrySendError::Full(job)
                | std::sync::mpsc::TrySendError::Disconnected(job) => job,
            };
            let _ = job.1.try_send(Outcome::Failed);
        }
        rx
    }
    pub fn read(&self, selection: Selection) -> Reply {
        self.dispatch(Command::Read(selection))
    }
    pub fn save(&self, selection: Selection, token: String) -> Reply {
        self.dispatch(Command::Save(
            selection,
            token,
            self.epoch.load(Ordering::SeqCst),
        ))
    }
    pub fn delete(&self, selection: Selection) -> Reply {
        self.dispatch(Command::Delete(selection))
    }
    pub fn remember(&self, server: String) -> Reply {
        self.dispatch(Command::Remember(server))
    }
}

#[cfg(test)]
#[path = "tests/configuration.rs"]
mod configuration_tests;
#[cfg(test)]
#[path = "tests/protocol.rs"]
mod protocol_tests;

#[cfg(test)]
#[path = "tests/persistence.rs"]
mod tests;
