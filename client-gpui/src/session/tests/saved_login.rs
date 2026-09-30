//! Application-lifetime saved-login interface with isolated metadata and a fake provider.
use super::*;
use crate::api::ApiFuture;
use crate::api::User;
use crate::api::test_support::{RequestAdapter, Response};
use crate::storage::{Config, Persistence};
use crate::test_support::storage::{Controlled, Shared};
use gpui_kit::TestAppContext;
use reqwest::{Request, StatusCode};
use std::sync::{Arc, Condvar, Mutex};

struct SavedAuth;
impl RequestAdapter for SavedAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        let response = match request.url().path() {
            "/api/v1/auth/login" => Response::controlled(
                StatusCode::OK,
                r#"{"user":{"id":"Ada","username":"Ada"},"access_token":"synthetic","expires_at":"2099-01-01T00:00:00Z"}"#,
            ),
            "/api/v1/me" => {
                assert_eq!(request.headers()["authorization"], "Bearer synthetic");
                Response::controlled(StatusCode::OK, r#"{"id":"Ada","username":"Ada"}"#)
            }
            "/api/v1/auth/logout" => Response::controlled(StatusCode::NO_CONTENT, ""),
            _ => panic!("unexpected session request"),
        };
        Box::pin(async move { Ok(response) })
    }
}

fn settle(cx: &mut TestAppContext, session: &mut SessionCoordinator, expected: &str) {
    let guard = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.executor().run_until_parked();
        while let Ok(update) = session.updates().try_recv() {
            session.apply(update);
        }
        if session
            .storage_feedback()
            .is_some_and(|s| s.contains(expected))
        {
            return;
        }
        assert!(
            std::time::Instant::now() < guard,
            "missing status {expected}"
        );
        std::thread::yield_now();
    }
}

fn take_updates(
    cx: &mut TestAppContext,
    session: &SessionCoordinator,
    count: usize,
) -> Vec<SessionUpdate> {
    let guard = std::time::Instant::now() + Duration::from_secs(5);
    let mut updates = Vec::new();
    while updates.len() < count {
        cx.executor().run_until_parked();
        while let Ok(update) = session.updates().try_recv() {
            updates.push(update);
        }
        assert!(
            std::time::Instant::now() < guard,
            "missing session delivery"
        );
        std::thread::yield_now();
    }
    assert_eq!(updates.len(), count);
    updates
}

#[gpui_kit::test]
fn late_old_cleanup_cannot_acknowledge_logout_of_a_new_identical_selection(
    cx: &mut TestAppContext,
) {
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let shared: Shared = Arc::new((
        Mutex::new((vec![], false, false, false, false, false)),
        Condvar::new(),
    ));
    let mut session = SessionCoordinator::new(
        HttpTransport::with_adapter(Arc::new(SavedAuth)),
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
        Config::default(),
        Some(Persistence::start(
            Controlled(shared.clone()),
            Some(path.clone()),
        )),
    );
    session.submit("Ada".into(), "password".into(), false);
    settle(cx, &mut session, "Login saved in");
    let old_selection = crate::storage::load_at(Some(&path)).saved.unwrap();
    session.logout();
    // Hold opaque deliveries after actual provider cleanup and server revocation.
    let old_cleanup = take_updates(cx, &session, 2);
    session.submit("Ada".into(), "new password".into(), false);
    for update in take_updates(cx, &session, 1) {
        session.apply(update);
    }
    let new_save = take_updates(cx, &session, 1);
    assert_eq!(
        crate::storage::load_at(Some(&path)).saved,
        Some(old_selection)
    );
    // Logout can precede delivery of the save confirmation. Equal public metadata
    // does not mean that the earlier deletion removed this newly written credential.
    session.logout();
    for update in old_cleanup.into_iter().chain(new_save) {
        session.apply(update);
    }
    settle(cx, &mut session, "Saved login removed");
    assert!(crate::storage::load_at(Some(&path)).saved.is_none());
    assert!(shared.0.lock().unwrap().0.is_empty());
    assert!(session.storage_retry().is_none());
}

struct UnavailableRestore;
impl RequestAdapter for UnavailableRestore {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.url().path() == "/api/v1/me" {
            Box::pin(async { Err(ApiError::Unavailable) })
        } else {
            SavedAuth.execute(request)
        }
    }
}

#[gpui_kit::test]
fn failed_manual_replacement_after_restore_failure_retains_the_rollback_cleanup_identity(
    cx: &mut TestAppContext,
) {
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let selection = crate::storage::Selection {
        server: DEFAULT_SERVER_URL.into(),
        user: User {
            id: "Ada".into(),
            username: "Ada".into(),
        },
        expires_at: 2_000_000_000,
    };
    let config = Config {
        server: Some(selection.server.clone()),
        saved: Some(selection.clone()),
        pending_deletions: vec![],
    };
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let shared: Shared = Arc::new((
        Mutex::new((
            vec![(crate::storage::account(&selection), "old-synthetic".into())],
            false,
            true,
            false,
            false,
            false,
        )),
        Condvar::new(),
    ));
    let mut session = SessionCoordinator::new(
        HttpTransport::with_adapter(Arc::new(UnavailableRestore)),
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
        config,
        Some(Persistence::start(
            Controlled(shared.clone()),
            Some(path.clone()),
        )),
    );
    settle(cx, &mut session, "Could not verify saved login");
    session.submit("Ada".into(), "new password".into(), false);
    settle(cx, &mut session, "memory-only");
    assert_eq!(crate::storage::load_at(Some(&path)).saved, Some(selection));
    session.logout();
    settle(cx, &mut session, "Saved login removed");
    assert!(session.storage_retry().is_none());
    assert!(crate::storage::load_at(Some(&path)).saved.is_none());
    assert!(shared.0.lock().unwrap().0.is_empty());
}

struct FixtureVerification;
impl RequestAdapter for FixtureVerification {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        assert_eq!(request.url().path(), "/api/v1/me");
        assert_eq!(request.headers()["authorization"], "Bearer synthetic");
        Box::pin(async {
            Ok(Response::controlled(
                StatusCode::OK,
                r#"{"id":"synthetic-user-1","username":"SyntheticAda"}"#,
            ))
        })
    }
}

#[gpui_kit::test]
fn restart_restores_selected_login_without_losing_old_cleanup_warning(cx: &mut TestAppContext) {
    cx.background_executor.allow_parking();
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../../../baseline-47/compatibility.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&fixtures["configs"][0]["value"]).unwrap(),
    )
    .unwrap();
    let config = crate::storage::load_at(Some(&path));
    let selected = config.saved.clone().unwrap();
    let old = config.pending_deletions[0].clone();
    let shared: Shared = Arc::new((
        Mutex::new((
            vec![
                (crate::storage::account(&selected), "synthetic".into()),
                (crate::storage::account(&old), "old".into()),
            ],
            false,
            false,
            true,
            false,
            false,
        )),
        Condvar::new(),
    ));
    let api = HttpTransport::with_adapter(Arc::new(FixtureVerification));
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let mut session = SessionCoordinator::new(
        api.clone(),
        execution.clone(),
        config,
        Some(Persistence::start(
            Controlled(shared.clone()),
            Some(path.clone()),
        )),
    );
    settle(cx, &mut session, "Login restored");
    assert_eq!(session.active().unwrap().user.username, "SyntheticAda");
    assert!(matches!(
        session.storage_retry(),
        Some(StorageRetry::Deletion)
    ));
    assert!(
        session
            .storage_feedback()
            .unwrap()
            .contains("Could not confirm deletion"),
        "restoration must not erase old cleanup warning"
    );
    drop(session);
    assert_eq!(
        crate::storage::load_at(Some(&path)).pending_deletions,
        vec![old]
    );
    // A later process resumes durable work without restoring or deleting the old identity.
    shared.0.lock().unwrap().3 = false;
    let mut restarted = SessionCoordinator::new(
        api,
        execution,
        crate::storage::load_at(Some(&path)),
        Some(Persistence::start(
            Controlled(shared.clone()),
            Some(path.clone()),
        )),
    );
    settle(cx, &mut restarted, "Login restored");
    assert!(restarted.storage_retry().is_none());
    assert_eq!(
        crate::storage::load_at(Some(&path)).saved,
        Some(selected.clone())
    );
    assert!(
        crate::storage::load_at(Some(&path))
            .pending_deletions
            .is_empty()
    );
    assert_eq!(
        shared.0.lock().unwrap().0.as_slice(),
        &[(crate::storage::account(&selected), "synthetic".into())]
    );
}

#[gpui_kit::test]
fn noncandidate_replacement_cleanup_remains_retryable_after_logout_and_restart(
    cx: &mut TestAppContext,
) {
    cx.background_executor.allow_parking();
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../../../baseline-47/compatibility.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&fixtures["configs"][1]["value"]).unwrap(),
    )
    .unwrap();
    let config = crate::storage::load_at(Some(&path));
    let old = config.saved.clone().unwrap();
    let shared: Shared = Arc::new((
        Mutex::new((
            vec![(crate::storage::account(&old), "old-synthetic".into())],
            false,
            false,
            true,
            false,
            false,
        )),
        Condvar::new(),
    ));
    let api = HttpTransport::with_adapter(Arc::new(SavedAuth));
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let mut session = SessionCoordinator::new(
        api.clone(),
        execution.clone(),
        config,
        Some(Persistence::start(
            Controlled(shared.clone()),
            Some(path.clone()),
        )),
    );
    // Noncandidate metadata must not prefill or start/offer restoration.
    assert_eq!(session.initial_username(), "");
    assert!(!session.pending());
    assert!(session.storage_retry().is_none());
    session.submit("Ada".into(), "password".into(), false);
    settle(cx, &mut session, "Login saved, but");
    settle(cx, &mut session, "Could not confirm deletion");
    assert!(session.active().is_some());
    assert!(matches!(
        session.storage_retry(),
        Some(StorageRetry::Deletion)
    ));
    assert_eq!(
        crate::storage::load_at(Some(&path)).pending_deletions,
        vec![old.clone()]
    );

    session.logout();
    for update in take_updates(cx, &session, 2) {
        session.apply(update);
    }
    assert!(session.active().is_none());
    assert!(
        session
            .storage_feedback()
            .unwrap()
            .contains("Could not confirm deletion")
    );
    assert!(matches!(
        session.storage_retry(),
        Some(StorageRetry::Deletion)
    ));
    drop(session);

    let config = crate::storage::load_at(Some(&path));
    assert!(config.saved.is_none());
    assert_eq!(config.pending_deletions.len(), 2);
    assert!(config.pending_deletions.contains(&old));
    let mut restarted = SessionCoordinator::new(
        api,
        execution,
        config,
        Some(Persistence::start(
            Controlled(shared.clone()),
            Some(path.clone()),
        )),
    );
    for update in take_updates(cx, &restarted, 2) {
        restarted.apply(update);
    }
    assert!(restarted.active().is_none());
    assert!(
        restarted
            .storage_feedback()
            .unwrap()
            .contains("Could not confirm deletion")
    );
    assert!(matches!(
        restarted.storage_retry(),
        Some(StorageRetry::Deletion)
    ));
    shared.0.lock().unwrap().3 = false;
    restarted.retry_storage();
    for update in take_updates(cx, &restarted, 1) {
        restarted.apply(update);
    }
    assert!(matches!(
        restarted.storage_retry(),
        Some(StorageRetry::Deletion)
    ));
    restarted.retry_storage();
    settle(cx, &mut restarted, "Saved login removed");
    assert!(restarted.storage_retry().is_none());
    assert!(
        crate::storage::load_at(Some(&path))
            .pending_deletions
            .is_empty()
    );
    assert!(shared.0.lock().unwrap().0.is_empty());
}

#[gpui_kit::test]
fn legacy_prefill_and_logged_out_restart_fixtures_use_session_startup(cx: &mut TestAppContext) {
    cx.background_executor.allow_parking();
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../../../baseline-47/compatibility.json")).unwrap();
    for index in 1..5 {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(
            &path,
            serde_json::to_vec(&fixtures["configs"][index]["value"]).unwrap(),
        )
        .unwrap();
        let config = crate::storage::load_at(Some(&path));
        let selected = config.saved.clone();
        let entries = config
            .saved
            .iter()
            .chain(config.pending_deletions.iter())
            .map(|s| (crate::storage::account(s), "synthetic".into()))
            .collect();
        let shared: Shared = Arc::new((
            Mutex::new((entries, false, false, true, false, false)),
            Condvar::new(),
        ));
        let api = HttpTransport::with_adapter(Arc::new(FixtureVerification));
        let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
        let mut session = SessionCoordinator::new(
            api.clone(),
            execution.clone(),
            config.clone(),
            Some(Persistence::start(
                Controlled(shared.clone()),
                Some(path.clone()),
            )),
        );
        assert_eq!(session.server(), config.server.as_deref().unwrap());
        if index == 3 {
            assert_eq!(session.initial_username(), "SyntheticAda");
            assert!(session.active().is_none());
            settle(cx, &mut session, "Login restored");
            assert_eq!(session.active().unwrap().user.username, "SyntheticAda");
        } else {
            assert_eq!(session.initial_username(), "");
            assert!(!session.pending());
            assert!(session.active().is_none());
            if index == 4 {
                settle(cx, &mut session, "Could not confirm deletion");
                assert!(matches!(
                    session.storage_retry(),
                    Some(StorageRetry::Deletion)
                ));
                drop(session);
                shared.0.lock().unwrap().3 = false;
                let mut restarted = SessionCoordinator::new(
                    api,
                    execution,
                    crate::storage::load_at(Some(&path)),
                    Some(Persistence::start(
                        Controlled(shared.clone()),
                        Some(path.clone()),
                    )),
                );
                settle(cx, &mut restarted, "Saved login removed");
                assert!(restarted.active().is_none());
                assert!(restarted.storage_retry().is_none());
                assert!(
                    crate::storage::load_at(Some(&path))
                        .pending_deletions
                        .is_empty()
                );
            } else {
                cx.executor().run_until_parked();
                assert!(
                    session.updates().try_recv().is_err(),
                    "distinct stored spelling is not an eligible candidate"
                );
                assert!(session.storage_retry().is_none());
            }
        }
        assert_eq!(crate::storage::load_at(Some(&path)).saved, selected);
    }
}

#[gpui_kit::test]
fn delayed_failed_deletion_cannot_remove_a_new_save_of_the_same_identity(cx: &mut TestAppContext) {
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let shared: Shared = Arc::new((
        Mutex::new((vec![], false, false, false, false, false)),
        Condvar::new(),
    ));
    let mut session = SessionCoordinator::new(
        HttpTransport::with_adapter(Arc::new(SavedAuth)),
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
        Config::default(),
        Some(Persistence::start(
            Controlled(shared.clone()),
            Some(path.clone()),
        )),
    );
    session.submit("Ada".into(), "password".into(), false);
    settle(cx, &mut session, "Login saved in");
    // Provider deletion remains blocked beyond its deadline while auth can move on.
    shared.0.lock().unwrap().3 = true;
    shared.0.lock().unwrap().4 = true;
    session.logout();
    cx.background_executor
        .advance_clock(Duration::from_secs(10));
    settle(cx, &mut session, "Could not confirm deletion");
    assert!(matches!(
        session.storage_retry(),
        Some(StorageRetry::Deletion)
    ));
    session.submit("Ada".into(), "new password".into(), false);
    settle(cx, &mut session, "Saving login");
    assert!(session.active().is_some());
    assert!(
        session.storage_retry().is_none(),
        "old retry may not target active account"
    );
    session.retry_storage(); // Inert even if a removed screen retained its retry intention.
    shared.0.lock().unwrap().4 = false;
    shared.1.notify_all();
    settle(cx, &mut session, "Login saved in");
    assert!(session.storage_retry().is_none());
    let selected = crate::storage::load_at(Some(&path)).saved.unwrap();
    // Read through storage's compatibility interface, never through coordinator internals.
    let probe = Persistence::start(Controlled(shared.clone()), Some(path));
    assert_eq!(
        probe.read(selected).recv_blocking().unwrap(),
        crate::storage::Outcome::Token(Some("synthetic".into()))
    );
    shared.0.lock().unwrap().3 = false;
    session.logout();
    settle(cx, &mut session, "Saved login removed");
    assert!(shared.0.lock().unwrap().0.is_empty());
}

#[gpui_kit::test]
fn successful_new_logout_preserves_warning_for_an_older_failed_deletion(cx: &mut TestAppContext) {
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let shared: Shared = Arc::new((
        Mutex::new((vec![], false, false, false, false, false)),
        Condvar::new(),
    ));
    let mut session = SessionCoordinator::new(
        HttpTransport::with_adapter(Arc::new(SavedAuth)),
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
        Config::default(),
        Some(Persistence::start(
            Controlled(shared.clone()),
            Some(path.clone()),
        )),
    );
    session.submit("Ada".into(), "password".into(), false);
    settle(cx, &mut session, "Login saved in");
    let old = crate::storage::load_at(Some(&path)).saved.unwrap();
    shared.0.lock().unwrap().3 = true;
    session.logout();
    for update in take_updates(cx, &session, 2) {
        session.apply(update);
    }
    assert!(
        session
            .storage_feedback()
            .unwrap()
            .contains("Could not confirm deletion")
    );

    session.change_server("https://other.example.test".into());
    session.submit("Ada".into(), "new password".into(), false);
    settle(cx, &mut session, "Login saved in");
    assert!(session.active().is_some());
    assert!(
        session
            .storage_feedback()
            .unwrap()
            .contains("Could not confirm deletion")
    );
    shared.0.lock().unwrap().3 = false;
    session.logout();
    for update in take_updates(cx, &session, 2) {
        session.apply(update);
    }
    assert!(session.active().is_none());
    assert!(matches!(
        session.storage_retry(),
        Some(StorageRetry::Deletion)
    ));
    let feedback = session.storage_feedback().unwrap();
    assert!(
        feedback.contains("Could not confirm deletion"),
        "{feedback}"
    );
    assert!(
        !feedback.contains("Removing"),
        "no deletion is still in flight"
    );
    let config = crate::storage::load_at(Some(&path));
    assert!(config.saved.is_none());
    assert_eq!(config.pending_deletions, vec![old]);
    session.retry_storage();
    settle(cx, &mut session, "Saved login removed");
    assert!(session.storage_retry().is_none());
    assert!(
        crate::storage::load_at(Some(&path))
            .pending_deletions
            .is_empty()
    );
    assert!(shared.0.lock().unwrap().0.is_empty());
}

#[gpui_kit::test]
fn expired_startup_keeps_prefill_but_never_verifies_the_candidate(cx: &mut TestAppContext) {
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let selection = crate::storage::Selection {
        server: DEFAULT_SERVER_URL.into(),
        user: User {
            id: "Ada".into(),
            username: "Ada".into(),
        },
        expires_at: 1_799_999_999,
    };
    let config = Config {
        server: Some(selection.server.clone()),
        saved: Some(selection),
        pending_deletions: vec![],
    };
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let shared: Shared = Arc::new((
        Mutex::new((vec![], false, false, false, false, false)),
        Condvar::new(),
    ));
    let mut session = SessionCoordinator::new(
        HttpTransport::with_adapter(Arc::new(SavedAuth)),
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
        config,
        Some(Persistence::start(Controlled(shared), Some(path))),
    );
    assert_eq!(session.initial_username(), "Ada");
    assert!(session.active().is_none());
    assert!(session.feedback().unwrap().contains("expired"));
    settle(cx, &mut session, "Saved login removed");
}

#[gpui_kit::test]
fn save_restore_and_logout_are_owned_without_either_screen(cx: &mut TestAppContext) {
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let shared: Shared = Arc::new((
        Mutex::new((vec![], false, false, false, false, false)),
        Condvar::new(),
    ));
    let api = HttpTransport::with_adapter(Arc::new(SavedAuth));
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let mut session = SessionCoordinator::new(
        api.clone(),
        execution.clone(),
        Config::default(),
        Some(Persistence::start(
            Controlled(shared.clone()),
            Some(path.clone()),
        )),
    );
    session.submit("Ada".into(), "synthetic password".into(), false);
    settle(cx, &mut session, "Login saved in");
    assert!(session.active().is_some());
    assert!(session.storage_retry().is_none());
    drop(session);

    let config = crate::storage::load_at(Some(&path));
    let mut restarted = SessionCoordinator::new(
        api,
        execution,
        config,
        Some(Persistence::start(Controlled(shared), Some(path.clone()))),
    );
    assert_eq!(restarted.initial_username(), "Ada");
    assert!(
        restarted.active().is_none(),
        "candidate is not an accepted context"
    );
    settle(cx, &mut restarted, "Login restored");
    assert_eq!(restarted.active().unwrap().user.username, "Ada");
    restarted.logout();
    assert!(restarted.active().is_none());
    settle(cx, &mut restarted, "Saved login removed");
    assert!(restarted.storage_retry().is_none());
    assert!(crate::storage::load_at(Some(&path)).saved.is_none());
}
