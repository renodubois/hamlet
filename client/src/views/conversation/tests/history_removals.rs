//! Removal-safe history through controlled Chat transitions and real Kit controls.
use super::history_test_support::{drain, mount_history};
use crate::api::test_support::{RequestAdapter, Response};
use crate::api::{ApiError, ApiFuture, Message};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, Pixels, ScrollDelta, TestAppContext, Window, point, px};
use std::sync::Arc;

fn message(id: usize) -> Message {
    Message {
        id: format!("{id:06}"),
        channel_id: "1".into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: if id.is_multiple_of(2) {
            format!("message {id}\nsecond line\nthird line\nfourth line")
        } else {
            format!("message {id}")
        },
        created_at: "2026-01-01T00:00:00Z".into(),
    }
}
fn rows(ids: impl DoubleEndedIterator<Item = usize>) -> Vec<Message> {
    ids.rev().map(message).collect()
}
fn page(messages: Vec<Message>, cursor: Option<&str>) -> Response {
    let items: Vec<_> = messages
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "id":m.id,"channel_id":m.channel_id,
                "author":{"id":m.author_id,"display_name":m.author_name},
                "text":m.text,"created_at":m.created_at
            })
        })
        .collect();
    Response::controlled(
        reqwest::StatusCode::OK,
        serde_json::json!({"items":items,"next_cursor":cursor}).to_string(),
    )
}
struct LoadedHistory;
impl RequestAdapter for LoadedHistory {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        assert_eq!(request.method(), reqwest::Method::GET);
        let response = if request.url().path() == "/api/v1/channels" {
            Response::controlled(
                reqwest::StatusCode::OK,
                r#"{"items":[{"id":"1","name":"General","type":"text"}]}"#,
            )
        } else {
            assert!(request.url().query().is_none(), "removal must not backfill");
            page(rows(1..=80), None)
        };
        Box::pin(async move { Ok(response) })
    }
}
struct OlderRequest {
    cursor: String,
    reply: async_channel::Sender<Result<Response, ApiError>>,
}
struct HeldOlder(std::sync::mpsc::Sender<OlderRequest>);
impl RequestAdapter for HeldOlder {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.url().path() == "/api/v1/channels" {
            return LoadedHistory.execute(request);
        }
        if let Some((_, cursor)) = request.url().query_pairs().find(|(key, _)| key == "before") {
            let (reply, receive) = async_channel::bounded(1);
            self.0
                .send(OlderRequest {
                    cursor: cursor.into_owned(),
                    reply,
                })
                .unwrap();
            Box::pin(async move { receive.recv().await.unwrap() })
        } else {
            Box::pin(async { Ok(page(rows(1..=80), Some("000001"))) })
        }
    }
}

#[gpui_kit::test]
fn empty_history_loads_older_using_cursor_of_removed_row_without_backfill(cx: &mut TestAppContext) {
    let (send, requests) = std::sync::mpsc::channel();
    let (activity, cx) = mount_history(cx, Arc::new(HeldOlder(send)));
    activity.replace_history_for_test("1", Vec::new());
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("load-older").label(),
            Some("Load older messages")
        );
        assert!(window.try_find("history-empty").is_none());
        assert!(
            requests.try_recv().is_err(),
            "removal alone cannot backfill"
        );
        window.click("load-older", cx);
    });
    drain(cx, &activity);
    let older = requests.try_recv().expect("deliberate older read");
    assert_eq!(older.cursor, "000001", "cursor survives its displayed row");
    older.reply.try_send(Ok(page(rows(0..=0), None))).unwrap();
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000000").label(),
            Some("message 0\nsecond line\nthird line\nfourth line")
        );
        assert!(window.try_find("load-older").is_none());
    });
    assert!(requests.try_recv().is_err());
}

#[gpui_kit::test]
fn removing_all_rows_preserves_older_loading_retry_and_exhaustion(cx: &mut TestAppContext) {
    let (send, requests) = std::sync::mpsc::channel();
    let (activity, cx) = mount_history(cx, Arc::new(HeldOlder(send)));
    activity.request_older();
    drain(cx, &activity);
    let pending = requests.try_recv().unwrap();
    activity.replace_history_for_test("1", Vec::new());
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("older-loading").label(),
            Some("Loading older messages…")
        );
        assert!(window.try_find("load-older").is_none());
        assert!(window.try_find("retry-older").is_none());
        assert!(window.try_find("history-empty").is_none());
    });
    assert!(
        requests.try_recv().is_err(),
        "removal cannot duplicate the read"
    );
    pending.reply.try_send(Err(ApiError::Unavailable)).unwrap();
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("retry-older").label(),
            Some("Retry older messages")
        );
        assert!(window.try_find("history-empty").is_none());
        assert!(window.try_find("older-loading").is_none());
        window.click("retry-older", cx);
    });
    drain(cx, &activity);
    let retry = requests.try_recv().unwrap();
    assert_eq!(pending.cursor, "000001");
    assert_eq!(retry.cursor, "000001");
    retry.reply.try_send(Ok(page(Vec::new(), None))).unwrap();
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("history-empty").label(),
            Some("No messages in this channel yet.")
        );
        assert!(window.try_find("load-older").is_none());
        assert!(window.try_find("retry-older").is_none());
        assert!(window.try_find("older-loading").is_none());
    });
    assert!(requests.try_recv().is_err());
}

fn scroll_up(window: &mut Window, cx: &mut App) -> (usize, Pixels) {
    window.render_frame(cx);
    for _ in 0..5 {
        window.scroll("history", ScrollDelta::Pixels(point(px(0.), px(83.))), cx);
        window.render_frame(cx);
    }
    assert!(window.try_find("jump-latest").is_some());
    let top = window.find("history").bounds().top();
    // The partially visible first row is the reader's anchor, not an arbitrary row.
    (1..=80)
        .find_map(|id| {
            let item = window.try_find(format!("message-{id:06}"))?;
            let bounds = item.bounds();
            (bounds.top() <= top && bounds.bottom() > top).then_some((id, bounds.top()))
        })
        .expect("partially visible anchor")
}

#[gpui_kit::test]
fn bottom_reader_follows_latest_removal_and_mixed_edits(cx: &mut TestAppContext) {
    let (activity, cx) = mount_history(cx, Arc::new(LoadedHistory));
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("jump-latest").is_none());
    });
    activity.replace_history_for_test("1", rows(1..=79));
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("message-000080").is_none());
        assert!(window.try_find("jump-latest").is_none());
        assert_eq!(
            window.find("message-000079").bounds().bottom(),
            window.find("history").bounds().bottom()
        );
    });
    let mut next = rows((1..=79).filter(|id| *id != 2));
    let mut inserted = message(78);
    inserted.id = "000078a".into();
    next.insert(1, inserted);
    activity.replace_history_for_test("1", next);
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("jump-latest").is_none());
        assert_eq!(
            window.find("message-000079").bounds().bottom(),
            window.find("history").bounds().bottom()
        );
    });
}

#[gpui_kit::test]
fn removed_anchor_prefers_next_survivor_on_tie_not_new_insertions(cx: &mut TestAppContext) {
    let (activity, cx) = mount_history(cx, Arc::new(LoadedHistory));
    let anchor = cx.update(scroll_up);
    let mut next = rows((1..=80).filter(|id| *id != anchor.0));
    let mut inserted = message(anchor.0);
    inserted.id.push('a');
    let ix = next
        .iter()
        .position(|row| row.id == format!("{:06}", anchor.0 - 1))
        .unwrap();
    next.insert(ix, inserted);
    activity.replace_history_for_test("1", next);
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find(format!("message-{:06}", anchor.0 + 1))
                .bounds()
                .top(),
            anchor.1
        );
        assert!(window.try_find("jump-latest").is_some());
    });
}

#[gpui_kit::test]
fn removed_anchor_uses_nearest_preceding_survivor_at_its_top(cx: &mut TestAppContext) {
    let (activity, cx) = mount_history(cx, Arc::new(LoadedHistory));
    let anchor = cx.update(scroll_up);
    activity.replace_history_for_test(
        "1",
        rows((1..=80).filter(|id| !(*id >= anchor.0 && *id <= anchor.0 + 3))),
    );
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find(format!("message-{:06}", anchor.0 - 1))
                .bounds()
                .top(),
            // GPUI exposes measured bounds only at/after the logical anchor.
            // The preceding fallback must be visible, not given an unverified offset.
            window.find("history").bounds().top()
        );
        assert!(window.try_find("jump-latest").is_some());
        assert!(window.try_find("message-000080").is_none());
    });
}

#[gpui_kit::test]
fn deleting_tall_anchor_keeps_shorter_next_survivor_visible(cx: &mut TestAppContext) {
    let (activity, cx) = mount_history(cx, Arc::new(LoadedHistory));
    let mut messages = rows(1..=80);
    messages
        .iter_mut()
        .find(|row| row.id == "000064")
        .unwrap()
        .text = "tall message line\n".repeat(25);
    activity.replace_history_for_test("1", messages);
    drain(cx, &activity);
    let survivor_height = cx.update(|window, cx| {
        window.render_frame(cx);
        for _ in 0..30 {
            if window.try_find("message-000064").is_some() {
                break;
            }
            window.scroll("history", ScrollDelta::Pixels(point(px(0.), px(80.))), cx);
            window.render_frame(cx);
        }
        let top = window.find("history").bounds().top();
        let row = window.find("message-000064").bounds();
        assert!(row.size.height > px(200.));
        // Newly encountered rows replace offscreen height estimates during layout.
        // Correct against rendered bounds until the intended partial row is stable.
        for _ in 0..5 {
            let row_top = window.find("message-000064").bounds().top();
            if row_top == top - px(200.) {
                break;
            }
            window.scroll(
                "history",
                ScrollDelta::Pixels(point(px(0.), top - px(200.) - row_top)),
                cx,
            );
            window.render_frame(cx);
        }
        assert_eq!(window.find("message-000064").bounds().top(), top - px(200.));
        let survivor_height = window.find("message-000065").bounds().size.height;
        assert!(
            survivor_height < px(200.),
            "offset exceeds replacement height"
        );
        assert!(window.try_find("jump-latest").is_some());
        survivor_height
    });
    activity.replace_history_for_test("1", rows((1..=80).filter(|id| *id != 64)));
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let top = window.find("history").bounds().top();
        let replacement = window.find("message-000065").bounds();
        assert!(
            replacement.bottom() > top,
            "chosen replacement must remain visible"
        );
        assert_eq!(replacement.top(), top - survivor_height + px(1.));
        assert!(window.try_find("jump-latest").is_some());
    });
}

#[gpui_kit::test]
fn mixed_removals_preserve_surviving_variable_height_anchor(cx: &mut TestAppContext) {
    let (activity, cx) = mount_history(cx, Arc::new(LoadedHistory));
    let anchor = cx.update(scroll_up);
    let layout = cx.update(|window, _| window.find("history").bounds());
    // Above, within and below the viewport, including the latest row, with a prepend.
    let mut next = rows((1..=79).filter(|id| *id != 2 && *id != anchor.0 + 1));
    next.push(message(0));
    activity.replace_history_for_test("1", next);
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find(format!("message-{:06}", anchor.0))
                .bounds()
                .top(),
            anchor.1
        );
        assert_eq!(window.find("history").bounds(), layout);
        assert!(window.try_find("message-000080").is_none());
        assert!(window.try_find("jump-latest").is_some());
    });
}
