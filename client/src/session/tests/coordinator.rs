//! Owned session interface, shared controlled execution; no view or real provider.
use super::*;
use crate::api::test_support::{RequestAdapter, Response};
use crate::api::{ApiFuture, HttpTransport, User};
use crate::runtime::Execution;
use gpui_kit::TestAppContext;
use reqwest::{Request, StatusCode};
use std::{sync::Arc, time::Duration};

#[gpui_kit::test]
fn session_loss_closes_surviving_chat_handles_before_any_host_update(cx: &mut TestAppContext) {
    for reason in ["logout", "expiry", "server", "rejection"] {
        let (mut session, calls) = controlled(cx);
        let generation = accept(cx, &mut session, &calls, "Old", 1_800_000_100);
        let old_client = session.active().map(Session::client).unwrap();
        let activity = session.chat().unwrap();
        activity.start();
        cx.executor().run_until_parked();
        assert!(
            activity
                .apply(activity.updates().try_recv().unwrap())
                .is_none()
        );
        cx.executor().run_until_parked();
        calls
            .try_recv()
            .unwrap()
            .reply
            .try_send(Ok(Response::controlled(
                StatusCode::OK,
                r#"{"items":[{"id":"1","name":"General","type":"text"}]}"#,
            )))
            .unwrap();
        cx.executor().run_until_parked();
        assert!(
            activity
                .apply(activity.updates().try_recv().unwrap())
                .is_none()
        );
        cx.executor().run_until_parked();
        calls
            .try_recv()
            .unwrap()
            .reply
            .try_send(Ok(Response::controlled(
                StatusCode::OK,
                r#"{"items":[],"next_cursor":"older"}"#,
            )))
            .unwrap();
        cx.executor().run_until_parked();
        assert!(
            activity
                .apply(activity.updates().try_recv().unwrap())
                .is_none()
        );
        activity.edit_draft("private draft".into());
        activity.request_older();
        activity.send();
        activity.create_channel("Room");
        cx.executor().run_until_parked();
        let mut pending = Vec::new();
        while let Ok(call) = calls.try_recv() {
            pending.push(call);
        }
        assert_eq!(pending.len(), 3);
        // Finish before invalidation but leave deliveries queued for the old chat.
        for call in pending {
            call.reply
                .try_send(Ok(Response::controlled(
                    StatusCode::UNAUTHORIZED,
                    r#"{"error":{"code":"unauthorized"}}"#,
                )))
                .unwrap();
        }
        cx.executor().run_until_parked();
        assert_eq!(activity.updates().len(), 3);
        match reason {
            "logout" => session.logout(),
            "expiry" => session.expire(1_800_000_100),
            "server" => session.change_server("https://new.example".into()),
            _ => session.protected_rejected(generation),
        }
        // No shell, observer or lifecycle consumer has run yet.
        assert!(session.active().is_none());
        assert!(session.chat().is_none());
        assert!(activity.read().channels.is_none());
        assert!(activity.read().history.is_empty());
        assert!(activity.read().drafts.is_empty());
        activity.start();
        activity.edit_draft("must stay closed".into());
        activity.send();
        activity.request_older();
        activity.create_channel("Must stay closed");
        cx.executor().run_until_parked();
        while let Ok(call) = calls.try_recv() {
            assert_eq!(call.request.url().path(), "/api/v1/auth/logout");
            call.reply
                .try_send(Ok(Response::controlled(StatusCode::NO_CONTENT, "")))
                .unwrap();
        }
        deliver(cx, &mut session);
        let current = accept(cx, &mut session, &calls, "New", 1_800_000_100);
        assert_ne!(generation, current);
        while let Ok(update) = activity.updates().try_recv() {
            assert!(activity.apply(update).is_none());
        }
        session.chat_ended(SessionEnd::Rejected(generation));
        assert_eq!(session.session_generation(), Some(current));
        assert_eq!(session.active().unwrap().user.username, "New");
        assert!(activity.read().drafts.is_empty());
        assert!(calls.try_recv().is_err());
        drop(old_client); // An independently retained bound client never reopens activity.
    }
}

struct Login;
impl RequestAdapter for Login {
    fn execute(&self, _: Request) -> ApiFuture<Result<Response, ApiError>> {
        Box::pin(async {
            Ok(Response::controlled(
                StatusCode::OK,
                r#"{"user":{"id":"1","username":"Ada"},"access_token":"synthetic","expires_at":"2099-01-01T00:00:00Z"}"#,
            ))
        })
    }
}

#[gpui_kit::test]
fn application_session_authenticates_and_invalidates_without_a_screen(cx: &mut TestAppContext) {
    let mut session = SessionCoordinator::new(
        HttpTransport::with_adapter(Arc::new(Login)),
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
        Config::default(),
        None,
    );
    let updates = session.updates();
    session.submit("Ada".into(), "password".into(), false);
    assert!(session.pending());
    assert!(session.active().is_none());
    cx.executor().run_until_parked();
    session.apply(updates.try_recv().unwrap());
    assert!(session.session_generation().is_some());
    assert_eq!(session.active().unwrap().user.username, "Ada");
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::Authenticated));
    session.logout();
    assert!(session.active().is_none());
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::Invalidated));
}

struct Call {
    request: Request,
    reply: async_channel::Sender<Result<Response, ApiError>>,
}
struct Gated(async_channel::Sender<Call>);
impl RequestAdapter for Gated {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        let (reply, result) = async_channel::bounded(1);
        self.0.try_send(Call { request, reply }).unwrap();
        Box::pin(async move { result.recv().await.unwrap_or(Err(ApiError::Unavailable)) })
    }
}
fn controlled(cx: &TestAppContext) -> (SessionCoordinator, async_channel::Receiver<Call>) {
    let (calls, requests) = async_channel::unbounded();
    (
        SessionCoordinator::new(
            crate::test_support::live::transport(Arc::new(Gated(calls))),
            Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
            Config::default(),
            None,
        ),
        requests,
    )
}
fn auth(call: Call, name: &str, expiry: i64) {
    let status = if call.request.url().path().ends_with("signup") {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    let body = serde_json::json!({
        "user": {"id": name, "username": name}, "access_token": format!("synthetic-{name}"),
        "expires_at": chrono::DateTime::from_timestamp(expiry, 0).unwrap().to_rfc3339(),
    });
    call.reply
        .try_send(Ok(Response::controlled(status, body.to_string())))
        .unwrap();
}
fn deliver(cx: &mut TestAppContext, session: &mut SessionCoordinator) {
    cx.executor().run_until_parked();
    let updates = session.updates();
    while let Ok(update) = updates.try_recv() {
        session.apply(update);
    }
}
fn accept(
    cx: &mut TestAppContext,
    session: &mut SessionCoordinator,
    calls: &async_channel::Receiver<Call>,
    name: &str,
    expiry: i64,
) -> u64 {
    session.submit(name.into(), "synthetic password".into(), false);
    cx.executor().run_until_parked();
    auth(calls.try_recv().unwrap(), name, expiry);
    deliver(cx, session);
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::Authenticated));
    session.session_generation().unwrap()
}

#[gpui_kit::test]
fn obsolete_authentication_and_rejection_cannot_replace_a_new_session(cx: &mut TestAppContext) {
    let (mut session, calls) = controlled(cx);
    session.submit("Old".into(), "old password".into(), true);
    session.submit("Duplicate".into(), "duplicate password".into(), true);
    cx.executor().run_until_parked();
    let old = calls.try_recv().unwrap();
    assert!(calls.try_recv().is_err(), "pending submission is inert");
    session.change_server("https://new.example".into());
    assert!(!session.pending());
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::ServerChanged));
    let generation = accept(cx, &mut session, &calls, "New", 1_800_000_100);
    auth(old, "Old", 1_800_000_100);
    deliver(cx, &mut session);
    assert_eq!(session.active().unwrap().user.username, "New");
    assert_eq!(session.active().unwrap().server, "https://new.example");
    assert!(session.take_lifecycle().is_none());
    session.protected_rejected(generation.wrapping_sub(1));
    assert!(session.active().is_some());
    assert!(session.take_lifecycle().is_none());
    session.protected_rejected(generation);
    assert!(session.active().is_none());
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::Invalidated));
    assert!(session.feedback().unwrap().contains("rejected"));
}

#[gpui_kit::test]
fn queued_old_expiry_is_inert_but_current_expiry_blocks_protected_dispatch(
    cx: &mut TestAppContext,
) {
    let (mut session, calls) = controlled(cx);
    let old = accept(cx, &mut session, &calls, "Old", 1_800_000_003);
    cx.background_executor.advance_clock(Duration::from_secs(3));
    cx.executor().run_until_parked();
    // An already-delivered timer can survive cancellation; identity still gates it.
    let expiry = session.updates().try_recv().unwrap();
    session.logout();
    assert!(session.active().is_none());
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::Invalidated));
    cx.executor().run_until_parked();
    let revoke = calls.try_recv().unwrap();
    assert_eq!(
        revoke.request.headers()["authorization"],
        "Bearer synthetic-Old"
    );
    let new = accept(cx, &mut session, &calls, "New", 1_800_000_006);
    assert_ne!(new, old);
    session.apply(expiry);
    assert_eq!(session.session_generation(), Some(new));
    assert!(session.active().is_some());
    assert!(session.take_lifecycle().is_none());
    revoke.reply.try_send(Err(ApiError::Unavailable)).unwrap();
    deliver(cx, &mut session);
    assert!(
        session.feedback().is_none(),
        "old cleanup cannot change new feedback"
    );
    cx.background_executor.advance_clock(Duration::from_secs(3));
    deliver(cx, &mut session);
    assert!(session.active().is_none());
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::Invalidated));
    assert!(
        calls.try_recv().is_err(),
        "expiry does not start new protected work"
    );
}

#[gpui_kit::test]
fn form_and_restore_intentions_cannot_orphan_an_active_expiry(cx: &mut TestAppContext) {
    let (mut session, calls) = controlled(cx);
    let generation = accept(cx, &mut session, &calls, "Ada", 1_800_000_003);
    session.cancel_pending();
    assert_eq!(session.session_generation(), Some(generation));
    session.retry_storage();
    assert!(!session.restore_pending());
    cx.background_executor.advance_clock(Duration::from_secs(3));
    deliver(cx, &mut session);
    assert!(session.active().is_none());
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::Invalidated));
}

#[gpui_kit::test]
fn restored_context_uses_the_same_lifetime_and_server_change_invalidates_it(
    cx: &mut TestAppContext,
) {
    use crate::test_support::storage::{Controlled, Shared};
    use std::sync::{Condvar, Mutex};
    cx.background_executor.allow_parking();
    let selection = crate::storage::Selection {
        server: DEFAULT_SERVER_URL.into(),
        user: User {
            id: "Ada".into(),
            username: "Ada".into(),
        },
        expires_at: 1_800_000_003,
    };
    let config = Config {
        server: Some(selection.server.clone()),
        saved: Some(selection.clone()),
        pending_deletions: vec![],
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let shared: Shared = Arc::new((
        Mutex::new((
            vec![(
                crate::storage::account(&selection),
                "synthetic-saved".into(),
            )],
            false,
            false,
            false,
            false,
            false,
        )),
        Condvar::new(),
    ));
    let (requests, calls) = async_channel::unbounded();
    let mut session = SessionCoordinator::new(
        HttpTransport::with_adapter(Arc::new(Gated(requests))),
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
        config,
        Some(Persistence::start(Controlled(shared), Some(path))),
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let verification = loop {
        cx.executor().run_until_parked();
        if let Ok(call) = calls.try_recv() {
            break call;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    };
    assert_eq!(verification.request.url().path(), "/api/v1/me");
    assert!(session.active().is_none());
    verification
        .reply
        .try_send(Ok(Response::controlled(
            StatusCode::OK,
            r#"{"id":"Ada","username":"Ada"}"#,
        )))
        .unwrap();
    deliver(cx, &mut session);
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::Authenticated));
    let generation = session.session_generation().unwrap();
    assert!(session.chat().is_some());
    session.change_server("https://other.example".into());
    assert!(session.active().is_none());
    assert!(session.chat().is_none());
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::ServerChanged));
    let current = accept(cx, &mut session, &calls, "New", 1_800_000_100);
    assert_ne!(generation, current);
    cx.background_executor.advance_clock(Duration::from_secs(3));
    deliver(cx, &mut session);
    assert!(
        session.session_generation() == Some(current),
        "old restored expiry is inert"
    );
    assert!(session.take_lifecycle().is_none());
}

#[gpui_kit::test]
fn logout_is_local_before_revocation_and_retains_distinct_remote_outcomes(cx: &mut TestAppContext) {
    for (result, warning) in [
        (Ok(Response::controlled(StatusCode::NO_CONTENT, "")), None),
        (
            Ok(Response::controlled(StatusCode::UNAUTHORIZED, "{}")),
            Some("already invalid (401)"),
        ),
        (Err(ApiError::Unavailable), Some("could not be confirmed")),
    ] {
        let (mut session, calls) = controlled(cx);
        accept(cx, &mut session, &calls, "Ada", 1_800_000_100);
        session.logout();
        assert!(session.active().is_none());
        assert!(session.chat().is_none());
        assert_eq!(session.take_lifecycle(), Some(Lifecycle::Invalidated));
        cx.executor().run_until_parked();
        let revoke = calls.try_recv().unwrap();
        assert_eq!(revoke.request.url().path(), "/api/v1/auth/logout");
        assert_eq!(
            revoke.request.url().origin().ascii_serialization(),
            DEFAULT_SERVER_URL
        );
        assert_eq!(
            revoke.request.headers()["authorization"],
            "Bearer synthetic-Ada"
        );
        assert!(session.feedback().is_none());
        revoke.reply.try_send(result).unwrap();
        deliver(cx, &mut session);
        match warning {
            Some(warning) => assert!(session.feedback().unwrap().contains(warning)),
            None => assert!(session.feedback().is_none()),
        }
        assert!(session.active().is_none());
        assert!(session.take_lifecycle().is_none());
    }
}
