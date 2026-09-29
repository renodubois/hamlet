use super::bound_auth::fixture_api as bound_api;
use super::bound_auth::*;
use super::*;

fn open_auth(
    window: &mut Window,
    cx: &mut gpui_kit::App,
    api: Arc<dyn RequestAdapter>,
) -> gpui_kit::Entity<Hamlet> {
    crate::views::app_shell::open(
        window,
        cx,
        HttpTransport::with_adapter(api),
        crate::storage::Config::default(),
        None,
        crate::runtime::Execution::controlled(cx.background_executor().clone(), 1_800_000_000),
    )
}

struct PendingAuth(Arc<AtomicUsize>);
impl RequestAdapter for PendingAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, AuthError>> {
        assert!(request.url().path().starts_with("/api/v1/auth/"));
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::pending())
    }
}

#[gpui_kit::test]
fn login_validation_and_pending_button_are_visible_and_inert(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let calls = Arc::new(AtomicUsize::new(0));
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_auth(window, cx, bound_api(PendingAuth(calls.clone())));
        Root::new(view, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Log in"));
        window.click("login", cx);
        window.render_frame(cx);
        assert!(
            window
                .find("auth-feedback")
                .label()
                .unwrap()
                .contains("username and password")
        );
        assert_eq!(window.find("login").label(), Some("Log in"));
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Signing in…"));
        window.click("login", cx);
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Signing in…"));
    });
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

struct SignupReject {
    error: AuthError,
    submissions: Arc<std::sync::Mutex<Vec<(String, String)>>>,
}
impl RequestAdapter for SignupReject {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, AuthError>> {
        assert_eq!(request.url().path(), "/api/v1/auth/signup");
        let body: serde_json::Value =
            serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
        self.submissions.lock().unwrap().push((
            body["username"].as_str().unwrap().into(),
            body["password"].as_str().unwrap().into(),
        ));
        let error = self.error.clone();
        Box::pin(async move {
            match error {
                AuthError::Conflict => Ok(Response::controlled(
                    StatusCode::CONFLICT,
                    r#"{"error":{"code":"conflict"}}"#,
                )),
                error => Err(error),
            }
        })
    }
}

#[gpui_kit::test]
fn signup_rejection_keeps_form_and_reports_uncertainty(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for (error, expected) in [
        (AuthError::Conflict, "already exists"),
        (AuthError::Unavailable, "may have succeeded"),
    ] {
        let submissions = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = submissions.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = open_auth(
                window,
                cx,
                bound_api(SignupReject {
                    error,
                    submissions: captured,
                }),
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
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find("auth-feedback")
                    .label()
                    .unwrap()
                    .contains(expected)
            );
            assert_eq!(window.find("username").value(), Some("Alice_1"));
            assert_eq!(window.find("signup").label(), Some("Create user"));
            // A deliberate correction submits through the same controls without retyping
            // the password; the outward request proves recoverable input survived.
            window.click("username", cx);
            window.press("ctrl-a", cx);
            window.input("Alice_2", cx);
            window.click("signup", cx);
        });
        cx.run_until_parked();
        assert_eq!(
            submissions.lock().unwrap().as_slice(),
            &[
                ("Alice_1".into(), "long password".into()),
                ("Alice_2".into(), "long password".into())
            ]
        );
    }
}

#[gpui_kit::test]
fn signup_form_supports_keyboard_focus(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_auth(window, cx, bound_api(BoundAuth));
        Root::new(view, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("auth-mode", cx);
        window.click("server-url", cx);
        assert_eq!(window.find("server-url").focused(), Some(true));
        window.press("tab", cx);
        assert_eq!(window.find("username").focused(), Some(true));
        window.input("Alice", cx);
        assert_eq!(window.find("username").value(), Some("Alice"));
    });
}

#[gpui_kit::test]
fn signup_controls_validate_and_enter_conversation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_auth(window, cx, bound_api(BoundAuth));
        Root::new(view, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("auth-mode", cx);
        window.render_frame(cx);
        assert_eq!(window.find("signup").label(), Some("Create user"));
        window.click("username", cx);
        window.input("bad!", cx);
        window.click("password", cx);
        window.input("short", cx);
        window.click("signup", cx);
        window.render_frame(cx);
        assert!(
            window
                .find("auth-feedback")
                .label()
                .unwrap()
                .contains("3–32")
        );
        window.click("username", cx);
        window.press("ctrl-a", cx);
        window.input("Alice_1", cx);
        window.click("signup", cx);
        window.render_frame(cx);
        assert!(
            window
                .find("auth-feedback")
                .label()
                .unwrap()
                .contains("8–256")
        );
        window.click("password", cx);
        window.press("ctrl-a", cx);
        window.input("long password", cx);
        window.click("signup", cx);
        window.render_frame(cx);
        assert_eq!(window.find("signup").label(), Some("Creating user…"));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("session-status")
                .label()
                .unwrap()
                .contains("Alice_1")
        );
        assert_eq!(
            window.find("message-000000000000001").label(),
            Some("first line\nsecond line")
        );
        window.click("logout", cx);
        window.render_frame(cx);
        assert!(window.find("password").value().is_none_or(str::is_empty));
    });
}

#[gpui_kit::test]
fn pending_signup_is_inert_and_preserves_editable_inputs(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let calls = Arc::new(AtomicUsize::new(0));
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_auth(window, cx, bound_api(PendingAuth(calls.clone())));
        Root::new(view, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("auth-mode", cx);
        window.click("username", cx);
        window.input("Alice", cx);
        window.click("password", cx);
        window.input("long password", cx);
        window.click("signup", cx);
        window.render_frame(cx);
        window.click("signup", cx);
        window.render_frame(cx);
        assert_eq!(window.find("signup").label(), Some("Creating user…"));
    });
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    cx.update(|window, cx| {
        window.click("auth-mode", cx);
        window.render_frame(cx);
        assert_eq!(window.find("signup").label(), Some("Creating user…"));
        assert_eq!(window.find("username").value(), Some("Alice"));
    });
}
