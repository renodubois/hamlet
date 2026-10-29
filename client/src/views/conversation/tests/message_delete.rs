//! Real history controls driven by validated deletion events, not row replacement.
use super::history_test_support::{drain, mount_history_with_transport};
use crate::api::test_support::{RequestAdapter, Response};
use crate::api::{ApiError, ApiFuture};
use crate::test_support::live::Streams;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ScrollDelta, TestAppContext, point, px};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const CHANNEL: &str = "000000000000001";
fn page(count: usize, cursor: Option<&str>) -> Response {
    Response::controlled(
        reqwest::StatusCode::OK,
        serde_json::json!({
            "items":(1..=count).map(|id| serde_json::json!({
                "id":format!("{id:015}"),"channel_id":CHANNEL,
                "author":{"id":"u","display_name":"Ada"},
                "text":if id % 2 == 0 { format!("message {id}\nsecond line\nthird line\nfourth line") } else { format!("message {id}") },
                "created_at":"2026-01-01T00:00:00Z"
            })).collect::<Vec<_>>(),"next_cursor":cursor
        })
        .to_string(),
    )
}
struct History {
    reads: AtomicUsize,
    count: usize,
    cursor: Option<&'static str>,
    older: async_channel::Sender<async_channel::Sender<Result<Response, ApiError>>>,
}
impl RequestAdapter for History {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        assert_eq!(request.method(), reqwest::Method::GET);
        self.reads.fetch_add(1, Ordering::SeqCst);
        if let Some((_, cursor)) = request.url().query_pairs().find(|(key, _)| key == "before") {
            assert_eq!(cursor, "cursor-for-removed-row");
            let (send, receive) = async_channel::bounded(1);
            self.older.try_send(send).unwrap();
            return Box::pin(async move { receive.recv().await.unwrap() });
        }
        let response = if request.url().path() == "/api/v1/channels" {
            Response::controlled(
                reqwest::StatusCode::OK,
                serde_json::json!({"items":[{"id":CHANNEL,"name":"General","type":"text"}]})
                    .to_string(),
            )
        } else {
            page(self.count, self.cursor)
        };
        Box::pin(async move { Ok(response) })
    }
}
fn remove(streams: &Streams, id: usize) {
    streams.change(serde_json::json!({"type":"message_deleted","channel_id":CHANNEL,"message_id":format!("{id:015}")}));
}

#[gpui_kit::test]
fn live_deletion_preserves_empty_history_load_loading_retry_and_exhaustion_controls(
    cx: &mut TestAppContext,
) {
    let (older, calls) = async_channel::unbounded();
    let api = Arc::new(History {
        reads: AtomicUsize::new(0),
        count: 2,
        cursor: Some("cursor-for-removed-row"),
        older,
    });
    let streams = Streams::default();
    let (chat, cx) = mount_history_with_transport(cx, streams.transport(api.clone()));
    for id in 1..=2 {
        remove(&streams, id);
    }
    drain(cx, &chat);
    assert_eq!(
        api.reads.load(Ordering::SeqCst),
        2,
        "deletion alone does not backfill"
    );
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("message-000000000000001").is_none());
        assert!(window.try_find("message-000000000000002").is_none());
        assert!(window.try_find("history-empty").is_none());
        assert_eq!(
            window.find("load-older").label(),
            Some("Load older messages")
        );
        window.click("load-older", cx);
    });
    drain(cx, &chat);
    let pending = calls.try_recv().unwrap();
    remove(&streams, 1);
    drain(cx, &chat);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("older-loading").label(),
            Some("Loading older messages…")
        );
        assert!(window.try_find("history-empty").is_none());
    });
    pending.try_send(Err(ApiError::Unavailable)).unwrap();
    drain(cx, &chat);
    remove(&streams, 2);
    drain(cx, &chat);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("retry-older").label(),
            Some("Retry older messages")
        );
        assert!(window.try_find("history-empty").is_none());
        window.click("retry-older", cx);
    });
    drain(cx, &chat);
    // This stale page contains both deleted rows and cannot bring either back.
    calls
        .try_recv()
        .unwrap()
        .try_send(Ok(page(2, None)))
        .unwrap();
    drain(cx, &chat);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("history-empty").label(),
            Some("No messages in this channel yet.")
        );
        assert!(window.try_find("load-older").is_none());
        assert!(window.try_find("retry-older").is_none());
    });
    assert_eq!(api.reads.load(Ordering::SeqCst), 4);
}

#[gpui_kit::test]
fn live_deletion_preserves_bottom_follow_and_scrolled_surviving_anchor(cx: &mut TestAppContext) {
    let (older, _) = async_channel::unbounded();
    let api = Arc::new(History {
        reads: AtomicUsize::new(0),
        count: 80,
        cursor: None,
        older,
    });
    let streams = Streams::default();
    let (chat, cx) = mount_history_with_transport(cx, streams.transport(api.clone()));
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("jump-latest").is_none());
    });
    remove(&streams, 80);
    drain(cx, &chat);
    let (anchor, top) = cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("message-000000000000080").is_none());
        assert!(window.try_find("jump-latest").is_none());
        assert_eq!(
            window.find("message-000000000000079").bounds().bottom(),
            window.find("history").bounds().bottom()
        );
        for _ in 0..5 {
            window.scroll("history", ScrollDelta::Pixels(point(px(0.), px(83.))), cx);
            window.render_frame(cx);
        }
        assert!(window.try_find("jump-latest").is_some());
        let top = window.find("history").bounds().top();
        (1..=79)
            .find_map(|id| {
                let bounds = window.try_find(format!("message-{id:015}"))?.bounds();
                (bounds.top() <= top && bounds.bottom() > top).then_some((id, bounds.top()))
            })
            .expect("partially visible anchor")
    });
    for id in [2, anchor + 1, 79] {
        remove(&streams, id);
    }
    drain(cx, &chat);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find(format!("message-{anchor:015}")).bounds().top(),
            top
        );
        assert!(window.try_find("jump-latest").is_some());
    });
    remove(&streams, anchor);
    drain(cx, &chat);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find(format!("message-{:015}", anchor + 2))
                .bounds()
                .top(),
            top
        );
        assert!(window.try_find("jump-latest").is_some());
    });
    assert_eq!(api.reads.load(Ordering::SeqCst), 2);
}
