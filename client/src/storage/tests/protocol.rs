//! Failures and queue ordering through the storage interface, never a real provider.
use super::{Outcome, Persistence, Selection, load_at};
use crate::api::User;
use crate::test_support::storage::{Controlled, Shared};
use std::sync::{Arc, Condvar, Mutex};

fn provider() -> Shared {
    Arc::new((
        Mutex::new((vec![], false, false, false, false, false)),
        Condvar::new(),
    ))
}

fn selection() -> Selection {
    Selection {
        server: "https://chat.example.test".into(),
        user: User {
            id: "synthetic-user".into(),
            username: "SyntheticAda".into(),
        },
        expires_at: 2_000_000_000,
    }
}

#[test]
fn metadata_failure_rolls_back_replacement_and_preserves_previous_login() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let shared = provider();
    let worker = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
    let old = selection();
    assert_eq!(
        worker
            .save(old.clone(), "synthetic-old-secret".into())
            .recv_blocking()
            .unwrap(),
        Outcome::Saved
    );
    let original_metadata = std::fs::read(&path).unwrap();
    // Fail only the metadata commit, after the provider has accepted the new token.
    std::fs::create_dir(path.with_extension("tmp")).unwrap();
    let mut renewed = old.clone();
    renewed.expires_at += 100;
    let mut different = renewed.clone();
    different.user.id = "synthetic-new-user".into();
    for replacement in [renewed, different] {
        assert_eq!(
            worker
                .save(replacement, "synthetic-new-secret".into())
                .recv_blocking()
                .unwrap(),
            Outcome::Failed
        );
        assert_eq!(std::fs::read(&path).unwrap(), original_metadata);
        assert_eq!(
            worker.read(old.clone()).recv_blocking().unwrap(),
            Outcome::Token(Some("synthetic-old-secret".into()))
        );
        assert_eq!(shared.0.lock().unwrap().0.len(), 1);
    }
    // Without durable intent, deletion must not touch the provider either.
    assert_eq!(
        worker.delete(old.clone()).recv_blocking().unwrap(),
        Outcome::Failed
    );
    assert_eq!(
        worker.read(old.clone()).recv_blocking().unwrap(),
        Outcome::Token(Some("synthetic-old-secret".into()))
    );
    std::fs::remove_dir(path.with_extension("tmp")).unwrap();
    assert_eq!(
        worker.delete(old).recv_blocking().unwrap(),
        Outcome::Deleted
    );
    assert!(shared.0.lock().unwrap().0.is_empty());
}

#[test]
fn full_queue_rejects_without_blocking_and_preferences_wait_for_provider_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let shared = provider();
    let worker = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
    let selected = selection();
    assert_eq!(
        worker
            .save(selected.clone(), "synthetic-secret".into())
            .recv_blocking()
            .unwrap(),
        Outcome::Saved
    );
    shared.0.lock().unwrap().4 = true;
    let deletion = worker.delete(selected.clone());
    {
        let mut state = shared.0.lock().unwrap();
        while !state.5 {
            state = shared.1.wait(state).unwrap();
        }
    }
    let queued: Vec<_> = (0..16)
        .map(|n| worker.remember(format!("https://queued-{n}.example")))
        .collect();
    assert_eq!(
        worker
            .remember("https://overflow.example".into())
            .try_recv()
            .unwrap(),
        Outcome::Failed
    );
    assert!(
        queued
            .iter()
            .all(|reply| matches!(reply.try_recv(), Err(async_channel::TryRecvError::Empty)))
    );
    let during = load_at(Some(&path));
    assert_eq!(during.server.as_deref(), Some("https://chat.example.test"));
    assert!(during.saved.is_none());
    assert_eq!(during.pending_deletions, vec![selected]);
    {
        shared.0.lock().unwrap().4 = false;
        shared.1.notify_all();
    }
    assert_eq!(deletion.recv_blocking().unwrap(), Outcome::Deleted);
    for reply in queued {
        assert_eq!(reply.recv_blocking().unwrap(), Outcome::Remembered);
    }
    let after = load_at(Some(&path));
    assert_eq!(after.server.as_deref(), Some("https://queued-15.example"));
    assert!(after.saved.is_none());
    assert!(after.pending_deletions.is_empty());
    assert!(shared.0.lock().unwrap().0.is_empty());
}
