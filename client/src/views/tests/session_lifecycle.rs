//! Cross-screen cleanup through real Kit controls and the actual application lifecycle.
use super::bound_auth::*;
use super::*;
use crate::runtime::Execution;
use serde_json::json;

struct ManyChannels;
impl RequestAdapter for ManyChannels {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.url().path() == "/api/v1/channels" {
            let items: Vec<_> = (1..=60)
                .map(|id| json!({"id":format!("{id:015}"), "name":format!("Channel {id}"), "type":"text"}))
                .collect();
            return Box::pin(async move {
                Ok(Response::controlled(
                    StatusCode::OK,
                    json!({"items":items}).to_string(),
                ))
            });
        }
        BoundAuth.execute(request)
    }
}

#[gpui_kit::test]
fn account_footer_stays_fixed_when_channels_scroll(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(window, cx, Arc::new(ManyChannels));
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
    cx.update(|window, cx| {
        window.render_frame(cx);
        let footer = window.find("sidebar-footer").bounds();
        let first = window.find("channel-000000000000001").bounds();
        window.scroll(
            "channels",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(-500.))),
            cx,
        );
        window.render_frame(cx);
        assert!(window.find("channel-000000000000001").bounds().origin.y < first.origin.y);
        assert_eq!(window.find("sidebar-footer").bounds(), footer);
        assert!(window.find("logout").bounds().origin.y >= footer.origin.y);
        window.click("logout", cx);
        window.render_frame(cx);
        assert!(window.try_find("sidebar-footer").is_none());
        assert_eq!(window.find("login").label(), Some("Log in"));
    });
}

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
