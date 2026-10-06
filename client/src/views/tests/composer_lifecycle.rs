//! Independent composer lifetime through real Kit controls and the conversation interface.
use crate::api::test_support::{RequestAdapter, Response};
use crate::api::{ApiError, ApiFuture};
use crate::conversation::ConversationHandle;
use crate::runtime::Execution;
use crate::views::conversation::composer::ComposerView;
use gpui_kit::component::{Root, button::Button};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, Window, div,
};
use std::sync::{Arc, Mutex};

type Send = (
    String,
    String,
    async_channel::Sender<Result<Response, ApiError>>,
);
#[derive(Default)]
struct ComposerApi(Mutex<Vec<Send>>);
impl RequestAdapter for ComposerApi {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.method() == reqwest::Method::POST {
            let body: serde_json::Value =
                serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
            let (send, receive) = async_channel::bounded(1);
            self.0.lock().unwrap().push((
                request.url().path().into(),
                body["text"].as_str().unwrap().into(),
                send,
            ));
            return Box::pin(async move { receive.recv().await.unwrap() });
        }
        let body = if request.url().path() == "/api/v1/channels" {
            r#"{"items":[{"id":"1","name":"General","type":"text"},{"id":"2","name":"Other","type":"text"}]}"#
        } else {
            r#"{"items":[]}"#
        };
        Box::pin(async move { Ok(Response::controlled(reqwest::StatusCode::OK, body)) })
    }
}
struct ComposerHost {
    activity: ConversationHandle,
    composer: Entity<ComposerView>,
    visible: bool,
}
impl Render for ComposerHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut host = div().size_full().flex().flex_col();
        if self.visible {
            host = host.child(self.composer.clone());
        }
        host.child(
            Button::new("recreate-composer")
                .label("Recreate composer")
                .on_click(cx.listener(|host, _, window, cx| {
                    host.composer =
                        cx.new(|cx| ComposerView::new(host.activity.clone(), window, cx));
                    cx.notify();
                })),
        )
        .child(
            Button::new("toggle-composer")
                .label("Toggle composer")
                .on_click(cx.listener(|host, _, _, cx| {
                    host.visible = !host.visible;
                    cx.notify();
                })),
        )
    }
}
fn drain(cx: &mut gpui_kit::VisualTestContext, activity: &ConversationHandle) {
    loop {
        cx.run_until_parked();
        let Ok(update) = activity.updates().try_recv() else {
            break;
        };
        assert!(activity.apply(update).is_none());
    }
}
fn text(window: &mut Window, cx: &mut gpui_kit::App) -> String {
    window.render_frame(cx);
    window.click("composer", cx);
    window.press("ctrl-a", cx);
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(String::new()));
    window.press("ctrl-c", cx);
    cx.read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap_or_default()
}
fn mount(
    cx: &mut TestAppContext,
    api: Arc<ComposerApi>,
) -> (ConversationHandle, &mut gpui_kit::VisualTestContext) {
    cx.update(gpui_kit::init);
    let client = crate::test_support::live::transport(api)
        .server("https://composer.example")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    let activity = ConversationHandle::new(
        1,
        1_800_001_000,
        client,
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
    );
    activity.start();
    loop {
        cx.run_until_parked();
        let Ok(update) = activity.updates().try_recv() else {
            break;
        };
        assert!(activity.apply(update).is_none());
    }
    activity.edit_draft("first\ntrailing\n".into());
    activity.select_channel("2");
    activity.edit_draft("other draft".into());
    activity.select_channel("1");
    let (_, cx) = cx.add_window_view(|window, cx| {
        let composer = cx.new(|cx| ComposerView::new(activity.clone(), window, cx));
        let host = cx.new(|_| ComposerHost {
            activity: activity.clone(),
            composer,
            visible: true,
        });
        Root::new(host, window, cx)
    });
    drain(cx, &activity);
    (activity, cx)
}

#[gpui_kit::test]
fn retained_hidden_composer_observes_invalidation_and_cannot_restore_drafts(
    cx: &mut TestAppContext,
) {
    let api = Arc::new(ComposerApi::default());
    let (activity, cx) = mount(cx, api.clone());
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.dispatch_action(
            Box::new(gpui_kit::base::input::Enter {
                secondary: false,
                shift: false,
            }),
            cx,
        );
    });
    drain(cx, &activity);
    cx.update(|window, cx| window.click("toggle-composer", cx));
    drain(cx, &activity);
    let (_, _, late) = api.0.lock().unwrap().remove(0);
    // Session invalidation closes the same handle even if a child remains retained.
    activity.close();
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.click("toggle-composer", cx);
        window.render_frame(cx);
        assert!(window.try_find("composer").is_none());
        assert!(window.try_find("send-message").is_none());
        assert!(window.try_find("send-feedback").is_none());
        window.dispatch_action(
            Box::new(gpui_kit::base::input::Enter {
                secondary: false,
                shift: false,
            }),
            cx,
        );
        window.click("recreate-composer", cx);
    });
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("composer").is_none());
    });
    assert_eq!(activity.read().draft("1"), "");
    assert_eq!(activity.read().draft("2"), "");
    assert!(late.try_send(Err(ApiError::AlreadyInvalid)).is_err());
    assert!(api.0.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn composer_recreation_observes_originating_pending_confirmation_and_timeout(
    cx: &mut TestAppContext,
) {
    let api = Arc::new(ComposerApi::default());
    let (activity, cx) = mount(cx, api.clone());
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.dispatch_action(
            Box::new(gpui_kit::base::input::Enter {
                secondary: false,
                shift: false,
            }),
            cx,
        );
    });
    drain(cx, &activity);
    cx.update(|window, cx| window.click("recreate-composer", cx));
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("send-message").is_none());
        assert_eq!(text(window, cx), "first\ntrailing\n");
        window.input("cannot edit while pending", cx);
        window.click("composer", cx);
        window.dispatch_action(
            Box::new(gpui_kit::base::input::Enter {
                secondary: false,
                shift: false,
            }),
            cx,
        );
        assert_eq!(text(window, cx), "first\ntrailing\n");
    });
    assert_eq!(api.0.lock().unwrap().len(), 1);
    let (path, sent, reply) = api.0.lock().unwrap().remove(0);
    assert_eq!(path, "/api/v1/channels/1/messages");
    assert_eq!(sent, "first\ntrailing\n");
    activity.select_channel("2");
    drain(cx, &activity);
    cx.update(|window, cx| {
        assert_eq!(text(window, cx), "other draft");
        window.input("still editable", cx);
        window.click("toggle-composer", cx);
    });
    reply.try_send(Ok(Response::controlled(reqwest::StatusCode::CREATED,
        r#"{"id":"published","channel_id":"1","author":{"id":"u","display_name":"Ada"},"text":"first\ntrailing\n","created_at":"2026-01-01T00:00:00Z"}"#
    ))).unwrap();
    drain(cx, &activity);
    assert_eq!(activity.read().draft("1"), "");
    assert_eq!(activity.read().draft("2"), "still editable");
    cx.update(|window, cx| {
        window.click("toggle-composer", cx);
        assert_eq!(text(window, cx), "still editable");
        assert!(window.try_find("send-message").is_none());
    });
    activity.select_channel("1");
    drain(cx, &activity);
    cx.update(|window, cx| {
        assert_eq!(text(window, cx), "");
        window.input("uncertain draft", cx);
        window.click("composer", cx);
        window.dispatch_action(
            Box::new(gpui_kit::base::input::Enter {
                secondary: false,
                shift: false,
            }),
            cx,
        );
    });
    drain(cx, &activity);
    cx.update(|window, cx| window.click("recreate-composer", cx));
    drain(cx, &activity);
    let (_, sent, late) = api.0.lock().unwrap().remove(0);
    assert_eq!(sent, "uncertain draft");
    cx.background_executor
        .advance_clock(std::time::Duration::from_secs(9));
    drain(cx, &activity);
    cx.update(|window, cx| {
        // No click or input is needed to receive pending/error notification updates.
        window.render_frame(cx);
        assert!(window.try_find("send-message").is_none());
        assert!(
            window
                .find("send-feedback")
                .label()
                .unwrap()
                .contains("may already")
        );
        assert_eq!(text(window, cx), "uncertain draft");
        window.click("recreate-composer", cx);
    });
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("send-feedback")
                .label()
                .unwrap()
                .contains("may already")
        );
        assert_eq!(text(window, cx), "uncertain draft");
    });
    assert!(late.try_send(Err(ApiError::AlreadyInvalid)).is_err());
    assert!(
        api.0.lock().unwrap().is_empty(),
        "recreation and timeout never replay writes"
    );
}

#[gpui_kit::test]
fn queued_composer_enter_cannot_submit_the_newly_selected_channels_draft(cx: &mut TestAppContext) {
    let api = Arc::new(ComposerApi::default());
    let (activity, cx) = mount(cx, api.clone());
    cx.update(|window, cx| {
        assert_eq!(text(window, cx), "first\ntrailing\n");
        window.input("final edit for first channel", cx);
        // Navigation has occurred, but the old focused textarea has not hydrated yet.
        // Its queued Enter belongs to that old presentation, never the new draft.
        activity.select_channel("2");
        window.dispatch_action(
            Box::new(gpui_kit::base::input::Enter {
                secondary: false,
                shift: false,
            }),
            cx,
        );
    });
    drain(cx, &activity);
    assert!(
        api.0.lock().unwrap().is_empty(),
        "stale Enter cannot submit another channel"
    );
    assert_eq!(activity.read().draft("1"), "final edit for first channel");
    assert_eq!(activity.read().draft("2"), "other draft");
    cx.update(|window, cx| {
        assert_eq!(text(window, cx), "other draft");
    });
}

#[gpui_kit::test]
fn independent_composer_hydrates_and_recreates_without_overwriting_channel_drafts(
    cx: &mut TestAppContext,
) {
    let api = Arc::new(ComposerApi::default());
    let (activity, cx) = mount(cx, api.clone());
    cx.update(|window, cx| {
        assert_eq!(text(window, cx), "first\ntrailing\n");
        window.input("edited through textarea", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.click("recreate-composer", cx);
    });
    drain(cx, &activity);
    cx.update(|window, cx| {
        assert_eq!(text(window, cx), "edited through textarea");
    });
    activity.select_channel("2");
    drain(cx, &activity);
    cx.update(|window, cx| {
        assert_eq!(text(window, cx), "other draft");
        window.click("recreate-composer", cx);
        window.click("recreate-composer", cx);
    });
    drain(cx, &activity);
    assert_eq!(activity.read().draft("1"), "edited through textarea");
    assert_eq!(activity.read().draft("2"), "other draft");
    assert!(api.0.lock().unwrap().is_empty(), "hydration cannot submit");
}
