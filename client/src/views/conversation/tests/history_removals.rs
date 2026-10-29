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
fn resized_bottom_reader_follows_latest_removal_and_mixed_edits(cx: &mut TestAppContext) {
    let (activity, cx) = mount_history(cx, Arc::new(LoadedHistory));
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("jump-latest").is_none());
    });
    let mut size = cx.update(|window, _| window.viewport_size());
    size.width -= px(120.);
    cx.simulate_resize(size);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000080").bounds().bottom(),
            window.find("history").bounds().bottom()
        );
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
fn resized_scrolled_up_reader_keeps_anchor_on_removal_without_another_wheel(
    cx: &mut TestAppContext,
) {
    let (activity, cx) = mount_history(cx, Arc::new(LoadedHistory));
    let anchor = cx.update(scroll_up);
    let mut size = cx.update(|window, _| window.viewport_size());
    size.width -= px(120.);
    cx.simulate_resize(size);
    let resized_anchor = cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .find(format!("message-{:06}", anchor.0))
            .bounds()
            .top()
    });
    // Width changes invalidate GPUI's offscreen height estimates. Remove the
    // latest row before another wheel can re-seed them in the production view.
    activity.replace_history_for_test("1", rows(1..=79));
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find(format!("message-{:06}", anchor.0))
                .bounds()
                .top(),
            resized_anchor
        );
        assert!(window.try_find("message-000080").is_none());
        assert!(window.try_find("jump-latest").is_some());
    });
}

#[gpui_kit::test]
fn height_only_shrink_at_bottom_follows_before_and_after_delete(cx: &mut TestAppContext) {
    let (activity, cx) = mount_history(cx, Arc::new(LoadedHistory));
    cx.update(|window, cx| window.render_frame(cx));
    let mut size = cx.update(|window, _| window.viewport_size());
    size.height -= px(160.);
    cx.simulate_resize(size);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000080").bounds().bottom(),
            window.find("history").bounds().bottom(),
            "resize itself must follow"
        );
    });
    activity.replace_history_for_test("1", rows(1..=79));
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000079").bounds().bottom(),
            window.find("history").bounds().bottom()
        );
        assert!(window.try_find("jump-latest").is_none());
    });
}

#[gpui_kit::test]
fn height_only_resize_scrolled_reader_keeps_anchor_after_delete(cx: &mut TestAppContext) {
    let (activity, cx) = mount_history(cx, Arc::new(LoadedHistory));
    let anchor = cx.update(scroll_up);
    let mut size = cx.update(|window, _| window.viewport_size());
    size.height += px(160.);
    cx.simulate_resize(size);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find(format!("message-{:06}", anchor.0))
                .bounds()
                .top(),
            anchor.1
        );
    });
    activity.replace_history_for_test("1", rows(1..=79));
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
        assert!(window.try_find("jump-latest").is_some());
    });
}

#[gpui_kit::test]
fn known_fitting_tail_downward_wheel_resumes_follow_for_append(cx: &mut TestAppContext) {
    let (activity, cx, _) = mount_shrunken_tail_reader(cx, false);
    activity.replace_history_for_test("1", rows(71..=73));
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.scroll("history", ScrollDelta::Pixels(point(px(0.), px(1000.))), cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000071").bounds().top(),
            window.find("history").bounds().top()
        );
        assert!(window.try_find("jump-latest").is_some());
        window.scroll("history", ScrollDelta::Pixels(point(px(0.), px(-80.))), cx);
        window.render_frame(cx);
        assert!(
            window.try_find("jump-latest").is_none(),
            "known fitting viewport must retire protection"
        );
    });
    let mut next = rows(71..=73);
    let mut appended = message(81);
    appended.text = "new tall message\n".repeat(44);
    next.insert(0, appended);
    activity.replace_history_for_test("1", next);
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000081").bounds().bottom(),
            window.find("history").bounds().bottom()
        );
        assert!(window.try_find("jump-latest").is_none());
    });
}

#[gpui_kit::test]
fn unchanged_hover_frames_do_not_repeat_tail_padding_layout_after_append(cx: &mut TestAppContext) {
    use super::history_test_support::mount_history_with_view;
    let (activity, view, cx) = mount_history_with_view(
        cx,
        crate::test_support::live::transport(Arc::new(LoadedHistory)),
    );
    let anchor = cx.update(scroll_up);
    activity.replace_history_for_test("1", rows(anchor.0..=anchor.0 + 1));
    drain(cx, &activity);
    cx.update(|window, cx| window.render_frame(cx));
    assert_padding_layouts_reused(cx, &view);
    let mut next = rows(anchor.0..=anchor.0 + 1);
    next.insert(0, message(81));
    activity.replace_history_for_test("1", next.clone());
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window.find("message-000081").bounds().bottom()
                < window.find("history").bounds().bottom()
        );
    });
    assert_padding_layouts_reused(cx, &view);

    let layouts = cx.update(|_, cx| view.read(cx).tail_padding_layouts);
    let mut size = cx.update(|window, _| window.viewport_size());
    size.width -= px(120.);
    cx.simulate_resize(size);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            view.read(cx).tail_padding_layouts > layouts,
            "width invalidates padding"
        );
    });
    assert_padding_layouts_reused(cx, &view);
    let layouts = cx.update(|_, cx| view.read(cx).tail_padding_layouts);
    size.height += px(80.);
    cx.simulate_resize(size);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            view.read(cx).tail_padding_layouts > layouts,
            "height invalidates padding"
        );
    });
    assert_padding_layouts_reused(cx, &view);

    // Same IDs but changed row content must invalidate the cached geometry too.
    let (layouts, old_bottom) = cx.update(|window, cx| {
        (
            view.read(cx).tail_padding_layouts,
            window.find("message-000081").bounds().bottom(),
        )
    });
    next[0].text = "changed appended message\n".repeat(3);
    activity.replace_history_for_test("1", next.clone());
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            view.read(cx).tail_padding_layouts > layouts,
            "content revision invalidates padding"
        );
        assert!(window.find("message-000081").bounds().bottom() > old_bottom);
    });
    assert_padding_layouts_reused(cx, &view);

    next[0].text = "appended tall message\n".repeat(44);
    activity.replace_history_for_test("1", next);
    drain(cx, &activity);
    cx.update(|window, cx| window.render_frame(cx));
    assert_padding_layouts_reused(cx, &view);
}

fn assert_padding_layouts_reused(
    cx: &mut gpui_kit::VisualTestContext,
    view: &gpui_kit::Entity<super::MessageHistoryView>,
) {
    let layouts = cx.update(|_, cx| view.read(cx).tail_padding_layouts);
    assert!(layouts > 0, "fixture exercises padding measurement");
    cx.update(|window, cx| {
        for _ in 0..4 {
            window.hover("history", cx);
            window.render_frame(cx);
        }
        assert_eq!(
            view.read(cx).tail_padding_layouts,
            layouts,
            "unchanged hover frames reuse padding, including zero gap"
        );
    });
}

#[test]
fn unknown_measurements_are_not_a_known_fitting_viewport() {
    // Exercise the pure predicate on independent, unmeasured presentation state,
    // rather than reaching into a mounted view to bypass its scroll controls.
    let list = gpui_kit::ListState::new(80, gpui_kit::ListAlignment::Top, px(0.));
    assert_eq!(list.is_scrolled_to_end(), None);
    assert!(
        !super::known_fitting_viewport(&list),
        "unknown rows must not resume following"
    );
}

fn tall_tail_rows(include_tail: bool) -> Vec<Message> {
    (1..=if include_tail { 75 } else { 74 })
        .rev()
        .map(|id| {
            let mut row = message(id);
            row.text = if id == 75 {
                "latest tall line\n".repeat(44)
            } else {
                format!("message {id}\nsecond line\nthird line")
            };
            row
        })
        .collect()
}

fn mount_shrunken_tail_reader(
    cx: &mut TestAppContext,
    sliver: bool,
) -> (
    crate::chat::ChatHandle,
    &mut gpui_kit::VisualTestContext,
    Pixels,
) {
    let (activity, cx) = mount_history(cx, Arc::new(LoadedHistory));
    activity.replace_history_for_test("1", tall_tail_rows(true));
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("message-000075").bounds().size.height > px(1000.));
        for _ in 0..40 {
            if window.try_find("message-000071").is_some() {
                break;
            }
            window.scroll("history", ScrollDelta::Pixels(point(px(0.), px(80.))), cx);
            window.render_frame(cx);
        }
        let top = window.find("history").bounds().top();
        let offset = if sliver {
            window.find("message-000071").bounds().size.height - px(1.)
        } else {
            px(25.)
        };
        for _ in 0..5 {
            let row_top = window.find("message-000071").bounds().top();
            if row_top == top - offset {
                break;
            }
            window.scroll(
                "history",
                ScrollDelta::Pixels(point(px(0.), top - offset - row_top)),
                cx,
            );
            window.render_frame(cx);
        }
        assert_eq!(window.find("message-000071").bounds().top(), top - offset);
        assert!(
            window.find("message-000075").bounds().top() < window.find("history").bounds().bottom()
        );
        assert!(window.try_find("jump-latest").is_some());
    });
    let mut size = cx.update(|window, _| window.viewport_size());
    size.width -= px(120.);
    cx.simulate_resize(size);
    let anchor_y = cx.update(|window, cx| {
        window.render_frame(cx);
        window.find("message-000071").bounds().top()
    });
    activity.replace_history_for_test("1", tall_tail_rows(false));
    drain(cx, &activity);
    (activity, cx, anchor_y)
}

#[gpui_kit::test]
fn large_tail_shrink_after_resize_keeps_partial_top_anchor_and_jump_latest(
    cx: &mut TestAppContext,
) {
    let (_, cx, anchor_y) = mount_shrunken_tail_reader(cx, false);
    cx.update(|window, cx| {
        for _ in 0..3 {
            window.render_frame(cx);
            assert_eq!(window.find("message-000071").bounds().top(), anchor_y);
            assert!(window.try_find("message-000075").is_none());
            assert!(window.try_find("jump-latest").is_some());
            assert!(
                window.find("message-000074").bounds().bottom()
                    < window.find("history").bounds().bottom()
            );
        }
    });
}

#[gpui_kit::test]
fn appends_consume_tail_space_without_moving_reader_and_jump_retires_it(cx: &mut TestAppContext) {
    let (activity, cx, anchor_y) = mount_shrunken_tail_reader(cx, false);
    let old_bottom = cx.update(|window, cx| {
        window.render_frame(cx);
        window.find("message-000074").bounds().bottom()
    });
    let mut next = tall_tail_rows(false);
    let mut appended = message(76);
    appended.text = "new message\nsecond line\nthird line".into();
    next.insert(0, appended);
    activity.replace_history_for_test("1", next.clone());
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let appended = window.find("message-000076").bounds();
        assert_eq!(appended.top(), old_bottom);
        assert_eq!(window.find("message-000071").bounds().top(), anchor_y);
        assert!(appended.bottom() < window.find("history").bounds().bottom());
        assert!(window.try_find("jump-latest").is_some());
    });
    let mut appended = message(77);
    appended.text = "new tall message\n".repeat(44);
    next.insert(0, appended);
    activity.replace_history_for_test("1", next);
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("message-000071").bounds().top(), anchor_y);
        assert!(
            window.find("message-000077").bounds().bottom()
                > window.find("history").bounds().bottom()
        );
        assert!(window.try_find("jump-latest").is_some());
        window.click("jump-latest", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000077").bounds().bottom(),
            window.find("history").bounds().bottom()
        );
        assert!(window.try_find("jump-latest").is_none());
    });
}

#[gpui_kit::test]
fn upward_wheel_keeps_tail_reader_and_downward_wheel_can_retire_space(cx: &mut TestAppContext) {
    let (_, cx, anchor_y) = mount_shrunken_tail_reader(cx, false);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.scroll("history", ScrollDelta::Pixels(point(px(0.), px(10.))), cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000071").bounds().top(),
            anchor_y + px(10.)
        );
        assert!(window.try_find("jump-latest").is_some());
        window.scroll("history", ScrollDelta::Pixels(point(px(0.), px(-80.))), cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000074").bounds().bottom(),
            window.find("history").bounds().bottom()
        );
        assert!(window.try_find("jump-latest").is_none());
    });
}

#[gpui_kit::test]
fn repeated_tail_shrink_leaves_only_bounded_space_and_a_visible_survivor(cx: &mut TestAppContext) {
    let (activity, cx, anchor_y) = mount_shrunken_tail_reader(cx, true);
    activity.replace_history_for_test(
        "1",
        tall_tail_rows(false)
            .into_iter()
            .filter(|row| row.id == "000071")
            .collect(),
    );
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let row = window.find("message-000071").bounds();
        let pane = window.find("history").bounds();
        assert_eq!(row.top(), anchor_y);
        assert_eq!(row.bottom(), pane.top() + px(1.));
        assert!(window.try_find("jump-latest").is_some());
        window.click("jump-latest", cx);
        window.render_frame(cx);
        assert_eq!(window.find("message-000071").bounds().top(), pane.top());
        assert!(window.try_find("jump-latest").is_none());
    });
}

#[gpui_kit::test]
fn focused_sliver_tray_still_reveals_after_large_tail_shrink(cx: &mut TestAppContext) {
    let (_, cx, _) = mount_shrunken_tail_reader(cx, true);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("history-pane", cx);
        for _ in 0..10 {
            window.press("tab", cx);
            window.render_frame(cx);
            if window.find("trash-message-000071").focused() == Some(true) {
                break;
            }
        }
        assert_eq!(window.find("trash-message-000071").focused(), Some(true));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let trash = window.find("trash-message-000071");
        let pane = window.find("history").bounds();
        assert_eq!(trash.focused(), Some(true));
        assert!(trash.visible());
        assert!(trash.bounds().top() >= pane.top());
        assert!(trash.bounds().bottom() <= pane.bottom());
    });
}

#[gpui_kit::test]
fn large_tail_shrink_and_append_at_bottom_keep_following_without_space(cx: &mut TestAppContext) {
    let (activity, cx) = mount_history(cx, Arc::new(LoadedHistory));
    activity.replace_history_for_test("1", tall_tail_rows(true));
    drain(cx, &activity);
    cx.update(|window, cx| window.render_frame(cx));
    let mut size = cx.update(|window, _| window.viewport_size());
    size.width -= px(120.);
    cx.simulate_resize(size);
    cx.update(|window, cx| window.render_frame(cx));
    activity.replace_history_for_test("1", tall_tail_rows(false));
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000074").bounds().bottom(),
            window.find("history").bounds().bottom()
        );
        assert!(window.try_find("jump-latest").is_none());
    });
    activity.replace_history_for_test("1", tall_tail_rows(true));
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000075").bounds().bottom(),
            window.find("history").bounds().bottom()
        );
        assert!(window.try_find("jump-latest").is_none());
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
