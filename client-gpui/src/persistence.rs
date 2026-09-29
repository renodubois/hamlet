//! One selected server/user. The config contains only public identity metadata; the bearer
//! credential lives exclusively in the Linux Secret Service. All Secret Service calls run on a
//! dedicated blocking worker, never the GPUI thread. FIFO ordering makes logout deletion follow
//! any in-flight save, including a save whose UI deadline has already passed.
#[cfg(not(target_os = "linux"))]
compile_error!("Persistent sessions require the Linux Secret Service backend");

use crate::session::User;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

#[cfg(not(test))]
const SERVICE: &str = "org.hamlet.gpui.rewrite.session.v1";
pub const DEADLINE: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Selection {
    pub server: String,
    pub user: User,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub server: Option<String>,
    pub saved: Option<Selection>,
    #[serde(default)]
    pub pending_deletions: Vec<Selection>,
}

#[cfg(not(test))]
fn path() -> Option<PathBuf> {
    let home = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|s| PathBuf::from(s).join(".config")))?;
    Some(home.join("hamlet-gpui").join("session.json"))
}

#[cfg(not(test))]
pub fn load() -> Config {
    load_at(path().as_deref())
}
fn load_at(path: Option<&std::path::Path>) -> Config {
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
        .is_some_and(|s| crate::http::validate_server(s).is_ok())
        && config.saved.as_ref().is_none_or(|saved| {
            crate::http::validate_server(&saved.server).is_ok()
                && !saved.user.id.is_empty()
                && !saved.user.username.is_empty()
        })
        && config.pending_deletions.iter().all(|s| {
            crate::http::validate_server(&s.server).is_ok()
                && !s.user.id.is_empty()
                && !s.user.username.is_empty()
        })
    {
        config
    } else {
        Config::default()
    }
}

fn write_config(path: Option<&std::path::Path>, config: &Config) -> Result<(), ()> {
    let path = path.ok_or(())?;
    std::fs::create_dir_all(path.parent().ok_or(())?).map_err(|_| ())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(config).map_err(|_| ())?).map_err(|_| ())?;
    std::fs::rename(tmp, path).map_err(|_| ())
}

// No debug output of store errors: backend errors may include implementation-specific data.
pub(crate) trait Store: Send + 'static {
    fn get(&mut self, selection: &Selection) -> Result<Option<String>, ()>;
    fn put(&mut self, selection: &Selection, token: &str) -> Result<(), ()>;
    fn delete(&mut self, selection: &Selection) -> Result<(), ()>;
}
#[cfg(not(test))]
struct SecretService;
pub(crate) fn account(selection: &Selection) -> String {
    // Length prefix prevents ambiguous server/user pairings; the URL is already validated.
    format!(
        "{}:{}:{}",
        selection.server.len(),
        selection.server,
        selection.user.id
    )
}
#[cfg(not(test))]
impl SecretService {
    fn entry(selection: &Selection) -> Result<keyring::Entry, ()> {
        keyring::Entry::new(SERVICE, &account(selection)).map_err(|_| ())
    }
}
#[cfg(not(test))]
impl Store for SecretService {
    fn get(&mut self, s: &Selection) -> Result<Option<String>, ()> {
        match Self::entry(s)?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(()),
        }
    }
    fn put(&mut self, s: &Selection, token: &str) -> Result<(), ()> {
        Self::entry(s)?.set_password(token).map_err(|_| ())
    }
    fn delete(&mut self, s: &Selection) -> Result<(), ()> {
        match Self::entry(s)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(()),
        }
    }
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
pub(crate) mod tests {
    use super::*;
    use std::sync::{Condvar, Mutex};
    // Entries, block writes, fail writes, fail deletes.
    pub(crate) type Shared = Arc<(
        Mutex<(Vec<(String, String)>, bool, bool, bool, bool, bool)>,
        Condvar,
    )>;
    pub(crate) struct Controlled(pub(crate) Shared);
    impl Store for Controlled {
        fn get(&mut self, s: &Selection) -> Result<Option<String>, ()> {
            Ok(self
                .0
                .0
                .lock()
                .unwrap()
                .0
                .iter()
                .find(|(k, _)| k == &account(s))
                .map(|(_, v)| v.clone()))
        }
        fn put(&mut self, s: &Selection, t: &str) -> Result<(), ()> {
            let (lock, signal) = &*self.0;
            let mut state = lock.lock().unwrap();
            while state.1 {
                state = signal.wait(state).unwrap();
            }
            if state.2 {
                return Err(());
            }
            state.0.retain(|(k, _)| k != &account(s));
            state.0.push((account(s), t.into()));
            Ok(())
        }
        fn delete(&mut self, s: &Selection) -> Result<(), ()> {
            let (lock, signal) = &*self.0;
            let mut state = lock.lock().unwrap();
            state.5 = true;
            signal.notify_all();
            while state.4 {
                state = signal.wait(state).unwrap();
            }
            if state.3 {
                return Err(());
            }
            state.0.retain(|(k, _)| k != &account(s));
            Ok(())
        }
    }
    #[test]
    fn keys_isolate_server_and_user() {
        let s = |server: &str, id: &str| Selection {
            server: server.into(),
            user: User {
                id: id.into(),
                username: "Ada".into(),
            },
            expires_at: 100,
        };
        assert_ne!(account(&s("https://a", "1")), account(&s("https://b", "1")));
        assert_ne!(account(&s("https://a", "1")), account(&s("https://a", "2")));
    }
    fn selection(server: &str, user: &str, expiry: i64) -> Selection {
        Selection {
            server: server.into(),
            user: User {
                id: user.into(),
                username: "Ada".into(),
            },
            expires_at: expiry,
        }
    }
    #[test]
    fn save_read_replace_same_account_and_delete_without_plaintext_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let shared = Arc::new((
            Mutex::new((vec![], false, false, false, false, false)),
            Condvar::new(),
        ));
        let worker = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
        let first = selection("https://one.example", "1", 100);
        assert_eq!(
            worker
                .save(first.clone(), "private".into())
                .recv_blocking()
                .unwrap(),
            Outcome::Saved
        );
        let config = std::fs::read_to_string(&path).unwrap();
        assert!(config.contains("https://one.example"));
        assert!(!config.contains("private"));
        assert_eq!(
            worker.read(first.clone()).recv_blocking().unwrap(),
            Outcome::Token(Some("private".into()))
        );
        let renewed = selection("https://one.example", "1", 200);
        assert_eq!(
            worker
                .save(renewed.clone(), "renewed".into())
                .recv_blocking()
                .unwrap(),
            Outcome::Saved
        );
        assert_eq!(
            worker.read(renewed.clone()).recv_blocking().unwrap(),
            Outcome::Token(Some("renewed".into()))
        );
        assert_eq!(
            worker.delete(renewed).recv_blocking().unwrap(),
            Outcome::Deleted
        );
        assert!(shared.0.lock().unwrap().0.is_empty());
        assert!(load_at(Some(&path)).saved.is_none());
    }
    #[test]
    fn storage_failures_and_cross_account_cleanup_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let shared = Arc::new((
            Mutex::new((vec![], false, true, false, false, false)),
            Condvar::new(),
        ));
        let worker = Persistence::start(Controlled(shared.clone()), Some(path));
        let alice = selection("https://one.example", "1", 100);
        assert_eq!(
            worker
                .save(alice.clone(), "private".into())
                .recv_blocking()
                .unwrap(),
            Outcome::Failed
        );
        assert!(shared.0.lock().unwrap().0.is_empty());
        shared.0.lock().unwrap().2 = false;
        assert_eq!(
            worker
                .save(alice.clone(), "private".into())
                .recv_blocking()
                .unwrap(),
            Outcome::Saved
        );
        shared.0.lock().unwrap().3 = true;
        assert_eq!(
            worker
                .save(selection("https://two.example", "1", 100), "other".into())
                .recv_blocking()
                .unwrap(),
            Outcome::SavedWithCleanupWarning
        );
        assert_eq!(
            shared.0.lock().unwrap().0.len(),
            2,
            "old credential cannot silently disappear on failed delete"
        );
    }
    #[test]
    fn failed_old_account_cleanup_survives_restart_without_restoring_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let shared = Arc::new((
            Mutex::new((vec![], false, false, false, false, false)),
            Condvar::new(),
        ));
        let a = selection("https://a.example", "alice", 100);
        let b = selection("https://b.example", "bob", 200);
        let worker = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
        assert_eq!(
            worker
                .save(a.clone(), "old-secret".into())
                .recv_blocking()
                .unwrap(),
            Outcome::Saved
        );
        shared.0.lock().unwrap().3 = true;
        assert_eq!(
            worker
                .save(b.clone(), "new-secret".into())
                .recv_blocking()
                .unwrap(),
            Outcome::SavedWithCleanupWarning
        );
        let config = load_at(Some(&path));
        assert_eq!(config.server.as_deref(), Some(b.server.as_str()));
        assert_eq!(config.saved, Some(b.clone()));
        assert_eq!(config.pending_deletions, vec![a.clone()]);
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(!json.contains("old-secret") && !json.contains("new-secret"));
        drop(worker);
        let restarted = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
        shared.0.lock().unwrap().3 = false;
        for pending in load_at(Some(&path)).pending_deletions {
            assert_eq!(
                restarted.delete(pending).recv_blocking().unwrap(),
                Outcome::Deleted
            );
        }
        assert!(load_at(Some(&path)).pending_deletions.is_empty());
        assert_eq!(
            restarted.read(b).recv_blocking().unwrap(),
            Outcome::Token(Some("new-secret".into()))
        );
        assert_eq!(shared.0.lock().unwrap().0.len(), 1);
    }

    #[test]
    fn last_authenticated_server_is_independent_of_failed_credential_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let shared = Arc::new((
            Mutex::new((vec![], false, false, false, false, false)),
            Condvar::new(),
        ));
        let a = selection("https://a.example", "alice", 100);
        let b = selection("https://b.example", "bob", 200);
        let worker = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
        assert_eq!(
            worker
                .save(a.clone(), "old-secret".into())
                .recv_blocking()
                .unwrap(),
            Outcome::Saved
        );
        // Editing a form alone does not touch config; authentication does.
        assert_eq!(
            load_at(Some(&path)).server.as_deref(),
            Some(a.server.as_str())
        );
        shared.0.lock().unwrap().2 = true;
        assert_eq!(
            worker.remember(b.server.clone()).recv_blocking().unwrap(),
            Outcome::Remembered
        );
        assert_eq!(
            worker
                .save(b.clone(), "new-secret".into())
                .recv_blocking()
                .unwrap(),
            Outcome::Failed
        );
        drop(worker);
        let config = load_at(Some(&path));
        assert_eq!(config.server.as_deref(), Some(b.server.as_str()));
        assert_eq!(config.saved, Some(a.clone()));
        assert_ne!(
            config.saved.as_ref().unwrap().server,
            config.server.unwrap()
        );
        assert_eq!(
            shared.0.lock().unwrap().0,
            vec![(account(&a), "old-secret".into())]
        );
        assert!(
            !std::fs::read_to_string(path)
                .unwrap()
                .contains("old-secret")
        );
    }

    #[test]
    fn failed_replacement_preserves_previous_selection_and_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let shared = Arc::new((
            Mutex::new((vec![], false, false, false, false, false)),
            Condvar::new(),
        ));
        let worker = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
        let alice = selection("https://one.example", "alice", 100);
        assert_eq!(
            worker
                .save(alice.clone(), "old".into())
                .recv_blocking()
                .unwrap(),
            Outcome::Saved
        );
        shared.0.lock().unwrap().2 = true;
        for replacement in [
            selection("https://one.example", "bob", 200),
            selection("https://two.example", "alice", 200),
        ] {
            assert_eq!(
                worker
                    .save(replacement, "new".into())
                    .recv_blocking()
                    .unwrap(),
                Outcome::Failed
            );
            assert_eq!(load_at(Some(&path)).saved, Some(alice.clone()));
            assert_eq!(
                worker.read(alice.clone()).recv_blocking().unwrap(),
                Outcome::Token(Some("old".into()))
            );
        }
        assert_eq!(shared.0.lock().unwrap().0.len(), 1);
    }
    #[test]
    fn delayed_failed_deletions_can_retry_without_touching_new_user() {
        let shared = Arc::new((
            Mutex::new((vec![], false, false, true, true, false)),
            Condvar::new(),
        ));
        let dir = tempfile::tempdir().unwrap();
        let worker = Persistence::start(
            Controlled(shared.clone()),
            Some(dir.path().join("session.json")),
        );
        let first = selection("https://first.example", "alice", 100);
        let second = selection("https://second.example", "bob", 100);
        let fresh = selection("https://fresh.example", "charlie", 100);
        for s in [&first, &second, &fresh] {
            shared
                .0
                .lock()
                .unwrap()
                .0
                .push((account(s), s.user.id.clone()));
        }
        let first_delete = worker.delete(first.clone());
        let second_delete = worker.delete(second.clone());
        {
            let mut state = shared.0.lock().unwrap();
            while !state.5 {
                state = shared.1.wait(state).unwrap();
            }
            assert_eq!(state.0.len(), 3, "first delete is held before its failure");
            state.4 = false;
            shared.1.notify_all();
        }
        assert_eq!(first_delete.recv_blocking().unwrap(), Outcome::Failed);
        assert_eq!(second_delete.recv_blocking().unwrap(), Outcome::Failed);
        assert_eq!(shared.0.lock().unwrap().0.len(), 3);
        shared.0.lock().unwrap().3 = false;
        assert_eq!(
            worker.delete(first).recv_blocking().unwrap(),
            Outcome::Deleted
        );
        assert_eq!(
            worker.delete(second).recv_blocking().unwrap(),
            Outcome::Deleted
        );
        assert_eq!(
            shared.0.lock().unwrap().0,
            vec![(account(&fresh), "charlie".into())]
        );
    }
    #[test]
    fn save_completing_after_invalidation_is_cleaned_before_deletion_reply() {
        struct SlowStore {
            state: Arc<(Mutex<(bool, bool)>, Condvar)>,
            tokens: Arc<Mutex<Vec<String>>>,
        }
        impl Store for SlowStore {
            fn get(&mut self, _: &Selection) -> Result<Option<String>, ()> {
                Ok(self.tokens.lock().unwrap().last().cloned())
            }
            fn put(&mut self, _: &Selection, token: &str) -> Result<(), ()> {
                self.tokens.lock().unwrap().push(token.into());
                let (lock, signal) = &*self.state;
                let mut state = lock.lock().unwrap();
                state.0 = true;
                signal.notify_all();
                while !state.1 {
                    state = signal.wait(state).unwrap();
                }
                Ok(())
            }
            fn delete(&mut self, _: &Selection) -> Result<(), ()> {
                self.tokens.lock().unwrap().clear();
                Ok(())
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new((Mutex::new((false, false)), Condvar::new()));
        let tokens = Arc::new(Mutex::new(vec![]));
        let worker = Persistence::start(
            SlowStore {
                state: state.clone(),
                tokens: tokens.clone(),
            },
            Some(dir.path().join("session.json")),
        );
        let chosen = selection("https://one.example", "1", 100);
        let save = worker.save(chosen.clone(), "secret".into());
        {
            let (lock, signal) = &*state;
            let mut value = lock.lock().unwrap();
            while !value.0 {
                value = signal.wait(value).unwrap();
            }
        }
        worker.invalidate();
        let delete = worker.delete(chosen);
        {
            let (lock, signal) = &*state;
            lock.lock().unwrap().1 = true;
            signal.notify_all();
        }
        assert_eq!(save.recv_blocking().unwrap(), Outcome::Stale);
        assert_eq!(delete.recv_blocking().unwrap(), Outcome::Deleted);
        assert!(tokens.lock().unwrap().is_empty());
    }
    #[test]
    fn late_save_is_removed_before_logout_delete_completes() {
        let shared = Arc::new((
            Mutex::new((vec![], true, false, false, false, false)),
            Condvar::new(),
        ));
        let dir = tempfile::tempdir().unwrap();
        let worker = Persistence::start(
            Controlled(shared.clone()),
            Some(dir.path().join("session.json")),
        );
        let selection = Selection {
            server: "http://127.0.0.1:8081".into(),
            user: User {
                id: "1".into(),
                username: "Ada".into(),
            },
            expires_at: 100,
        };
        let save = worker.save(selection.clone(), "secret".into());
        // Even if invalidation happens before the worker starts, queued deletion still wins.
        worker.invalidate();
        let delete = worker.delete(selection);
        {
            let mut state = shared.0.lock().unwrap();
            state.1 = false;
            shared.1.notify_all();
        }
        assert!(matches!(save.recv_blocking().unwrap(), Outcome::Stale));
        assert_eq!(delete.recv_blocking().unwrap(), Outcome::Deleted);
        assert!(shared.0.lock().unwrap().0.is_empty());
    }
}
