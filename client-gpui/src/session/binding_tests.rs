//! Authentication ownership through bound clients; synthetic requests and controlled completion.
use super::*;
use crate::api::{
    HttpTransport,
    legacy::HttpAuth,
    test_support::{RequestAdapter, Response},
};
use reqwest::{Request, StatusCode};
use std::sync::{Arc, Mutex};

type Reply = async_channel::Sender<Result<Response, AuthError>>;
struct Controlled(Mutex<Vec<(Request, Reply)>>);
impl RequestAdapter for Controlled {
    fn execute(&self, request: Request) -> crate::api::ApiFuture<Result<Response, AuthError>> {
        let (tx, rx) = async_channel::bounded(1);
        self.0.lock().unwrap().push((request, tx));
        Box::pin(async move { rx.recv().await.unwrap() })
    }
}

async fn respond(
    adapter: &Controlled,
    path: &str,
    bearer: Option<&str>,
    origin: &str,
    status: StatusCode,
    body: &str,
) {
    tokio::task::yield_now().await;
    let (request, reply) = adapter.0.lock().unwrap().pop().unwrap();
    assert_eq!(request.url().path(), path);
    assert_eq!(request.url().origin().ascii_serialization(), origin);
    assert_eq!(
        request
            .headers()
            .get("authorization")
            .map(|value| value.to_str().unwrap()),
        bearer
    );
    reply
        .send(Ok(Response::controlled(status, body)))
        .await
        .unwrap();
}

#[tokio::test]
async fn old_context_and_revocation_keep_their_binding_without_affecting_new_login() {
    let adapter = Arc::new(Controlled(Mutex::new(vec![])));
    let transport = HttpTransport::with_adapter(adapter.clone());
    let mut session = AppSession::new(Arc::new(HttpAuth::with_transport(transport.clone())));
    session.username = "Ada".into();
    session.password = "long password".into();
    let request = session.submit_signup().unwrap();
    let auth = tokio::spawn(
        transport
            .server(&request.server)
            .unwrap()
            .signup(request.username.clone(), request.password.clone()),
    );
    respond(&adapter, "/api/v1/auth/signup", None, DEFAULT_SERVER_URL, StatusCode::CREATED,
        r#"{"user":{"id":"42","username":"Ada"},"access_token":"old-secret","expires_at":"2099-01-01T00:00:00Z"}"#).await;
    assert!(session.complete_signup(request, auth.await.unwrap(), 0));
    let old = session.active_client().unwrap();
    let old_generation = session.session_generation().unwrap();
    let revocation = session.logout().unwrap();
    assert!(session.active_client().is_none());
    session.change_server("https://NEW.example/".into());
    session.username = "Bob".into();
    session.password = "new password".into();
    let request = session.submit().unwrap();
    let auth = tokio::spawn(
        transport
            .server(&request.server)
            .unwrap()
            .login(request.username.clone(), request.password.clone()),
    );
    respond(&adapter, "/api/v1/auth/login", None, "https://new.example", StatusCode::OK,
        r#"{"user":{"id":"43","username":"Bob"},"access_token":"new-secret","expires_at":"2099-01-01T00:00:00Z"}"#).await;
    assert!(session.complete_login(request, auth.await.unwrap(), 0));
    assert_eq!(
        session.active.as_ref().unwrap().server,
        "https://NEW.example/"
    );
    let rejected = tokio::spawn(old.current_user());
    respond(
        &adapter,
        "/api/v1/me",
        Some("Bearer old-secret"),
        DEFAULT_SERVER_URL,
        StatusCode::UNAUTHORIZED,
        "{}",
    )
    .await;
    assert_eq!(rejected.await.unwrap(), Err(AuthError::AlreadyInvalid));
    session.protected_rejected(old_generation);
    let revoke = tokio::spawn(revocation.client.logout());
    respond(
        &adapter,
        "/api/v1/auth/logout",
        Some("Bearer old-secret"),
        DEFAULT_SERVER_URL,
        StatusCode::UNAUTHORIZED,
        "{}",
    )
    .await;
    session.revocation_result(revocation.generation, revoke.await.unwrap());
    assert!(session.feedback.is_none());
    let current = tokio::spawn(session.active_client().unwrap().current_user());
    respond(
        &adapter,
        "/api/v1/me",
        Some("Bearer new-secret"),
        "https://new.example",
        StatusCode::OK,
        r#"{"id":"43","username":"Bob"}"#,
    )
    .await;
    assert_eq!(current.await.unwrap().unwrap().username, "Bob");
    assert_eq!(session.active.as_ref().unwrap().user.username, "Bob");
}

#[tokio::test]
async fn restoration_retry_remains_distinct_from_rejection_identity_expiry_and_missing_cleanup() {
    for (status, body, now, decision) in [
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "{}",
            0,
            RestoreDecision::Retry,
        ),
        (StatusCode::UNAUTHORIZED, "{}", 0, RestoreDecision::Delete),
        (
            StatusCode::OK,
            r#"{"id":"other","username":"Ada"}"#,
            0,
            RestoreDecision::Delete,
        ),
        (
            StatusCode::OK,
            r#"{"id":"42","username":"Other"}"#,
            0,
            RestoreDecision::Delete,
        ),
        (
            StatusCode::OK,
            r#"{"id":"42","username":"Ada"}"#,
            100,
            RestoreDecision::Delete,
        ),
    ] {
        let adapter = Arc::new(Controlled(Mutex::new(vec![])));
        let transport = HttpTransport::with_adapter(adapter.clone());
        let mut session = AppSession::new(Arc::new(HttpAuth::with_transport(transport.clone())));
        let expected = User {
            id: "42".into(),
            username: "Ada".into(),
        };
        let generation = session.begin_restore();
        let verified = tokio::spawn(AppSession::verify_saved(
            transport.server(DEFAULT_SERVER_URL).unwrap(),
            "saved-secret".into(),
        ));
        respond(
            &adapter,
            "/api/v1/me",
            Some("Bearer saved-secret"),
            DEFAULT_SERVER_URL,
            status,
            body,
        )
        .await;
        assert_eq!(
            session.finish_restore(
                generation,
                DEFAULT_SERVER_URL,
                &expected,
                100,
                verified.await.unwrap(),
                now
            ),
            decision
        );
        assert!(session.active_client().is_none());
        let generation = session.begin_restore();
        let missing =
            AppSession::verify_saved(transport.server(DEFAULT_SERVER_URL).unwrap(), String::new())
                .await;
        assert_eq!(
            session.finish_restore(generation, DEFAULT_SERVER_URL, &expected, 100, missing, 0),
            RestoreDecision::Delete
        );
        assert!(
            adapter.0.lock().unwrap().is_empty(),
            "missing credential must not dispatch verification"
        );
    }
}

#[tokio::test]
async fn saving_accepted_context_preserves_stored_identity_without_plaintext_metadata() {
    use crate::storage::{self, Outcome, Persistence, Selection, tests::Controlled as Store};
    let adapter = Arc::new(Controlled(Mutex::new(vec![])));
    let transport = HttpTransport::with_adapter(adapter.clone());
    let mut session = AppSession::new(Arc::new(HttpAuth::with_transport(transport.clone())));
    session.change_server("https://CHAT.example.test/".into());
    session.username = "Ada".into();
    session.password = "synthetic-password".into();
    let request = session.submit().unwrap();
    let auth = tokio::spawn(
        transport
            .server(&request.server)
            .unwrap()
            .login(request.username.clone(), request.password.clone()),
    );
    respond(&adapter, "/api/v1/auth/login", None, "https://chat.example.test", StatusCode::OK,
        r#"{"user":{"id":"42","username":"Ada"},"access_token":"saved-secret","expires_at":"2099-01-01T00:00:00Z"}"#).await;
    session.complete_login(request, auth.await.unwrap(), 0);
    let active = session.active.as_ref().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let shared = Arc::new((
        Mutex::new((vec![], false, false, false, false, false)),
        std::sync::Condvar::new(),
    ));
    let store = Persistence::start(Store(shared), Some(path.clone()));
    assert_eq!(
        store.remember(active.server.clone()).recv().await.unwrap(),
        Outcome::Remembered
    );
    assert_eq!(active.save(&store).recv().await.unwrap(), Outcome::Saved);
    let expected = Selection {
        server: "https://CHAT.example.test/".into(),
        user: User {
            id: "42".into(),
            username: "Ada".into(),
        },
        expires_at: 4_070_908_800,
    };
    assert_eq!(storage::load_at(Some(&path)).saved, Some(expected.clone()));
    assert_eq!(
        storage::account(&expected),
        "26:https://CHAT.example.test/:42"
    );
    let metadata = std::fs::read_to_string(path).unwrap();
    assert!(!metadata.contains("saved-secret"));
    assert!(!metadata.contains("synthetic-password"));
    let Outcome::Token(Some(token)) = store.read(expected.clone()).recv().await.unwrap() else {
        panic!("credential not saved under original identity")
    };
    let mut restarted = AppSession::new(session.api.clone());
    restarted.change_server(expected.server.clone());
    let generation = restarted.begin_restore();
    let verified = tokio::spawn(AppSession::verify_saved(
        transport.server(&expected.server).unwrap(),
        token,
    ));
    respond(
        &adapter,
        "/api/v1/me",
        Some("Bearer saved-secret"),
        "https://chat.example.test",
        StatusCode::OK,
        r#"{"id":"42","username":"Ada"}"#,
    )
    .await;
    assert_eq!(
        restarted.finish_restore(
            generation,
            &expected.server,
            &expected.user,
            expected.expires_at,
            verified.await.unwrap(),
            0
        ),
        RestoreDecision::Restored
    );
    assert!(restarted.active_client().is_some());
}

#[tokio::test]
async fn candidate_stays_private_until_verified_and_late_verification_cannot_activate() {
    let adapter = Arc::new(Controlled(Mutex::new(vec![])));
    let transport = HttpTransport::with_adapter(adapter.clone());
    let mut session = AppSession::new(Arc::new(HttpAuth::with_transport(transport.clone())));
    let server = session.server.clone();
    let expected = User {
        id: "42".into(),
        username: "Ada".into(),
    };
    let generation = session.begin_restore();
    let verification = tokio::spawn(AppSession::verify_saved(
        transport.server(&server).unwrap(),
        "saved-secret".into(),
    ));
    tokio::task::yield_now().await;
    assert!(
        session.active_client().is_none(),
        "binding is not activation"
    );
    let (request, reply) = adapter.0.lock().unwrap().pop().unwrap();
    assert_eq!(request.url().path(), "/api/v1/me");
    assert_eq!(request.headers()["authorization"], "Bearer saved-secret");
    session.logout();
    reply
        .send(Ok(Response::controlled(
            StatusCode::OK,
            r#"{"id":"42","username":"Ada"}"#,
        )))
        .await
        .unwrap();
    assert_eq!(
        session.finish_restore(
            generation,
            &server,
            &expected,
            100,
            verification.await.unwrap(),
            0
        ),
        RestoreDecision::Stale
    );
    assert!(
        session.active_client().is_none(),
        "late verification cannot reopen access"
    );
}
