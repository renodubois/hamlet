//! Worked examples captured before rearchitecture, exercised with isolated files/providers.
use super::{Outcome, Persistence, load_at};
use crate::test_support::storage::{Controlled, Shared};
use std::sync::{Arc, Condvar, Mutex};

fn fixture(name: &str) -> serde_json::Value {
    let baseline: serde_json::Value =
        serde_json::from_str(include_str!("../../../baseline-47/compatibility.json")).unwrap();
    baseline["configs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|config| config["name"] == name)
        .unwrap()["value"]
        .clone()
}

#[test]
fn baseline_provider_keys_service_and_configuration_paths_are_unchanged() {
    let baseline: serde_json::Value =
        serde_json::from_str(include_str!("../../../baseline-47/compatibility.json")).unwrap();
    assert_eq!(super::credentials::SERVICE, baseline["provider_service"]);
    for case in baseline["account_key_cases"].as_array().unwrap() {
        let selection = serde_json::from_value(case["selection"].clone()).unwrap();
        assert_eq!(super::account(&selection), case["expected_account"]);
    }
    for case in baseline["path_cases"].as_array().unwrap() {
        let environment = &case["environment"];
        let path = super::preferences::path_from_environment(
            environment["XDG_CONFIG_HOME"].as_str().map(Into::into),
            environment["HOME"].as_str().map(Into::into),
        );
        assert_eq!(path, case["expected_path"].as_str().map(Into::into));
    }
}

#[test]
fn selected_server_spelling_stays_independent_of_the_saved_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    for (name, server) in [
        (
            "last-successful-server-differs-after-failed-save",
            "https://other.example.test",
        ),
        (
            "trailing-slash-is-not-the-same-selected-identity",
            "https://chat.example.test/",
        ),
    ] {
        std::fs::write(&path, serde_json::to_vec(&fixture(name)).unwrap()).unwrap();
        let config = load_at(Some(&path));
        assert_eq!(config.server.as_deref(), Some(server));
        assert_eq!(config.saved.unwrap().server, "https://chat.example.test");
        assert!(config.pending_deletions.is_empty());
    }
}

#[test]
fn existing_pending_deletions_resume_after_restart_without_restoring_deleted_identity() {
    for (name, pending_key) in [
        (
            "selected-login-with-old-user-deletion-pending",
            "26:https://other.example.test:synthetic-old-user",
        ),
        (
            "logged-out-durable-deletion-intent",
            "25:https://chat.example.test:synthetic-user-1",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(&path, serde_json::to_vec(&fixture(name)).unwrap()).unwrap();
        let config = load_at(Some(&path));
        let pending = config.pending_deletions[0].clone();
        let mut entries = vec![(pending_key.into(), "synthetic-old-secret".into())];
        if config.saved.is_some() {
            entries.push((
                "25:https://chat.example.test:synthetic-user-1".into(),
                "synthetic-current-secret".into(),
            ));
        }
        let shared: Shared = Arc::new((
            Mutex::new((entries, false, false, true, false, false)),
            Condvar::new(),
        ));
        let worker = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
        assert_eq!(
            worker.read(pending.clone()).recv_blocking().unwrap(),
            Outcome::Stale
        );
        assert_eq!(
            worker.delete(pending.clone()).recv_blocking().unwrap(),
            Outcome::Failed
        );
        assert_eq!(
            load_at(Some(&path)).pending_deletions,
            vec![pending.clone()]
        );
        assert_eq!(load_at(Some(&path)).saved, config.saved);
        drop(worker);

        shared.0.lock().unwrap().3 = false;
        let restarted = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
        let pending = load_at(Some(&path)).pending_deletions.remove(0);
        assert_eq!(
            restarted.delete(pending).recv_blocking().unwrap(),
            Outcome::Deleted
        );
        let after = load_at(Some(&path));
        assert!(after.pending_deletions.is_empty());
        assert_eq!(after.server, config.server);
        assert_eq!(after.saved, config.saved);
        assert!(
            !shared
                .0
                .lock()
                .unwrap()
                .0
                .iter()
                .any(|(key, _)| key == pending_key)
        );
        if let Some(saved) = after.saved {
            assert_eq!(
                restarted.read(saved).recv_blocking().unwrap(),
                Outcome::Token(Some("synthetic-current-secret".into()))
            );
        } else {
            assert!(shared.0.lock().unwrap().0.is_empty());
        }
    }
}

#[test]
fn existing_login_can_be_read_replaced_and_deleted_under_its_original_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    // Write the original fixture, including its omitted pending_deletions field.
    std::fs::write(
        &path,
        serde_json::to_vec(&fixture("legacy-omitted-pending-deletions")).unwrap(),
    )
    .unwrap();
    let shared: Shared = Arc::new((
        Mutex::new((
            vec![(
                "25:https://chat.example.test:synthetic-user-1".into(),
                "synthetic-existing-secret".into(),
            )],
            false,
            false,
            false,
            false,
            false,
        )),
        Condvar::new(),
    ));
    let worker = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
    let config = load_at(Some(&path));
    assert_eq!(config.server.as_deref(), Some("https://chat.example.test"));
    assert!(config.pending_deletions.is_empty());
    let old = config.saved.unwrap();
    assert_eq!(
        worker.read(old.clone()).recv_blocking().unwrap(),
        Outcome::Token(Some("synthetic-existing-secret".into()))
    );
    let mut renewed = old.clone();
    renewed.expires_at = 2_000_001_000;
    assert_eq!(
        worker
            .save(renewed.clone(), "synthetic-renewed-secret".into())
            .recv_blocking()
            .unwrap(),
        Outcome::Saved
    );
    assert_eq!(worker.delete(old).recv_blocking().unwrap(), Outcome::Stale);
    assert_eq!(load_at(Some(&path)).saved, Some(renewed.clone()));
    assert_eq!(
        worker.read(renewed.clone()).recv_blocking().unwrap(),
        Outcome::Token(Some("synthetic-renewed-secret".into()))
    );
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "server": "https://chat.example.test",
            "saved": {"server": "https://chat.example.test", "user": {"id": "synthetic-user-1", "username": "SyntheticAda"}, "expires_at": 2_000_001_000i64},
            "pending_deletions": []
        })
    );
    assert_eq!(
        worker.delete(renewed.clone()).recv_blocking().unwrap(),
        Outcome::Deleted
    );
    assert_eq!(
        worker.read(renewed).recv_blocking().unwrap(),
        Outcome::Stale
    );
    assert!(shared.0.lock().unwrap().0.is_empty());
    assert!(load_at(Some(&path)).saved.is_none());
    assert!(load_at(Some(&path)).pending_deletions.is_empty());
}
