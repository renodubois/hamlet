use super::bound_auth::fixture_api as bound_api;
use super::bound_auth::*;
use super::*;

// Exercises the actual signup button -> Hamlet::submit -> enter_authenticated ->
// save_login chain. The worker is real, but its Store is controlled, not Secret Service.
struct PersistentSignupAuth {
    verified: Arc<AtomicUsize>,
    revoked: Arc<AtomicBool>,
}
impl RequestAdapter for PersistentSignupAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, AuthError>> {
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

#[gpui_kit::test]
fn signup_controls_save_and_restore_through_view_and_controlled_worker(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.background_executor.allow_parking(); // real dedicated storage worker
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
    let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
    let target = saved.clone();
    let first_store = shared.clone();
    let first_path = path.clone();
    let first_api = api.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Hamlet::new(window, cx, first_api);
            view.persistence = Some(Persistence::start(
                Controlled(first_store),
                Some(first_path),
            ));
            view
        });
        *target.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let first: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("auth-mode", cx);
        window.click("username", cx);
        window.input("Alice_1", cx);
        window.click("password", cx);
        window.input("long password", cx);
        window.click("signup", cx);
    });
    // The signup future and storage worker run on foreign threads. Pump GPUI until
    // both callbacks have been delivered, without a timing-dependent sleep.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        let saved = cx.update(|_, cx| {
            let view = first.read(cx);
            view.storage_feedback.as_deref()
                == Some("Login saved in Secret Service for this server and user.")
        });
        if saved {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "signup/storage did not finish"
        );
        std::thread::yield_now();
    }
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
        let view = first.read(cx);
        assert!(view.session.password.is_empty());
        assert_eq!(view.password.read(cx).text().len(), 0);
    });
    let config = crate::persistence::load_at(Some(&path));
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
            crate::persistence::account(&selection),
            "controlled-signup-token".into()
        )]
    );

    // Simulate a fresh view/worker using only the saved metadata and the shared
    // controlled credential backend. start_restore must read and verify identity.
    let restored = std::rc::Rc::new(std::cell::RefCell::new(None));
    let target = restored.clone();
    let second_store = shared.clone();
    let second_path = path.clone();
    let second_api = api.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = Hamlet::new(window, cx, second_api);
            view.persistence = Some(Persistence::start(
                Controlled(second_store),
                Some(second_path),
            ));
            view.credential = config.saved;
            view
        });
        *target.borrow_mut() = Some(view.clone());
        view.update(cx, |view, cx| view.start_restore(cx));
        Root::new(view, window, cx)
    });
    let restarted: gpui_kit::Entity<Hamlet> = restored.borrow().as_ref().unwrap().clone();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        let ready = cx.update(|_, cx| restarted.read(cx).session.active.is_some());
        if ready {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "restoration did not finish"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        verified.load(Ordering::SeqCst),
        1,
        "restore must check /me identity"
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
        assert_eq!(
            restarted.read(cx).session.active.as_ref().unwrap().user,
            selection.user
        );
        window.click("logout", cx);
        window.render_frame(cx);
        assert!(restarted.read(cx).session.active.is_none());
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if crate::persistence::load_at(Some(&path)).saved.is_none()
            && shared.0.lock().unwrap().0.is_empty()
            && revoked.load(Ordering::SeqCst)
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "logout cleanup did not finish"
        );
        std::thread::yield_now();
    }
    assert!(restarted.read_with(cx, |view, _| view.deletions.is_empty()));
}

#[gpui_kit::test]
fn headless_logout_dispatches_deletion_warns_and_retries(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.background_executor.allow_parking(); // real dedicated storage worker
    let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
    let target = saved.clone();
    let shared: Shared = Arc::new((
        Mutex::new((vec![], false, true, false, false, false)),
        Condvar::new(),
    ));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, bound_api(BoundAuth)));
        view.update(cx, |view, _| {
            view.persistence = Some(Persistence::start(
                Controlled(shared.clone()),
                Some(path.clone()),
            ));
        });
        *target.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.session.begin_restore();
            cx.notify();
        });
        window.render_frame(cx);
        assert!(
            window
                .find("auth-feedback")
                .label()
                .unwrap()
                .contains("Checking saved session")
        );
        assert!(window.find("login").label().unwrap().contains("Signing in"));
        view.update(cx, |view, cx| {
            view.session.cancel_pending();
            cx.notify();
        });
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("storage-status")
                .label()
                .unwrap()
                .contains("memory-only")
        );
        // Force an actual failed Secret Service deletion, then retry via the button.
        let selection = view.read(cx).credential.clone().unwrap();
        shared.0.lock().unwrap().2 = false;
        shared.0.lock().unwrap().3 = true;
        assert_eq!(
            view.read(cx)
                .persistence
                .as_ref()
                .unwrap()
                .save(selection, "token".into())
                .recv_blocking()
                .unwrap(),
            super::Outcome::Saved
        );
        shared.0.lock().unwrap().5 = false;
        window.click("logout", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            shared.0.lock().unwrap().5,
            "logout must dispatch deletion without retry"
        );
        assert_eq!(
            serde_json::from_slice::<crate::persistence::Config>(&std::fs::read(&path).unwrap())
                .unwrap()
                .pending_deletions
                .len(),
            1
        );
        assert!(
            window
                .find("storage-status")
                .label()
                .unwrap()
                .contains("saved login may remain")
        );
        assert!(
            window
                .find("retry-storage")
                .label()
                .unwrap()
                .contains("deletion")
        );
        shared.0.lock().unwrap().3 = false;
        window.click("retry-storage", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("storage-status")
                .label()
                .unwrap()
                .contains("removed")
        );
        assert!(shared.0.lock().unwrap().0.is_empty());
    });
}
