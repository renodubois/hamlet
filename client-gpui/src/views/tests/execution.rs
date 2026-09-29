//! Common lifecycle tests: injected execution and real Kit controls, no child fields.
use super::bound_auth::*;
use super::*;
use crate::conversation::{Channel, Message};
use crate::runtime::Execution;

#[path = "execution_storage.rs"]
mod storage;

fn login_controls(window: &mut Window, cx: &mut gpui_kit::App) {
    window.render_frame(cx);
    window.click("username", cx);
    window.input("Ada", cx);
    window.click("password", cx);
    window.input("pass", cx);
    window.click("login", cx);
}

fn assert_draft(window: &mut Window, cx: &mut gpui_kit::App, expected: &str) {
    window.click("composer", cx);
    window.press("ctrl-a", cx);
    window.press("ctrl-c", cx);
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some(expected.into())
    );
}

fn advance(cx: &mut gpui_kit::VisualTestContext, seconds: u64) {
    cx.background_executor
        .advance_clock(Duration::from_secs(seconds));
    cx.run_until_parked();
}

struct DelayedLogin {
    executor: gpui_kit::BackgroundExecutor,
}

impl RequestAdapter for DelayedLogin {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, AuthError>> {
        let delay = request
            .url()
            .path()
            .starts_with("/api/v1/auth/")
            .then(|| self.executor.timer(Duration::from_secs(2)));
        Box::pin(async move {
            if let Some(delay) = delay {
                delay.await;
            }
            BoundAuth.execute(request).await
        })
    }
}

fn delayed_login(executor: gpui_kit::BackgroundExecutor) -> Arc<dyn AuthApi> {
    bound_api(DelayedLogin { executor })
}

#[gpui_kit::test]
fn controlled_login_remains_pending_until_response_then_enters_workspace(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let executor = cx.background_executor.clone();
    let execution = Execution::controlled(executor.clone(), 1_800_000_000);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = crate::views::app_shell::open(
            window,
            cx,
            delayed_login(executor),
            crate::persistence::Config::default(),
            None,
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
    cx.run_until_parked();
    cx.background_executor.advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Signing in…"));
        window.click("login", cx); // duplicate submission stays inert
    });
    cx.background_executor.advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("session-status")
                .label()
                .unwrap()
                .contains("Ada")
        );
        assert!(window.find("message-000000000000001").label().is_some());
    });
}

#[gpui_kit::test]
fn send_times_out_at_nine_seconds_without_replay_or_late_draft_loss(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let sends = Arc::new(Mutex::new(Vec::new()));
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = crate::views::app_shell::open(
            window,
            cx,
            Arc::new(SendAuth(sends.clone())),
            crate::persistence::Config::default(),
            None,
            execution,
        );
        Root::new(view, window, cx)
    });
    cx.update(login_controls);
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("keep this draft", cx);
        window.click("send-message", cx);
    });
    cx.run_until_parked();
    advance(cx, 8);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("send-message").label(), Some("Sending…"));
    });
    advance(cx, 1);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("send-message").label(), Some("Send message"));
        assert_draft(window, cx, "keep this draft");
        assert!(
            window
                .find("send-feedback")
                .label()
                .unwrap()
                .contains("may already have been published")
        );
    });
    let sent = sends.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert!(
        sent[0]
            .2
            .try_send(Ok(Message {
                id: "late".into(),
                channel_id: sent[0].0.clone(),
                text: sent[0].1.clone(),
                author_id: "42".into(),
                author_name: "Ada".into(),
                created_at: "2026-01-01T00:00:00Z".into(),
            }))
            .is_err()
    );
    drop(sent);
    advance(cx, 30);
    assert_eq!(sends.lock().unwrap().len(), 1);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_draft(window, cx, "keep this draft");
    });
}

#[gpui_kit::test]
fn late_login_cannot_replace_a_newer_server_submission(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let executor = cx.background_executor.clone();
    let execution = Execution::controlled(executor.clone(), 1_800_000_000);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = crate::views::app_shell::open(
            window,
            cx,
            delayed_login(executor),
            crate::persistence::Config::default(),
            None,
            execution,
        );
        Root::new(view, window, cx)
    });
    cx.update(login_controls);
    cx.run_until_parked();
    advance(cx, 1);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("server-url", cx);
        window.press("ctrl-a", cx);
        window.input("http://127.0.0.1:8082", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("password").value().is_none_or(str::is_empty));
        window.click("username", cx);
        window.press("ctrl-a", cx);
        window.input("Bob", cx);
        window.click("password", cx);
        window.input("new password", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    advance(cx, 1); // Ada's old server responds; Bob must still be pending.
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Signing in…"));
        assert_eq!(window.find("username").value(), Some("Bob"));
        assert!(window.try_find("session-status").is_none());
    });
    advance(cx, 1);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("session-status").label(),
            Some("Logged in as Bob at http://127.0.0.1:8082")
        );
    });
}

#[gpui_kit::test]
fn expiry_uses_controlled_wall_time_and_clears_the_workspace(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    // BoundAuth expires at 4_070_908_800; the accepted response arrives at second 2.
    let executor = cx.background_executor.clone();
    let execution = Execution::controlled(executor.clone(), 4_070_908_795);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = crate::views::app_shell::open(
            window,
            cx,
            delayed_login(executor),
            crate::persistence::Config::default(),
            None,
            execution,
        );
        Root::new(view, window, cx)
    });
    cx.update(login_controls);
    cx.run_until_parked();
    advance(cx, 2);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("ephemeral draft", cx);
    });
    advance(cx, 2);
    cx.update(|window, cx| {
        window.render_frame(cx);
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
        assert_eq!(window.find("login").label(), Some("Log in"));
        assert!(window.find("password").value().is_none_or(str::is_empty));
        assert!(window.try_find("composer").is_none());
        assert!(
            window
                .find("auth-feedback")
                .label()
                .unwrap()
                .contains("expired")
        );
    });
}

struct ReadCounts {
    channels: Arc<AtomicUsize>,
    history: Arc<AtomicUsize>,
}
impl AuthApi for ReadCounts {
    fn login(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
        TestAuth.login(s, u, p)
    }
    fn signup(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
        TestAuth.signup(s, u, p)
    }
    fn logout(&self, s: String, t: String) -> ApiFuture<Result<(), AuthError>> {
        TestAuth.logout(s, t)
    }
    fn channels(&self, s: String, t: String) -> ApiFuture<Result<Vec<Channel>, AuthError>> {
        self.channels.fetch_add(1, Ordering::SeqCst);
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
        self.history.fetch_add(1, Ordering::SeqCst);
        TestAuth.history(s, t, id)
    }
}

#[gpui_kit::test]
fn automatic_polls_follow_focus_and_three_fifteen_second_intervals(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let channels = Arc::new(AtomicUsize::new(0));
    let history = Arc::new(AtomicUsize::new(0));
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = crate::views::app_shell::open(
            window,
            cx,
            Arc::new(ReadCounts {
                channels: channels.clone(),
                history: history.clone(),
            }),
            crate::persistence::Config::default(),
            None,
            execution,
        );
        Root::new(view, window, cx)
    });
    cx.update(|window, cx| {
        window.activate_window();
        login_controls(window, cx);
    });
    cx.run_until_parked();
    let counts = || {
        (
            channels.load(Ordering::SeqCst),
            history.load(Ordering::SeqCst),
        )
    };
    assert_eq!(counts(), (1, 1));
    // Preserve the startup focus reconciliation on the first tick after initial history.
    advance(cx, 1);
    assert_eq!(counts(), (1, 2));
    advance(cx, 2);
    assert_eq!(counts(), (1, 2));
    advance(cx, 1);
    assert_eq!(counts(), (1, 3));
    advance(cx, 11);
    assert_eq!(counts(), (2, 6)); // history at 1, 4, 7, 10, 13; channels at 15
    cx.deactivate_window();
    advance(cx, 60);
    assert_eq!(counts(), (2, 6));
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("connection-status")
                .label()
                .unwrap()
                .contains("paused")
        );
        window.activate_window();
    });
    cx.run_until_parked();
    assert_eq!(counts(), (3, 7));
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("logout", cx);
    });
    advance(cx, 60);
    assert_eq!(counts(), (3, 7));
}
