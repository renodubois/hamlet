//! Minute refreshes through the real history UI and controlled execution clock.
use super::history_test_support::{HistoryHost, ReaderHistory, drain};
use crate::api::test_support::{RequestAdapter, Response};
use crate::api::{ApiError, ApiFuture};
use crate::runtime::Execution;
use crate::views::conversation::message_history::MessageHistoryView;
use crate::workspace::WorkspaceHandle;
use chrono::{DateTime, Local, TimeZone};
use gpui_kit::component::{Root, button::Button};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, point, px};
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, Window, div,
};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

#[gpui_kit::test]
fn minute_refresh_without_messages_preserves_reader_and_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let api = Arc::new(ReaderHistory {
        reads: AtomicUsize::new(0),
        created_at: "2027-01-15T08:00:00Z",
    });
    let client = crate::test_support::live::transport(api.clone())
        .server("https://timestamps.example")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let activity = WorkspaceHandle::new(1, 1_800_100_000, client, execution.clone());
    activity.start();
    let ticks = Rc::new(Cell::new(0));
    let mut subscription = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let history = cx.new(|cx| MessageHistoryView::new(activity.clone(), execution, window, cx));
        let observed = ticks.clone();
        subscription = Some(cx.observe(&history, move |_, _, _| observed.set(observed.get() + 1)));
        let host = cx.new(|_| HistoryHost {
            activity: activity.clone(),
            history,
            visible: true,
        });
        Root::new(host, window, cx)
    });
    drain(cx, &activity);
    let (id, y, selected) = cx.update(|window, cx| {
        window.render_frame(cx);
        for _ in 0..4 {
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(point(px(0.), px(90.))),
                cx,
            );
            window.render_frame(cx);
        }
        let bounds = window.find("history").bounds();
        let (id, row) = (1..40)
            .find_map(|id| {
                let row = window.try_find(format!("message-{id}"))?;
                (row.bounds().origin.y >= bounds.origin.y
                    && row.bounds().bottom() <= bounds.bottom())
                .then_some((id, row))
            })
            .unwrap();
        let text = window.find(format!("message-text-{id}")).bounds();
        window.drag(
            text.origin + point(px(2.), px(2.)),
            text.bottom_right() - point(px(2.), px(2.)),
            cx,
        );
        let selected = gpui_kit::base::TextSelection::selected_text(window, cx);
        assert_eq!(selected, format!("message {id}\nsecond line"));
        (id, row.bounds().origin.y, selected)
    });
    cx.run_until_parked();
    let before = ticks.get();
    cx.background_executor
        .advance_clock(Duration::from_secs(59));
    cx.run_until_parked();
    assert_eq!(ticks.get(), before, "no early refresh");
    cx.background_executor.advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find(format!("message-timestamp-{id}")).label(),
            Some("1 minute ago")
        );
        assert_eq!(window.find(format!("message-{id}")).bounds().origin.y, y);
        assert_eq!(
            gpui_kit::base::TextSelection::selected_text(window, cx),
            selected
        );
        assert!(window.try_find("message-40").is_none());
    });
    assert_eq!(
        ticks.get(),
        before + 1,
        "one automatic refresh, not a forced redraw"
    );
    assert_eq!(api.reads.load(Ordering::SeqCst), 2);
}

struct TimestampApi {
    instants: Vec<String>,
    reads: AtomicUsize,
}
impl RequestAdapter for TimestampApi {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let body = if request.url().path() == "/api/v1/channels" {
            serde_json::json!({"items":[{"id":"1","name":"General","type":"text"},{"id":"2","name":"Other","type":"text"}]})
        } else {
            let channel = if request.url().path().contains("/1/") {
                "1"
            } else {
                "2"
            };
            serde_json::json!({"items":self.instants.iter().enumerate().map(|(i, instant)| {
                serde_json::json!({"id":format!("{channel}-{i}"),"channel_id":channel,
                    "author":{"id":"u","display_name":"Ada"},"text":format!("message {i}"),"created_at":instant})
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

struct RefreshHost {
    activity: WorkspaceHandle,
    execution: Execution,
    history: Option<Entity<MessageHistoryView>>,
}
impl Render for RefreshHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .children(self.history.clone())
            .child(
                Button::new("toggle-conversation")
                    .label("Toggle conversation")
                    .on_click(cx.listener(|host, _, window, cx| {
                        if host.history.is_some() {
                            host.history = None;
                        } else {
                            host.history = Some(cx.new(|cx| {
                                MessageHistoryView::new(
                                    host.activity.clone(),
                                    host.execution.clone(),
                                    window,
                                    cx,
                                )
                            }));
                        }
                        cx.notify();
                    })),
            )
    }
}

fn setup(
    cx: &mut TestAppContext,
    now: DateTime<Local>,
    instants: Vec<String>,
) -> (
    Entity<RefreshHost>,
    &mut gpui_kit::VisualTestContext,
    WorkspaceHandle,
    Arc<TimestampApi>,
) {
    cx.update(gpui_kit::init);
    let api = Arc::new(TimestampApi {
        instants,
        reads: AtomicUsize::new(0),
    });
    let client = crate::test_support::live::transport(api.clone())
        .server("https://timestamps.example")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    let execution = Execution::controlled(cx.background_executor.clone(), now.timestamp());
    let activity = WorkspaceHandle::new(1, now.timestamp() + 200_000, client, execution.clone());
    activity.start();
    let mut host = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let history =
            cx.new(|cx| MessageHistoryView::new(activity.clone(), execution.clone(), window, cx));
        let view = cx.new(|_| RefreshHost {
            activity: activity.clone(),
            execution,
            history: Some(history),
        });
        host = Some(view.clone());
        Root::new(view, window, cx)
    });
    let host = host.unwrap();
    drain(cx, &activity);
    (host, cx, activity, api)
}

fn labels(cx: &mut gpui_kit::VisualTestContext, channel: &str, expected: &[&str]) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        for (i, expected) in expected.iter().enumerate() {
            assert_eq!(
                window
                    .find(format!("message-timestamp-{channel}-{i}"))
                    .label(),
                Some(*expected)
            );
        }
    });
}

#[gpui_kit::test]
fn elapsed_boundaries_future_catchup_switching_and_cleanup(cx: &mut TestAppContext) {
    let now = Local
        .with_ymd_and_hms(2026, 3, 14, 12, 0, 0)
        .single()
        .unwrap();
    let (host, cx, activity, api) = setup(
        cx,
        now,
        vec![
            (now + chrono::Duration::seconds(60)).to_rfc3339(),
            (now - chrono::Duration::seconds(3599)).to_rfc3339(),
            (now - chrono::Duration::seconds(59)).to_rfc3339(),
        ],
    );
    labels(cx, "1", &["just now", "59 minutes ago", "just now"]);
    let ticks = Rc::new(Cell::new(0));
    let (weak, subscription) = cx.update(|_, cx| {
        let history = host.read(cx).history.as_ref().unwrap().clone();
        let observed = ticks.clone();
        (
            history.downgrade(),
            cx.observe(&history, move |_, _| observed.set(observed.get() + 1)),
        )
    });
    cx.run_until_parked();
    for expected in [
        ["just now", "1 hour ago", "1 minute ago"],
        ["1 minute ago", "1 hour ago", "2 minutes ago"],
    ] {
        let before = ticks.get();
        cx.background_executor
            .advance_clock(Duration::from_secs(60));
        cx.run_until_parked();
        assert_eq!(ticks.get(), before + 1);
        labels(cx, "1", &expected);
    }
    activity.select_channel("2");
    drain(cx, &activity);
    labels(cx, "2", &["1 minute ago", "1 hour ago", "2 minutes ago"]);
    activity.select_channel("1");
    drain(cx, &activity);
    let before = ticks.get();
    cx.background_executor
        .advance_clock(Duration::from_secs(60));
    cx.run_until_parked();
    assert_eq!(
        ticks.get(),
        before + 1,
        "switching must not add refresh loops"
    );
    labels(cx, "1", &["2 minutes ago", "1 hour ago", "3 minutes ago"]);
    cx.update(|window, cx| window.click("toggle-conversation", cx));
    cx.run_until_parked();
    cx.update(|_, _| {
        assert!(
            weak.upgrade().is_none(),
            "timer must not retain the closed view"
        )
    });
    let before = ticks.get();
    cx.background_executor
        .advance_clock(Duration::from_secs(60));
    cx.run_until_parked();
    assert_eq!(ticks.get(), before);
    drop(subscription);
    cx.update(|window, cx| window.click("toggle-conversation", cx));
    drain(cx, &activity);
    labels(cx, "1", &["3 minutes ago", "1 hour ago", "4 minutes ago"]);
    cx.background_executor
        .advance_clock(Duration::from_secs(60));
    cx.run_until_parked();
    labels(cx, "1", &["4 minutes ago", "1 hour ago", "5 minutes ago"]);
    assert_eq!(
        api.reads.load(Ordering::SeqCst),
        3,
        "refreshes and reopening cannot fetch history"
    );
}

#[gpui_kit::test]
fn next_minute_after_local_midnight_uses_calendar_labels(cx: &mut TestAppContext) {
    let now = Local
        .with_ymd_and_hms(2026, 3, 14, 23, 59, 30)
        .single()
        .unwrap();
    let today = Local
        .with_ymd_and_hms(2026, 3, 14, 13, 41, 0)
        .single()
        .unwrap();
    let yesterday = Local
        .with_ymd_and_hms(2026, 3, 13, 13, 41, 0)
        .single()
        .unwrap();
    let (_, cx, _activity, api) = setup(cx, now, vec![today.to_rfc3339(), yesterday.to_rfc3339()]);
    labels(cx, "1", &["10 hours ago", "yesterday at 1:41 PM"]);
    cx.background_executor
        .advance_clock(Duration::from_secs(60));
    cx.run_until_parked();
    labels(cx, "1", &["yesterday at 1:41 PM", "03/13/2026 1:41 PM"]);
    assert_eq!(api.reads.load(Ordering::SeqCst), 2);
}
