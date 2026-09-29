use super::*;

struct PendingAuth(Arc<AtomicUsize>);
impl AuthApi for PendingAuth {
    fn signup(&self, _: String, _: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::pending())
    }
    fn login(&self, _: String, _: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::pending())
    }
    fn logout(&self, _: String, _: String) -> ApiFuture<Result<(), AuthError>> {
        Box::pin(async { Ok(()) })
    }
    fn channels(
        &self,
        _: String,
        _: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Channel>, AuthError>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn create_channel(
        &self,
        _: String,
        _: String,
        _: String,
    ) -> ApiFuture<Result<crate::conversation::Channel, AuthError>> {
        Box::pin(async { unreachable!() })
    }
    fn history(
        &self,
        _: String,
        _: String,
        _: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Message>, AuthError>> {
        Box::pin(async { Ok(vec![]) })
    }
}

#[gpui_kit::test]
fn login_validation_and_pending_button_are_visible_and_inert(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let calls = Arc::new(AtomicUsize::new(0));
    let probe = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored = probe.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PendingAuth(calls.clone()))));
        *stored.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = probe.borrow().as_ref().unwrap().clone();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(!view.read(cx).login_disabled());
        window.click("login", cx);
        window.render_frame(cx);
        assert!(
            window
                .find("auth-feedback")
                .label()
                .unwrap()
                .contains("username and password")
        );
        assert!(!view.read(cx).login_disabled());
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Signing in…"));
        assert!(view.read(cx).login_disabled());
        window.click("login", cx);
        view.update(cx, |view, cx| view.submit(cx)); // guard also covers non-pointer callers
        window.render_frame(cx);
        assert!(view.read(cx).session.pending);
        assert!(view.read(cx).login_disabled());
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while calls.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

struct SignupReject {
    error: AuthError,
    submissions: Arc<std::sync::Mutex<Vec<(String, String)>>>,
}
impl AuthApi for SignupReject {
    fn signup(
        &self,
        _: String,
        username: String,
        password: String,
    ) -> ApiFuture<Result<Login, AuthError>> {
        self.submissions.lock().unwrap().push((username, password));
        let error = self.error.clone();
        Box::pin(async move { Err(error) })
    }
    fn login(&self, _: String, _: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
        Box::pin(async { unreachable!() })
    }
    fn logout(&self, _: String, _: String) -> ApiFuture<Result<(), AuthError>> {
        Box::pin(async { Ok(()) })
    }
    fn channels(
        &self,
        _: String,
        _: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Channel>, AuthError>> {
        Box::pin(async { unreachable!() })
    }
    fn create_channel(
        &self,
        _: String,
        _: String,
        _: String,
    ) -> ApiFuture<Result<crate::conversation::Channel, AuthError>> {
        Box::pin(async { unreachable!() })
    }
    fn history(
        &self,
        _: String,
        _: String,
        _: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Message>, AuthError>> {
        Box::pin(async { unreachable!() })
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
            let view = cx.new(|cx| {
                Hamlet::new(
                    window,
                    cx,
                    Arc::new(SignupReject {
                        error,
                        submissions: captured,
                    }),
                )
            });
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
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
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
    let (_, cx) = cx.add_window_view(|window, cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
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
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PendingAuth(calls.clone()))));
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
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while calls.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    cx.update(|window, cx| {
        window.click("auth-mode", cx);
        window.render_frame(cx);
        assert_eq!(window.find("signup").label(), Some("Creating user…"));
        assert_eq!(window.find("username").value(), Some("Alice"));
    });
}
