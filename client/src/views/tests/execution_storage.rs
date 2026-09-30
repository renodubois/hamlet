//! Controlled provider work stays on the real serialized worker, never the UI thread.
use super::*;
use crate::storage::{Config, Selection, Store};

type ProviderReply = async_channel::Sender<Result<Option<String>, ()>>;
enum ProviderCall {
    Read(ProviderReply),
    Write(ProviderReply),
    Delete(ProviderReply),
}
struct GatedStore(std::sync::mpsc::Sender<ProviderCall>);
impl GatedStore {
    fn call(&self, wrap: impl FnOnce(ProviderReply) -> ProviderCall) -> Result<Option<String>, ()> {
        let (tx, rx) = async_channel::bounded(1);
        self.0.send(wrap(tx)).map_err(|_| ())?;
        rx.recv_blocking().unwrap_or(Err(()))
    }
}
impl Store for GatedStore {
    fn get(&mut self, _: &Selection) -> Result<Option<String>, ()> {
        self.call(ProviderCall::Read)
    }
    fn put(&mut self, _: &Selection, _: &str) -> Result<(), ()> {
        self.call(ProviderCall::Write).map(|_| ())
    }
    fn delete(&mut self, _: &Selection) -> Result<(), ()> {
        self.call(ProviderCall::Delete).map(|_| ())
    }
}

struct VerifyAuth(std::sync::mpsc::Sender<async_channel::Sender<Result<User, ApiError>>>);
impl RequestAdapter for VerifyAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.url().path() != "/api/v1/me" {
            return BoundAuth.execute(request);
        }
        let (tx, rx) = async_channel::bounded(1);
        self.0.send(tx).unwrap();
        Box::pin(async move {
            rx.recv().await.unwrap().map(|user| {
                Response::controlled(
                    StatusCode::OK,
                    serde_json::json!({"id":user.id,"username":user.username}).to_string(),
                )
            })
        })
    }
}

// Only the dedicated provider thread uses wall time. Pump delivery until its observable
// API request arrives; all application deadlines remain on the controlled clock.
fn await_verification(
    cx: &mut gpui_kit::VisualTestContext,
    requests: &std::sync::mpsc::Receiver<async_channel::Sender<Result<User, ApiError>>>,
) -> async_channel::Sender<Result<User, ApiError>> {
    let guard = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if let Ok(reply) = requests.try_recv() {
            return reply;
        }
        assert!(
            std::time::Instant::now() < guard,
            "worker reply not delivered"
        );
        std::thread::yield_now();
    }
}

#[gpui_kit::test]
fn restoration_has_one_ten_second_budget_for_storage_and_verification(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    // Permit dedicated-worker wakeups; provider gates and explicit clock advances
    // still control the ordering and all application deadlines.
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let selection = Selection {
        server: crate::session::DEFAULT_SERVER_URL.into(),
        user: User {
            id: "42".into(),
            username: "Ada".into(),
        },
        expires_at: 4_070_908_800,
    };
    let config = Config {
        server: Some(selection.server.clone()),
        saved: Some(selection.clone()),
        pending_deletions: vec![],
    };
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let (calls, provider) = std::sync::mpsc::channel();
    let store = Persistence::start(GatedStore(calls), Some(path));
    let (verify, requests) = std::sync::mpsc::channel();
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = crate::views::app_shell::open(
            window,
            cx,
            bound_api(VerifyAuth(verify)),
            config,
            Some(store),
            execution,
        );
        Root::new(view, window, cx)
    });
    cx.run_until_parked();
    let ProviderCall::Read(read) = provider.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("expected credential read")
    };
    advance(cx, 7);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("auth-feedback")
                .label()
                .unwrap()
                .contains("Checking saved session")
        );
    });
    read.try_send(Ok(Some("synthetic-token".into()))).unwrap();
    let verification = await_verification(cx, &requests);
    advance(cx, 2);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("auth-feedback")
                .label()
                .unwrap()
                .contains("Checking saved session")
        );
    });
    advance(cx, 1);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Log in"));
        assert!(
            window
                .find("storage-status")
                .label()
                .unwrap()
                .contains("Could not verify saved login")
        );
        assert_eq!(
            window.find("retry-storage").label(),
            Some("Retry saved-login restoration")
        );
    });
    assert!(verification.try_send(Ok(selection.user)).is_err());
    advance(cx, 20);
    assert!(
        provider.try_recv().is_err(),
        "timeout is not authoritative deletion"
    );
    assert!(
        requests.try_recv().is_err(),
        "verification is not automatically replayed"
    );
}

#[gpui_kit::test]
fn private_restore_candidate_cannot_open_workspace_after_server_change(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let selection = Selection {
        server: crate::session::DEFAULT_SERVER_URL.into(),
        user: User {
            id: "42".into(),
            username: "Ada".into(),
        },
        expires_at: 4_070_908_800,
    };
    let config = Config {
        server: Some(selection.server.clone()),
        saved: Some(selection.clone()),
        pending_deletions: vec![],
    };
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let (calls, provider) = std::sync::mpsc::channel();
    let store = Persistence::start(GatedStore(calls), Some(path));
    let (verify, requests) = std::sync::mpsc::channel();
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = crate::views::app_shell::open(
            window,
            cx,
            bound_api(VerifyAuth(verify)),
            config,
            Some(store),
            execution,
        );
        Root::new(view, window, cx)
    });
    cx.run_until_parked();
    let ProviderCall::Read(read) = provider.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("expected read")
    };
    read.try_send(Ok(Some("saved-token".into()))).unwrap();
    let verification = await_verification(cx, &requests);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("server-url").value(),
            Some(crate::session::DEFAULT_SERVER_URL)
        );
        assert_eq!(window.find("username").value(), Some("Ada"));
        assert!(window.try_find("session-status").is_none());
        assert!(window.try_find("channels").is_none());
        assert!(window.try_find("composer").is_none());
        window.click("server-url", cx);
        window.press("ctrl-a", cx);
        window.input("http://127.0.0.1:8082", cx);
    });
    cx.run_until_parked();
    // Local invalidation is not blocked on deletion or on the old verification response.
    let ProviderCall::Delete(delete) = provider.recv_timeout(Duration::from_secs(5)).unwrap()
    else {
        panic!("expected deletion")
    };
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("session-status").is_none());
        assert_eq!(window.find("login").label(), Some("Log in"));
        window.click("username", cx);
        window.press("ctrl-a", cx);
        window.input("Bob", cx);
        window.click("password", cx);
        window.input("new password", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("session-status").label(),
            Some("Logged in as Bob at http://127.0.0.1:8082")
        );
    });
    verification.try_send(Ok(selection.user)).unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("session-status").label(),
            Some("Logged in as Bob at http://127.0.0.1:8082")
        );
        assert!(window.try_find("auth-feedback").is_none());
    });
    delete.try_send(Ok(None)).unwrap();
    let ProviderCall::Write(write) = provider.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("expected new save")
    };
    write.try_send(Ok(None)).unwrap();
}

#[gpui_kit::test]
fn failed_restoration_can_retry_then_yield_to_manual_login_without_late_activation(
    cx: &mut TestAppContext,
) {
    use super::super::saved_login::wait_status;
    cx.update(gpui_kit::init);
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let selection = Selection {
        server: crate::session::DEFAULT_SERVER_URL.into(),
        user: User {
            id: "42".into(),
            username: "Ada".into(),
        },
        expires_at: 4_070_908_800,
    };
    let config = Config {
        server: Some(selection.server.clone()),
        saved: Some(selection.clone()),
        pending_deletions: vec![],
    };
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let shared: Shared = Arc::new((
        Mutex::new((
            vec![(crate::storage::account(&selection), "saved-token".into())],
            false,
            false,
            false,
            false,
            false,
        )),
        Condvar::new(),
    ));
    let store = Persistence::start(Controlled(shared), Some(path));
    let (verify, requests) = std::sync::mpsc::channel();
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = crate::views::app_shell::open(
            window,
            cx,
            bound_api(VerifyAuth(verify)),
            config,
            Some(store),
            execution,
        );
        Root::new(view, window, cx)
    });
    let first = await_verification(cx, &requests);
    first.try_send(Err(ApiError::Unavailable)).unwrap();
    wait_status(cx, "Could not verify saved login");
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("username").value(), Some("Ada"));
        assert!(window.try_find("session-status").is_none());
        assert_eq!(
            window.find("retry-storage").label(),
            Some("Retry saved-login restoration")
        );
        window.click("retry-storage", cx);
    });
    let late = await_verification(cx, &requests);
    advance(cx, 10);
    wait_status(cx, "Could not verify saved login");
    cx.update(|window, cx| {
        window.click("username", cx);
        window.press("ctrl-a", cx);
        window.input("Manual", cx);
        window.click("password", cx);
        window.input("new password", cx);
        window.click("login", cx);
    });
    wait_status(cx, "Login saved in");
    assert!(
        late.try_send(Ok(selection.user)).is_err(),
        "deadline dropped obsolete verification"
    );
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("session-status")
                .label()
                .unwrap()
                .contains("Manual")
        );
        assert!(window.try_find("retry-storage").is_none());
        assert!(window.try_find("auth-feedback").is_none());
        assert!(window.try_find("password").is_none());
    });
}

#[gpui_kit::test]
fn delayed_save_and_delete_are_unconfirmed_at_ten_seconds_without_blocking_logout(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let (calls, provider) = std::sync::mpsc::channel();
    let store = Persistence::start(GatedStore(calls), Some(dir.path().join("session.json")));
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = crate::views::app_shell::open(
            window,
            cx,
            bound_api(BoundAuth),
            Config::default(),
            Some(store),
            execution,
        );
        Root::new(view, window, cx)
    });
    cx.update(login_controls);
    cx.run_until_parked();
    let ProviderCall::Write(write) = provider.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("expected credential write")
    };
    advance(cx, 9);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("storage-status")
                .label()
                .unwrap()
                .contains("Saving login")
        );
        assert!(
            window
                .find("session-status")
                .label()
                .unwrap()
                .contains("Ada")
        );
    });
    advance(cx, 1);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("storage-status")
                .label()
                .unwrap()
                .contains("saving is unconfirmed")
        );
        window.click("logout", cx);
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Log in"));
    });
    // The blocked write still owns the worker; deletion is queued, not run concurrently.
    assert!(provider.try_recv().is_err());
    advance(cx, 9);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("storage-status")
                .label()
                .unwrap()
                .contains("Removing saved login")
        );
    });
    advance(cx, 1);
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
    write.try_send(Ok(None)).unwrap();
    // Invalidated save rolls back before the queued deletion. Provider work survives UI timeout.
    for _ in 0..2 {
        let ProviderCall::Delete(delete) = provider.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("expected ordered cleanup")
        };
        delete.try_send(Ok(None)).unwrap();
    }
    advance(cx, 10);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Log in"));
        assert!(
            window
                .find("storage-status")
                .label()
                .unwrap()
                .contains("Could not confirm deletion")
        );
    });
}
