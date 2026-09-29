use super::*;
use crate::test_support::storage::Controlled;
use std::sync::{Condvar, Mutex};
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
