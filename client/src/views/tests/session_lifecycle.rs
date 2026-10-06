//! Cross-screen cleanup through real Kit controls and the actual application lifecycle.
use super::bound_auth::*;
use super::*;
use crate::runtime::Execution;

struct RejectNextHistory(Arc<AtomicBool>);
impl RequestAdapter for RejectNextHistory {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.url().path().ends_with("/messages") && self.0.swap(false, Ordering::SeqCst) {
            return Box::pin(async { Ok(Response::controlled(StatusCode::UNAUTHORIZED, "{}")) });
        }
        BoundAuth.execute(request)
    }
}

#[gpui_kit::test]
fn current_rejection_clears_all_channel_drafts_before_same_process_login(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let reject = Arc::new(AtomicBool::new(false));
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = crate::views::app_shell::open(
            window,
            cx,
            bound_api(RejectNextHistory(reject.clone())),
            crate::storage::Config::default(),
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
        window.input("password", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    for channel in ["channel-000000000000001", "channel-000000000000002"] {
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click(channel, cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            window.input("private draft", cx);
        });
    }
    reject.store(true, Ordering::SeqCst);
    cx.update(|window, cx| {
        window.click("composer", cx);
        window.dispatch_action(
            Box::new(gpui_kit::base::input::Enter {
                secondary: false,
                shift: false,
            }),
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("composer").is_none());
        assert!(window.try_find("channels").is_none());
        assert!(
            window
                .find("auth-feedback")
                .label()
                .unwrap()
                .contains("rejected")
        );
        assert!(window.find("password").value().is_none_or(str::is_empty));
        window.click("password", cx);
        window.input("password", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    for channel in ["channel-000000000000001", "channel-000000000000002"] {
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click(channel, cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            window.input("new", cx);
            window.press("ctrl-a", cx);
            window.press("ctrl-c", cx);
            assert_eq!(
                cx.read_from_clipboard().and_then(|item| item.text()),
                Some("new".into())
            );
        });
    }
}
