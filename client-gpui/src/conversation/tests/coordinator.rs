//! Owned conversation interface with bound HTTP outcomes and controlled execution/time.
use super::*;
use crate::api::test_support::{RequestAdapter, Response};
use crate::api::{ApiError, ApiFuture, HttpTransport};
use crate::runtime::Execution;
use gpui_kit::TestAppContext;
use reqwest::{Request, StatusCode};
use std::sync::Arc;

struct Call {
    request: Request,
    reply: async_channel::Sender<Result<Response, ApiError>>,
}
struct Gated(async_channel::Sender<Call>);
impl RequestAdapter for Gated {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        let (reply, receive) = async_channel::bounded(1);
        self.0.try_send(Call { request, reply }).unwrap();
        Box::pin(async move { receive.recv().await.unwrap_or(Err(ApiError::Unavailable)) })
    }
}
fn fixture(cx: &TestAppContext) -> (ConversationHandle, async_channel::Receiver<Call>) {
    let (send, calls) = async_channel::unbounded();
    let client = HttpTransport::with_adapter(Arc::new(Gated(send)))
        .server("https://conversation.example")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    (
        ConversationHandle::new(
            7,
            1_800_001_000,
            client,
            Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
        ),
        calls,
    )
}
fn respond(call: Call, value: serde_json::Value) {
    call.reply
        .try_send(Ok(Response::controlled(StatusCode::OK, value.to_string())))
        .unwrap();
}
fn drain(cx: &mut TestAppContext, activity: &ConversationHandle) {
    loop {
        cx.executor().run_until_parked();
        let Ok(update) = activity.updates().try_recv() else {
            break;
        };
        assert!(activity.apply(update).is_none());
    }
}
#[gpui_kit::test]
fn independent_observers_receive_intentions_completions_and_shutdown(cx: &mut TestAppContext) {
    let (activity, calls) = fixture(cx);
    let sidebar = activity.notifications();
    let layout = activity.notifications();
    activity.start(false);
    assert!(sidebar.try_recv().is_ok());
    assert!(layout.try_recv().is_ok());
    drain(cx, &activity);
    respond(
        calls.try_recv().unwrap(),
        serde_json::json!({"items":[
        {"id":"1","name":"General","type":"text"},
        {"id":"2","name":"Other","type":"text"}]}),
    );
    drain(cx, &activity);
    assert!(sidebar.try_recv().is_ok());
    assert!(layout.try_recv().is_ok());
    activity.select_channel("2");
    assert!(sidebar.try_recv().is_ok());
    assert!(layout.try_recv().is_ok());
    assert_eq!(activity.read().selected.as_deref(), Some("2"));
    activity.create_channel("bad!");
    assert!(sidebar.try_recv().is_ok());
    assert!(activity.read().create_feedback.is_some());
    drop(sidebar);
    let recreated = activity.notifications();
    activity.close();
    assert!(layout.try_recv().is_ok());
    assert!(recreated.try_recv().is_ok());
    assert!(activity.read().selected.is_none());
}

#[gpui_kit::test]
fn channel_navigation_reuses_history_and_drafts_without_a_view(cx: &mut TestAppContext) {
    let (activity, calls) = fixture(cx);
    activity.start(false);
    cx.executor().run_until_parked();
    let channels = calls.try_recv().unwrap();
    assert_eq!(channels.request.url().path(), "/api/v1/channels");
    respond(
        channels,
        serde_json::json!({"items":[
        {"id":"1","name":"General","type":"text"},
        {"id":"2","name":"Other","type":"text"}]}),
    );
    drain(cx, &activity);
    let history = calls.try_recv().unwrap();
    assert_eq!(history.request.url().path(), "/api/v1/channels/1/messages");
    respond(history, serde_json::json!({"items":[],"next_cursor":null}));
    drain(cx, &activity);
    activity.edit_draft("first draft".into());
    activity.select_channel("2");
    drain(cx, &activity);
    respond(
        calls.try_recv().unwrap(),
        serde_json::json!({"items":[],"next_cursor":null}),
    );
    drain(cx, &activity);
    activity.edit_draft("second draft".into());
    activity.select_channel("1");
    drain(cx, &activity);
    assert_eq!(activity.read().selected.as_deref(), Some("1"));
    assert_eq!(activity.read().draft("1"), "first draft");
    assert_eq!(activity.read().draft("2"), "second draft");
    assert!(
        calls.try_recv().is_err(),
        "cached selection does not refetch"
    );
}

#[gpui_kit::test]
fn delayed_tick_delivery_does_not_move_the_fifteen_second_channel_deadline(
    cx: &mut TestAppContext,
) {
    let (activity, calls) = ready(cx);
    activity.set_focused(true);
    fn answer_reads(
        cx: &mut TestAppContext,
        activity: &ConversationHandle,
        calls: &async_channel::Receiver<Call>,
    ) -> usize {
        let mut channels = 0;
        loop {
            drain(cx, activity);
            let Ok(call) = calls.try_recv() else {
                return channels;
            };
            assert_eq!(call.request.method(), reqwest::Method::GET);
            if call.request.url().path() == "/api/v1/channels" {
                channels += 1;
                respond(
                    call,
                    serde_json::json!({"items":[{"id":"1","name":"General","type":"text"}]}),
                );
            } else {
                respond(call, page(&["8"], None));
            }
        }
    }
    assert_eq!(answer_reads(cx, &activity, &calls), 1);
    // A busy host applies timer deliveries late. Poll ticks must not drift with
    // that queue latency, or a channel read due at 15s will have no wakeup at 15s.
    for _ in 0..11 {
        cx.background_executor
            .advance_clock(Duration::from_millis(1300));
        assert_eq!(answer_reads(cx, &activity, &calls), 0);
    }
    cx.background_executor
        .advance_clock(Duration::from_millis(700));
    assert_eq!(answer_reads(cx, &activity, &calls), 1);
}

fn message(id: &str, channel: &str) -> serde_json::Value {
    serde_json::json!({"id":id,"channel_id":channel,"author":{"id":"u","display_name":"Ada"},
        "text":"same text", "created_at":"2026-01-01T00:00:00Z"})
}
fn page(ids: &[&str], cursor: Option<&str>) -> serde_json::Value {
    serde_json::json!({"items":ids.iter().map(|id| message(id, "1")).collect::<Vec<_>>(),"next_cursor":cursor})
}
fn ready(cx: &mut TestAppContext) -> (ConversationHandle, async_channel::Receiver<Call>) {
    let (activity, calls) = fixture(cx);
    activity.start(false);
    drain(cx, &activity);
    respond(
        calls.try_recv().unwrap(),
        serde_json::json!({"items":[
        {"id":"1","name":"General","type":"text"},
        {"id":"2","name":"Other","type":"text"}]}),
    );
    drain(cx, &activity);
    respond(
        calls.try_recv().unwrap(),
        page(&["8"], Some("older opaque")),
    );
    drain(cx, &activity);
    (activity, calls)
}
fn ids(activity: &ConversationHandle) -> Vec<String> {
    let state = activity.read();
    let Some(Load::Ready(messages)) = state.history.get("1") else {
        panic!("history not ready")
    };
    messages.iter().map(|message| message.id.clone()).collect()
}
fn created(call: Call, value: serde_json::Value) {
    call.reply
        .try_send(Ok(Response::controlled(
            StatusCode::CREATED,
            value.to_string(),
        )))
        .unwrap();
}

#[gpui_kit::test]
fn closed_activity_discards_queued_pages_creates_sends_and_rejections(cx: &mut TestAppContext) {
    for rejected in [false, true] {
        let (activity, calls) = ready(cx);
        activity.edit_draft("do not retain".into());
        activity.send();
        activity.create_channel("New room");
        activity.request_older();
        drain(cx, &activity);
        let mut held = Vec::new();
        while let Ok(call) = calls.try_recv() {
            held.push(call);
        }
        assert_eq!(held.len(), 3);
        for call in held {
            if rejected {
                call.reply
                    .try_send(Ok(Response::controlled(
                        StatusCode::UNAUTHORIZED,
                        r#"{"error":{"code":"unauthorized"}}"#,
                    )))
                    .unwrap();
            } else if call.request.method() == reqwest::Method::GET {
                respond(call, page(&["7"], None));
            } else if call.request.url().path().ends_with("messages") {
                created(call, message("9", "1"));
            } else {
                created(
                    call,
                    serde_json::json!({"id":"3","name":"New room","type":"text"}),
                );
            }
        }
        // These responses have finished, so cancellation cannot remove the queued delivery.
        cx.executor().run_until_parked();
        assert_eq!(activity.updates().len(), 3);
        let survivor = activity.clone();
        activity.close();
        assert!(survivor.read().history.is_empty());
        assert!(survivor.read().drafts.is_empty());
        drain(cx, &survivor);
        survivor.start(true);
        survivor.set_focused(true);
        survivor.select_channel("1");
        survivor.edit_draft("cannot reopen".into());
        survivor.send();
        survivor.create_channel("Cannot reopen");
        survivor.refresh_channels();
        survivor.refresh_history();
        survivor.request_older();
        survivor.retry_older();
        cx.background_executor
            .advance_clock(Duration::from_secs(60));
        drain(cx, &survivor);
        assert!(calls.try_recv().is_err());
        assert!(survivor.read().selected.is_none());
        assert!(survivor.read().drafts.is_empty());
    }
}

#[gpui_kit::test]
fn current_rejection_reports_origin_and_closes_before_host_removes_workspace(
    cx: &mut TestAppContext,
) {
    let (activity, calls) = ready(cx);
    activity.edit_draft("private draft".into());
    activity.refresh_history();
    drain(cx, &activity);
    calls
        .try_recv()
        .unwrap()
        .reply
        .try_send(Ok(Response::controlled(
            StatusCode::UNAUTHORIZED,
            r#"{"error":{"code":"unauthorized"}}"#,
        )))
        .unwrap();
    cx.executor().run_until_parked();
    let update = activity.updates().try_recv().unwrap();
    assert_eq!(activity.apply(update), Some(SessionEnd::Rejected(7)));
    assert!(activity.read().drafts.is_empty());
    assert!(activity.read().channels.is_none());
    activity.refresh_channels();
    drain(cx, &activity);
    assert!(calls.try_recv().is_err());
}

#[gpui_kit::test]
fn second_confirmation_during_catchup_waits_for_fresh_contiguous_read(cx: &mut TestAppContext) {
    let (activity, calls) = ready(cx);
    activity.edit_draft("same text".into());
    activity.send();
    activity.refresh_history();
    drain(cx, &activity);
    let first = calls.try_recv().unwrap();
    let second = calls.try_recv().unwrap();
    let (send, poll) = if first.request.method() == reqwest::Method::POST {
        (first, second)
    } else {
        (second, first)
    };
    created(send, message("10", "1"));
    drain(cx, &activity);
    assert!(
        calls.try_recv().is_err(),
        "confirmation cannot overlap the pending poll"
    );
    respond(poll, page(&["9", "8"], None));
    drain(cx, &activity);
    let fresh = calls.try_recv().unwrap();
    assert_eq!(ids(&activity), ["9", "8"]);
    activity.edit_draft("same text".into());
    activity.send();
    drain(cx, &activity);
    created(calls.try_recv().unwrap(), message("12", "1"));
    drain(cx, &activity);
    respond(fresh, page(&["10"], Some("opaque +/cursor")));
    drain(cx, &activity);
    let continuation = calls.try_recv().unwrap();
    assert_eq!(
        continuation
            .request
            .url()
            .query_pairs()
            .find(|(key, _)| key == "before")
            .unwrap()
            .1,
        "opaque +/cursor"
    );
    assert_eq!(
        ids(&activity),
        ["9", "8"],
        "unconnected segment stays staged"
    );
    respond(continuation, page(&["10", "9", "8"], None));
    drain(cx, &activity);
    let after_second = calls.try_recv().unwrap();
    assert!(after_second.request.url().query().is_none());
    assert_eq!(ids(&activity), ["10", "9", "8"]);
    after_second
        .reply
        .try_send(Err(ApiError::Unavailable))
        .unwrap();
    drain(cx, &activity);
    assert!(matches!(
        activity.read().refreshing.get("1"),
        Some(Refresh::Incomplete(_))
    ));
    assert_eq!(ids(&activity), ["10", "9", "8"]);
    assert!(
        calls.try_recv().is_err(),
        "failed catch-up must not spin or replay a write"
    );
    activity.refresh_history();
    activity.refresh_history();
    drain(cx, &activity);
    let retry = calls.try_recv().unwrap();
    assert!(calls.try_recv().is_err());
    respond(retry, page(&["12", "11", "10"], None));
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["12", "11", "10", "9", "8"]);
    // Recovery establishes continuity first; the retained confirmation then gets its
    // own safe read, exactly as when a confirmation overlaps any ordinary refresh.
    let confirmation_read = calls.try_recv().unwrap();
    assert_eq!(confirmation_read.request.method(), reqwest::Method::GET);
    respond(confirmation_read, page(&["12", "11", "10"], None));
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["12", "11", "10", "9", "8"]);
    assert!(!activity.read().send_feedback.contains_key("1"));
    assert_eq!(activity.read().draft("1"), "");
    assert!(calls.try_recv().is_err());
}

#[gpui_kit::test]
fn pending_sends_are_per_channel_and_timeout_retains_only_originating_draft(
    cx: &mut TestAppContext,
) {
    let (activity, calls) = ready(cx);
    activity.edit_draft("first draft".into());
    activity.send();
    activity.send();
    activity.edit_draft("must not replace locked draft".into());
    drain(cx, &activity);
    let first_send = calls.try_recv().unwrap();
    assert!(calls.try_recv().is_err());
    activity.select_channel("2");
    drain(cx, &activity);
    respond(
        calls.try_recv().unwrap(),
        serde_json::json!({"items":[],"next_cursor":null}),
    );
    drain(cx, &activity);
    activity.edit_draft("second draft".into());
    activity.send();
    drain(cx, &activity);
    created(calls.try_recv().unwrap(), message("20", "2"));
    drain(cx, &activity);
    respond(
        calls.try_recv().unwrap(),
        serde_json::json!({"items":[],"next_cursor":null}),
    );
    drain(cx, &activity);
    assert_eq!(activity.read().draft("2"), "");
    cx.background_executor.advance_clock(Duration::from_secs(8));
    drain(cx, &activity);
    assert!(activity.read().send_pending.contains_key("1"));
    cx.background_executor.advance_clock(Duration::from_secs(1));
    drain(cx, &activity);
    assert!(!activity.read().send_pending.contains_key("1"));
    assert_eq!(activity.read().draft("1"), "first draft");
    assert!(activity.read().send_feedback["1"].contains("may already have been published"));
    assert!(
        first_send
            .reply
            .try_send(Ok(Response::controlled(
                StatusCode::CREATED,
                message("9", "1").to_string()
            )))
            .is_err()
    );
    assert!(
        calls.try_recv().is_err(),
        "inactive origin is not read or replayed"
    );
    activity.select_channel("1");
    drain(cx, &activity);
    let read = calls.try_recv().unwrap();
    assert_eq!(read.request.method(), reqwest::Method::GET);
    respond(read, page(&["9", "8"], None));
    drain(cx, &activity);
    assert_eq!(
        activity.read().draft("1"),
        "first draft",
        "matching text never confirms publication"
    );
    assert!(calls.try_recv().is_err());
}
