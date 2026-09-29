//! Owned session interface, shared controlled execution; no view or real provider.
use super::*;
use crate::api::test_support::{RequestAdapter, Response};
use crate::api::{ApiFuture, HttpTransport};
use crate::runtime::Execution;
use gpui_kit::TestAppContext;
use reqwest::{Request, StatusCode};
use std::{sync::Arc, time::Duration};

struct Login;
impl RequestAdapter for Login {
    fn execute(&self, _: Request) -> ApiFuture<Result<Response, AuthError>> {
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
        DEFAULT_SERVER_URL.into(),
    );
    let updates = session.updates();
    session.submit("Ada".into(), "password".into(), false);
    assert!(session.pending());
    assert!(session.active().is_none());
    cx.executor().run_until_parked();
    session.apply(updates.try_recv().unwrap());
    let generation = session.session_generation().unwrap();
    assert_eq!(session.active().unwrap().user.username, "Ada");
    assert_eq!(
        session.take_lifecycle(),
        Some(Lifecycle::Authenticated { save: true })
    );
    session.logout();
    assert!(session.client_for(generation).is_none());
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::Invalidated));
}

struct Call {
    request: Request,
    reply: async_channel::Sender<Result<Response, AuthError>>,
}
struct Gated(async_channel::Sender<Call>);
impl RequestAdapter for Gated {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, AuthError>> {
        let (reply, result) = async_channel::bounded(1);
        self.0.try_send(Call { request, reply }).unwrap();
        Box::pin(async move { result.recv().await.unwrap_or(Err(AuthError::Unavailable)) })
    }
}
fn controlled(cx: &TestAppContext) -> (SessionCoordinator, async_channel::Receiver<Call>) {
    let (calls, requests) = async_channel::unbounded();
    (
        SessionCoordinator::new(
            HttpTransport::with_adapter(Arc::new(Gated(calls))),
            Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
            DEFAULT_SERVER_URL.into(),
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
    assert_eq!(
        session.take_lifecycle(),
        Some(Lifecycle::Authenticated { save: true })
    );
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
    assert!(session.client_for(generation).is_some());
    assert!(session.take_lifecycle().is_none());
    session.protected_rejected(generation);
    assert!(session.client_for(generation).is_none());
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
    assert!(session.client_for(old).is_none());
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::Invalidated));
    cx.executor().run_until_parked();
    let revoke = calls.try_recv().unwrap();
    assert_eq!(
        revoke.request.headers()["authorization"],
        "Bearer synthetic-Old"
    );
    let new = accept(cx, &mut session, &calls, "New", 1_800_000_006);
    session.apply(expiry);
    assert!(session.client_for(new).is_some());
    assert!(session.take_lifecycle().is_none());
    revoke.reply.try_send(Err(AuthError::Unavailable)).unwrap();
    deliver(cx, &mut session);
    assert!(
        session.feedback().is_none(),
        "old cleanup cannot change new feedback"
    );
    cx.background_executor.advance_clock(Duration::from_secs(3));
    deliver(cx, &mut session);
    assert!(session.client_for(new).is_none());
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
    assert!(session.begin_restore().is_none());
    cx.background_executor.advance_clock(Duration::from_secs(3));
    deliver(cx, &mut session);
    assert!(session.client_for(generation).is_none());
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::Invalidated));
}

#[gpui_kit::test]
fn restored_context_uses_the_same_lifetime_and_server_change_invalidates_it(
    cx: &mut TestAppContext,
) {
    let (mut session, calls) = controlled(cx);
    let selection = crate::storage::Selection {
        server: DEFAULT_SERVER_URL.into(),
        user: User {
            id: "Ada".into(),
            username: "Ada".into(),
        },
        expires_at: 1_800_000_003,
    };
    let generation = session.begin_restore().unwrap();
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let verified = execution.spawn(SessionCoordinator::verify_saved(
        session.restore_server(&selection.server).unwrap(),
        "synthetic-saved".into(),
    ));
    cx.executor().run_until_parked();
    let verification = calls.try_recv().unwrap();
    assert_eq!(verification.request.url().path(), "/api/v1/me");
    assert!(session.client_for(generation).is_none());
    verification
        .reply
        .try_send(Ok(Response::controlled(
            StatusCode::OK,
            r#"{"id":"Ada","username":"Ada"}"#,
        )))
        .unwrap();
    cx.executor().run_until_parked();
    assert_eq!(
        session.finish_restore(generation, &selection, verified.try_recv().unwrap()),
        RestoreDecision::Restored
    );
    assert_eq!(
        session.take_lifecycle(),
        Some(Lifecycle::Authenticated { save: false })
    );
    assert!(session.client_for(generation).is_some());
    session.change_server("https://other.example".into());
    assert!(session.active().is_none());
    assert!(session.client_for(generation).is_none());
    assert_eq!(session.take_lifecycle(), Some(Lifecycle::ServerChanged));
    let current = accept(cx, &mut session, &calls, "New", 1_800_000_100);
    cx.background_executor.advance_clock(Duration::from_secs(3));
    deliver(cx, &mut session);
    assert!(
        session.client_for(current).is_some(),
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
        (Err(AuthError::Unavailable), Some("could not be confirmed")),
    ] {
        let (mut session, calls) = controlled(cx);
        let generation = accept(cx, &mut session, &calls, "Ada", 1_800_000_100);
        session.logout();
        assert!(session.active().is_none());
        assert!(session.client_for(generation).is_none());
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
