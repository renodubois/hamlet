//! Controlled provider work stays on the real serialized worker, never the UI thread.
use super::*;
use crate::persistence::{Config, Selection, Store};

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

struct VerifyAuth(std::sync::mpsc::Sender<async_channel::Sender<Result<User, AuthError>>>);
impl AuthApi for VerifyAuth {
    fn login(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
        TestAuth.login(s, u, p)
    }
    fn signup(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
        TestAuth.signup(s, u, p)
    }
    fn logout(&self, s: String, t: String) -> ApiFuture<Result<(), AuthError>> {
        TestAuth.logout(s, t)
    }
    fn current_user(&self, _: String, _: String) -> ApiFuture<Result<User, AuthError>> {
        let (tx, rx) = async_channel::bounded(1);
        self.0.send(tx).unwrap();
        Box::pin(async move { rx.recv().await.unwrap() })
    }
    fn channels(&self, s: String, t: String) -> ApiFuture<Result<Vec<Channel>, AuthError>> {
        TestAuth.channels(s, t)
    }
    fn create_channel(
        &self,
        s: String,
        t: String,
        n: String,
    ) -> ApiFuture<Result<Channel, AuthError>> {
        TestAuth.create_channel(s, t, n)
    }
    fn history(
        &self,
        s: String,
        t: String,
        id: String,
    ) -> ApiFuture<Result<Vec<Message>, AuthError>> {
        TestAuth.history(s, t, id)
    }
}

// Only the dedicated provider thread uses wall time. Pump delivery until its observable
// API request arrives; all application deadlines remain on the controlled clock.
fn await_verification(
    cx: &mut gpui_kit::VisualTestContext,
    requests: &std::sync::mpsc::Receiver<async_channel::Sender<Result<User, AuthError>>>,
) -> async_channel::Sender<Result<User, AuthError>> {
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
            Arc::new(VerifyAuth(verify)),
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
            Arc::new(TestAuth),
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
