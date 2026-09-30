use super::bound_auth::*;
use super::*;
use crate::runtime::Execution;
use crate::storage::{Config, Persistence};
use crate::views::app_shell::open;

// Real signup/save/restart/verified restore/logout, not private shell setup.
struct PersistentSignupAuth {
    verified: Arc<AtomicUsize>,
    revoked: Arc<AtomicBool>,
}
impl RequestAdapter for PersistentSignupAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        assert_eq!(
            request.url().origin().ascii_serialization(),
            crate::session::DEFAULT_SERVER_URL
        );
        let response = match request.url().path() {
            "/api/v1/auth/signup" => {
                let body: serde_json::Value =
                    serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
                assert_eq!(body["username"], "Alice_1");
                assert_eq!(body["password"], "long password");
                Response::controlled(
                    StatusCode::CREATED,
                    r#"{"user":{"id":"42","username":"Alice_1"},"access_token":"controlled-signup-token","expires_at":"2099-01-01T00:00:00Z"}"#,
                )
            }
            "/api/v1/auth/login" => panic!("signup must not call login"),
            "/api/v1/me" => {
                self.verified.fetch_add(1, Ordering::SeqCst);
                assert_eq!(
                    request.headers()["authorization"],
                    "Bearer controlled-signup-token"
                );
                if self.revoked.load(Ordering::SeqCst) {
                    Response::controlled(StatusCode::UNAUTHORIZED, "{}")
                } else {
                    Response::controlled(StatusCode::OK, r#"{"id":"42","username":"Alice_1"}"#)
                }
            }
            "/api/v1/auth/logout" => {
                assert_eq!(
                    request.headers()["authorization"],
                    "Bearer controlled-signup-token"
                );
                self.revoked.store(true, Ordering::SeqCst);
                Response::controlled(StatusCode::NO_CONTENT, "")
            }
            _ => return BoundAuth.execute(request),
        };
        Box::pin(async move { Ok(response) })
    }
}

pub(super) fn wait_status(cx: &mut gpui_kit::VisualTestContext, expected: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if cx.update(|window, cx| {
            window.render_frame(cx);
            window
                .try_find("storage-status")
                .and_then(|node| node.label().map(|label| label.contains(expected)))
                .unwrap_or(false)
        }) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "missing storage status: {expected}"
        );
        std::thread::yield_now();
    }
}

#[gpui_kit::test]
fn signup_controls_save_and_restore_through_view_and_controlled_worker(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let shared: Shared = Arc::new((
        Mutex::new((vec![], false, false, false, false, false)),
        Condvar::new(),
    ));
    let verified = Arc::new(AtomicUsize::new(0));
    let revoked = Arc::new(AtomicBool::new(false));
    let api = bound_api(PersistentSignupAuth {
        verified: verified.clone(),
        revoked: revoked.clone(),
    });
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let first_execution = execution.clone();
    let first_api = api.clone();
    let first_store = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open(
            window,
            cx,
            first_api,
            Config::default(),
            Some(first_store),
            first_execution,
        );
        Root::new(view, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("auth-mode", cx);
        window.click("username", cx);
        window.input("Alice_1", cx);
        window.click("password", cx);
        window.input("long password", cx);
        window.click("signup", cx);
    });
    wait_status(cx, "Login saved in");
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("session-status")
                .label()
                .unwrap()
                .contains("Alice_1")
        );
        assert!(window.find("message-000000000000001").label().is_some());
        assert!(window.try_find("password").is_none());
    });
    let config = crate::storage::load_at(Some(&path));
    let selection = config.saved.clone().expect("signup stored selection");
    assert_eq!(
        config.server.as_deref(),
        Some(crate::session::DEFAULT_SERVER_URL)
    );
    assert_eq!(selection.user.username, "Alice_1");
    let json = std::fs::read_to_string(&path).unwrap();
    assert!(!json.contains("long password") && !json.contains("controlled-signup-token"));
    assert_eq!(
        shared.0.lock().unwrap().0.as_slice(),
        &[(
            crate::storage::account(&selection),
            "controlled-signup-token".into()
        )]
    );

    // Fresh session/worker from durable metadata, using the same public startup path.
    let second_store = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open(window, cx, api, config, Some(second_store), execution);
        Root::new(view, window, cx)
    });
    wait_status(cx, "Login restored");
    assert_eq!(
        verified.load(Ordering::SeqCst),
        1,
        "restore must verify /me"
    );
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("session-status")
                .label()
                .unwrap()
                .contains("Alice_1")
        );
        window.click("logout", cx);
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Log in"));
        assert!(window.find("password").value().is_none_or(str::is_empty));
    });
    wait_status(cx, "Saved login removed");
    assert!(crate::storage::load_at(Some(&path)).saved.is_none());
    assert!(shared.0.lock().unwrap().0.is_empty());
    assert!(revoked.load(Ordering::SeqCst));
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("retry-storage").is_none());
    });
}

#[gpui_kit::test]
fn old_deletion_retry_remains_available_in_new_workspace_without_deleting_new_login(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    cx.background_executor.allow_parking();
    let shared: Shared = Arc::new((
        Mutex::new((vec![], false, false, false, false, false)),
        Condvar::new(),
    ));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let store = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open(
            window,
            cx,
            bound_api(BoundAuth),
            Config::default(),
            Some(store),
            execution,
        );
        Root::new(view, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    wait_status(cx, "Login saved in");
    shared.0.lock().unwrap().3 = true;
    cx.update(|window, cx| {
        window.click("logout", cx);
    });
    wait_status(cx, "Could not confirm deletion");
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("retry-storage").label(),
            Some("Retry saved-login deletion")
        );
        window.click("server-url", cx);
        window.press("ctrl-a", cx);
        window.input("http://127.0.0.1:8082", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.click("username", cx);
        window.press("ctrl-a", cx);
        window.input("Bob", cx);
        window.click("password", cx);
        window.input("new password", cx);
        window.click("login", cx);
    });
    wait_status(cx, "Login saved in");
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("storage-status")
                .label()
                .unwrap()
                .contains("Could not confirm deletion")
        );
    });
    let new = crate::storage::load_at(Some(&path)).saved.unwrap();
    assert_eq!(new.user.username, "Bob");
    assert_eq!(shared.0.lock().unwrap().0.len(), 2);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("password").is_none());
        assert_eq!(
            window.find("retry-storage").label(),
            Some("Retry saved-login deletion")
        );
        // Failure feedback remains observable even after the login controls leave the screen.
        window.click("retry-storage", cx);
    });
    wait_status(cx, "Could not confirm deletion");
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("session-status")
                .label()
                .unwrap()
                .contains("Bob")
        );
        shared.0.lock().unwrap().3 = false;
        window.click("retry-storage", cx);
    });
    wait_status(cx, "current session is unchanged");
    assert_eq!(
        crate::storage::load_at(Some(&path)).saved,
        Some(new.clone())
    );
    assert_eq!(
        shared.0.lock().unwrap().0.as_slice(),
        &[(crate::storage::account(&new), "secret".into())]
    );
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("session-status")
                .label()
                .unwrap()
                .contains("Bob")
        );
        assert!(window.try_find("retry-storage").is_none());
    });
}

#[gpui_kit::test]
fn headless_logout_dispatches_deletion_warns_and_retries(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.background_executor.allow_parking();
    let shared: Shared = Arc::new((
        Mutex::new((vec![], false, true, false, false, false)),
        Condvar::new(),
    ));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let store = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open(
            window,
            cx,
            bound_api(BoundAuth),
            Config::default(),
            Some(store),
            execution,
        );
        Root::new(view, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    wait_status(cx, "memory-only");
    // A failed save still requires explicit cleanup, never a false confirmation.
    shared.0.lock().unwrap().2 = false;
    shared.0.lock().unwrap().3 = true;
    shared.0.lock().unwrap().5 = false;
    cx.update(|window, cx| {
        window.click("logout", cx);
    });
    wait_status(cx, "saved login may remain");
    assert!(
        shared.0.lock().unwrap().5,
        "logout dispatches deletion without retry"
    );
    assert_eq!(
        crate::storage::load_at(Some(&path)).pending_deletions.len(),
        1
    );
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("retry-storage").label(),
            Some("Retry saved-login deletion")
        );
        shared.0.lock().unwrap().3 = false;
        window.click("retry-storage", cx);
    });
    wait_status(cx, "Saved login removed");
    assert!(shared.0.lock().unwrap().0.is_empty());
    assert!(
        crate::storage::load_at(Some(&path))
            .pending_deletions
            .is_empty()
    );
}
