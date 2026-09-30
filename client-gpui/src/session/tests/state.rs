use super::*;
use crate::api::HttpTransport;
fn app() -> AppSession {
    AppSession::new()
}
fn login(name: &str) -> Authentication {
    Authentication {
        user: User {
            id: "42".into(),
            username: name.into(),
        },
        client: HttpTransport::new()
            .server(DEFAULT_SERVER_URL)
            .unwrap()
            .restore_candidate("secret".into())
            .unwrap(),
        expires_at: 100,
    }
}
#[test]
fn login_feedback_and_recoverable_inputs() {
    let mut app = app();
    assert!(app.submit("", "").is_none());
    let request = app.submit("alice", "wrong").unwrap();
    assert!(app.submit("alice", "wrong").is_none());
    app.complete_login(request, Err(ApiError::InvalidCredentials), 0);
    assert_eq!(
        app.feedback.as_deref(),
        Some("Incorrect username or password.")
    );
    let request = app.submit("alice", "corrected").unwrap();
    app.complete_login(request, Ok(login("alice")), 0);
    assert_eq!(app.active.as_ref().unwrap().user.username, "alice");
}
#[test]
fn signup_enters_session_and_preserves_inputs_on_rejection() {
    let mut app = app();
    assert!(app.submit_signup("bad!", "short").is_none());
    assert!(app.feedback.as_deref().unwrap().contains("3–32"));
    let request = app.submit_signup("Alice_1", "long password").unwrap();
    assert!(app.submit_signup("Alice_1", "long password").is_none());
    assert!(app.complete_signup(request, Err(ApiError::Conflict), 0));
    assert!(app.feedback.as_deref().unwrap().contains("already exists"));
    let request = app.submit_signup("Alice_1", "long password").unwrap();
    assert!(app.complete_signup(request, Err(ApiError::Unavailable), 0));
    assert!(
        app.feedback
            .as_deref()
            .unwrap()
            .contains("may have succeeded")
    );
    let request = app.submit_signup("Alice_1", "long password").unwrap();
    assert!(app.complete_signup(request, Ok(login("Alice_1")), 0));
    assert_eq!(app.active.as_ref().unwrap().user.username, "Alice_1");
}
#[test]
fn stale_signup_cannot_replace_new_session_or_feedback() {
    let mut app = app();
    let old = app.submit_signup("Alice", "password").unwrap();
    app.cancel_pending();
    let newer = app.submit("Alice", "password").unwrap();
    app.complete_login(newer, Ok(login("new")), 0);
    assert!(!app.complete_signup(old, Err(ApiError::Conflict), 0));
    assert_eq!(app.active.as_ref().unwrap().user.username, "new");
    assert!(app.feedback.is_none());
}
#[test]
fn stale_outcomes_cannot_replace_or_invalidate_newer_session() {
    let mut app = app();
    let old = app.submit("a", "p").unwrap();
    app.logout();
    let newer = app.submit("a", "p").unwrap();
    app.complete_login(newer, Ok(login("new")), 0);
    let current = app.session_generation().unwrap();
    app.complete_login(old, Err(ApiError::InvalidCredentials), 0);
    app.protected_rejected(current.wrapping_sub(1));
    assert_eq!(app.active.as_ref().unwrap().user.username, "new");
    app.protected_rejected(current);
    assert!(app.active.is_none());
    assert!(app.feedback.as_deref().unwrap().contains("session expired"));
}
#[test]
fn logout_clears_immediately_and_only_current_revocation_warns() {
    let mut app = app();
    let request = app.submit("a", "p").unwrap();
    app.complete_login(request, Ok(login("a")), 0);
    let revocation = app.logout().unwrap();
    assert!(app.active.is_none());
    app.revocation_result(revocation.generation, Err(ApiError::Unavailable));
    assert!(
        app.feedback
            .as_deref()
            .unwrap()
            .contains("could not be confirmed")
    );
    app.revocation_result(revocation.generation, Err(ApiError::AlreadyInvalid));
    assert!(
        app.feedback
            .as_deref()
            .unwrap()
            .contains("already invalid (401)")
    );
    let request = app.submit("a", "p").unwrap();
    app.revocation_result(revocation.generation, Err(ApiError::Unavailable));
    assert!(
        app.feedback.is_none(),
        "old revocation cannot alter a newer login"
    );
    app.complete_login(request, Ok(login("b")), 0);
    app.revocation_result(revocation.generation, Err(ApiError::Unavailable));
    assert!(app.feedback.is_none());
    app.expire(100);
    assert!(app.active.is_none());
}
#[test]
fn saved_session_requires_verification_and_distinguishes_temporary_failure_and_expiry() {
    let mut app = app();
    let server = app.server.clone();
    let first = app.begin_restore();
    assert!(app.pending);
    assert!(app.active.is_none());
    assert_eq!(
        app.finish_restore(
            first,
            &server,
            &login("Ada").user,
            100,
            RestoreResult::Unavailable,
            0
        ),
        RestoreDecision::Retry
    );
    assert!(
        app.feedback
            .as_deref()
            .unwrap()
            .contains("not been removed")
    );
    let second = app.begin_restore();
    assert_eq!(
        app.finish_restore(
            second,
            &server,
            &login("Ada").user,
            100,
            RestoreResult::Verified {
                user: login("Ada").user,
                client: login("Ada").client,
            },
            0
        ),
        RestoreDecision::Restored
    );
    assert_eq!(app.active.as_ref().unwrap().user.username, "Ada");
    app.expire(100);
    assert!(app.active.is_none());
    let third = app.begin_restore();
    assert_eq!(
        app.finish_restore(
            third,
            &server,
            &login("Ada").user,
            100,
            RestoreResult::Unavailable,
            100
        ),
        RestoreDecision::Delete
    );
    assert!(app.feedback.as_deref().unwrap().contains("expired"));
    let fourth = app.begin_restore();
    assert_eq!(
        app.finish_restore(
            fourth,
            &server,
            &login("Ada").user,
            100,
            RestoreResult::Rejected,
            0
        ),
        RestoreDecision::Delete
    );
    assert!(app.feedback.as_deref().unwrap().contains("rejected"));
    let fifth = app.begin_restore();
    assert_eq!(
        app.finish_restore(
            fifth,
            &server,
            &login("Other").user,
            100,
            RestoreResult::Verified {
                user: login("Ada").user,
                client: login("Ada").client,
            },
            0
        ),
        RestoreDecision::Delete
    );
    assert!(app.feedback.as_deref().unwrap().contains("identity"));
}
#[test]
fn late_restoration_after_logout_or_server_change_cannot_reopen_session() {
    let mut app = app();
    let server = app.server.clone();
    let pending = app.begin_restore();
    app.logout();
    assert_eq!(
        app.finish_restore(
            pending,
            &server,
            &login("Ada").user,
            100,
            RestoreResult::Unavailable,
            0
        ),
        RestoreDecision::Stale
    );
    let pending = app.begin_restore();
    app.change_server("https://elsewhere.example".into());
    assert_eq!(
        app.finish_restore(
            pending,
            &server,
            &login("Ada").user,
            100,
            RestoreResult::Unavailable,
            0
        ),
        RestoreDecision::Stale
    );
    assert!(app.active.is_none());
}
#[test]
fn changing_servers_invalidates_pending_and_active() {
    let mut app = app();
    let request = app.submit("a", "p").unwrap();
    app.change_server("https://example.org".into());
    app.complete_login(request, Ok(login("a")), 0);
    assert!(app.active.is_none());
    assert!(!app.pending);
}
