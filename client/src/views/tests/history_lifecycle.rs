//! Independent history lifetime through Kit controls and the owned workspace interface.
use crate::api::test_support::{RequestAdapter, Response};
use crate::api::{ApiError, ApiFuture};
use crate::runtime::Execution;
use crate::views::conversation::message_history::MessageHistoryView;
use crate::workspace::WorkspaceHandle;
use gpui_kit::component::{Root, button::Button};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, Window, div,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct HistoryApi(Arc<AtomicUsize>);
impl RequestAdapter for HistoryApi {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let body = if request.url().path() == "/api/v1/channels" {
            r#"{"items":[{"id":"1","name":"General","type":"text"},{"id":"2","name":"Other","type":"text"}]}"#.to_owned()
        } else {
            let channel = if request.url().path().contains("/1/") {
                "1"
            } else {
                "2"
            };
            serde_json::json!({"items":[{"id":format!("{channel}-latest"),"channel_id":channel,
                "author":{"id":"u","display_name":"Ada"},"text":format!("channel {channel}\nsecond line"),
                "created_at":"2026-01-01T00:00:00Z"}]}).to_string()
        };
        Box::pin(async move { Ok(Response::controlled(reqwest::StatusCode::OK, body)) })
    }
}
struct HistoryHost {
    activity: WorkspaceHandle,
    history: Entity<MessageHistoryView>,
    visible: bool,
}
impl Render for HistoryHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut host = div().size_full().flex().flex_col();
        if self.visible {
            host = host.child(self.history.clone());
        }
        host.child(
            Button::new("recreate-history")
                .label("Recreate history")
                .on_click(cx.listener(|host, _, window, cx| {
                    host.history =
                        cx.new(|cx| MessageHistoryView::new(host.activity.clone(), window, cx));
                    cx.notify();
                })),
        )
        .child(
            Button::new("toggle-history")
                .label("Toggle history")
                .on_click(cx.listener(|host, _, _, cx| {
                    host.visible = !host.visible;
                    cx.notify();
                })),
        )
    }
}
fn drain(cx: &mut gpui_kit::VisualTestContext, activity: &WorkspaceHandle) {
    loop {
        cx.run_until_parked();
        let Ok(update) = activity.updates().try_recv() else {
            break;
        };
        assert!(activity.apply(update).is_none());
    }
}
#[gpui_kit::test]
fn independent_history_hydrates_cache_recreates_and_clears_when_hidden(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let calls = Arc::new(AtomicUsize::new(0));
    let client = crate::test_support::live::transport(Arc::new(HistoryApi(calls.clone())))
        .server("https://history.example")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    let activity = WorkspaceHandle::new(
        1,
        1_800_001_000,
        client,
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
    );
    activity.start();
    // Load before constructing the child: construction must hydrate, not initiate reads.
    loop {
        cx.run_until_parked();
        let Ok(update) = activity.updates().try_recv() else {
            break;
        };
        assert!(activity.apply(update).is_none());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let history = cx.new(|cx| MessageHistoryView::new(activity.clone(), window, cx));
        let host = cx.new(|_| HistoryHost {
            activity: activity.clone(),
            history,
            visible: true,
        });
        Root::new(host, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-1-latest").label(),
            Some("channel 1\nsecond line")
        );
        window.click("recreate-history", cx);
    });
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-1-latest").label(),
            Some("channel 1\nsecond line")
        );
        window.click("toggle-history", cx);
    });
    activity.select_channel("2");
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.click("toggle-history", cx);
        window.render_frame(cx);
        assert!(window.try_find("message-1-latest").is_none());
        assert_eq!(
            window.find("message-2-latest").label(),
            Some("channel 2\nsecond line")
        );
        window.click("toggle-history", cx);
    });
    activity.close();
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.click("toggle-history", cx);
        window.render_frame(cx);
        assert!(window.try_find("message-2-latest").is_none());
        assert!(window.try_find("refresh-history").is_none());
        assert!(window.try_find("jump-latest").is_none());
    });
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "recreation and shutdown cannot dispatch"
    );
}

#[gpui_kit::test]
fn history_shutdown_clears_selected_text(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let client =
        crate::test_support::live::transport(Arc::new(HistoryApi(Arc::new(AtomicUsize::new(0)))))
            .server("https://history.example")
            .unwrap()
            .restore_candidate("synthetic".into())
            .unwrap();
    let activity = WorkspaceHandle::new(
        1,
        1_800_001_000,
        client,
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
    );
    activity.start();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let history = cx.new(|cx| MessageHistoryView::new(activity.clone(), window, cx));
        let host = cx.new(|_| HistoryHost {
            activity: activity.clone(),
            history,
            visible: true,
        });
        Root::new(host, window, cx)
    });
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let bounds = window.find("message-text-1-latest").bounds();
        window.drag(
            bounds.origin + gpui_kit::point(gpui_kit::px(2.), gpui_kit::px(2.)),
            bounds.origin
                + gpui_kit::point(
                    bounds.size.width - gpui_kit::px(2.),
                    bounds.size.height - gpui_kit::px(2.),
                ),
            cx,
        );
        assert_eq!(
            gpui_kit::base::TextSelection::selected_text(window, cx),
            "channel 1\nsecond line"
        );
    });
    activity.close();
    drain(cx, &activity);
    cx.update(|window, cx| {
        assert!(
            gpui_kit::base::TextSelection::selected_text(window, cx).is_empty(),
            "shutdown must clear retained selection"
        );
    });
}

struct ReaderHistory {
    reads: AtomicUsize,
}
impl RequestAdapter for ReaderHistory {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        assert_eq!(request.method(), reqwest::Method::GET);
        self.reads.fetch_add(1, Ordering::SeqCst);
        let body = if request.url().path() == "/api/v1/channels" {
            serde_json::json!({"items":[{"id":"1","name":"General","type":"text"}]})
        } else {
            serde_json::json!({"items":(1..=40).rev().map(|id| {
                serde_json::json!({"id":id.to_string(),"channel_id":"1",
                    "author":{"id":"u","display_name":"Ada"},"text":format!("message {id}\nsecond line"),
                    "created_at":"2026-01-01T00:00:00Z"})
            }).collect::<Vec<_>>()})
        };
        Box::pin(async move {
            Ok(Response::controlled(
                reqwest::StatusCode::OK,
                body.to_string(),
            ))
        })
    }
}

#[gpui_kit::test]
fn healthy_events_and_reconnect_keep_reader_anchor_without_http_reads(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let api = Arc::new(ReaderHistory {
        reads: AtomicUsize::new(0),
    });
    let streams = crate::test_support::live::Streams::default();
    let client = streams
        .transport(api.clone())
        .server("https://history.example")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    let activity = WorkspaceHandle::new(
        1,
        1_800_001_000,
        client,
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
    );
    activity.start();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let history = cx.new(|cx| MessageHistoryView::new(activity.clone(), window, cx));
        let host = cx.new(|_| HistoryHost {
            activity: activity.clone(),
            history,
            visible: true,
        });
        Root::new(host, window, cx)
    });
    drain(cx, &activity);
    let anchor = cx.update(|window, cx| {
        window.render_frame(cx);
        for _ in 0..4 {
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(90.))),
                cx,
            );
            window.render_frame(cx);
        }
        assert!(window.try_find("message-40").is_none());
        (1..40)
            .find_map(|id| {
                window
                    .try_find(format!("message-{id}"))
                    .map(|item| (id, item.bounds().origin.y))
            })
            .unwrap()
    });
    streams.change(serde_json::json!({"type":"message_created","message":{"id":"41","channel_id":"1","author":{"id":"u","display_name":"Ada"},"text":"message 41\nsecond line","created_at":"2026-01-01T00:00:00Z"}}));
    drain(cx, &activity);
    let assert_anchor = |window: &mut Window, cx: &mut gpui_kit::App| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find(format!("message-{}", anchor.0))
                .bounds()
                .origin
                .y,
            anchor.1
        );
        assert!(window.try_find("message-41").is_none());
    };
    cx.update(assert_anchor);
    assert_eq!(api.reads.load(Ordering::SeqCst), 2);
    assert_eq!(streams.count(), 1);
    streams.disconnect();
    drain(cx, &activity);
    assert_eq!(
        activity.status(),
        "Live updates disconnected — reconnecting."
    );
    cx.update(assert_anchor);
    cx.background_executor
        .advance_clock(std::time::Duration::from_secs(3));
    drain(cx, &activity);
    cx.update(assert_anchor);
    assert_eq!(streams.count(), 2);
    assert_eq!(api.reads.load(Ordering::SeqCst), 2);
    assert_eq!(activity.status(), "");
    cx.update(|window, cx| {
        window.click("jump-latest", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("message-41").label(),
            Some("message 41\nsecond line")
        );
    });
}
