//! Owned workspace lifecycle with authenticated HTTP and SSE outcomes.
use super::live_support::*;
use super::*;
use crate::api::test_support::Response;
use gpui_kit::TestAppContext;
use reqwest::StatusCode;

#[gpui_kit::test]
fn independent_observers_receive_intentions_completions_and_shutdown(cx: &mut TestAppContext) {
    let (activity, calls, streams) = fixture(cx);
    let sidebar = activity.notifications();
    let layout = activity.notifications();
    activity.start();
    assert!(sidebar.try_recv().is_ok());
    assert!(layout.try_recv().is_ok());
    drain(cx, &activity);
    let stream = streams.try_recv().unwrap();
    stream.ready();
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), channels());
    drain(cx, &activity);
    assert!(sidebar.try_recv().is_ok());
    assert!(layout.try_recv().is_ok());
    activity.select_channel("2");
    assert!(sidebar.try_recv().is_ok());
    assert!(layout.try_recv().is_ok());
    assert_eq!(activity.read().selected.as_deref(), Some("2"));
    activity.create_channel("bad!");
    assert!(activity.read().create_feedback.is_some());
    drop(sidebar);
    let recreated = activity.notifications();
    activity.close();
    drain(cx, &activity);
    assert!(layout.try_recv().is_ok());
    assert!(recreated.try_recv().is_ok());
    assert!(activity.read().selected.is_none());
    assert!(stream.body.is_closed());
}

#[gpui_kit::test]
fn channel_navigation_reuses_history_and_drafts_without_a_view(cx: &mut TestAppContext) {
    let (activity, calls, streams, _stream) = ready(cx);
    activity.edit_draft("first draft".into());
    activity.select_channel("2");
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), page(&[]));
    drain(cx, &activity);
    activity.edit_draft("second draft".into());
    activity.select_channel("1");
    drain(cx, &activity);
    assert_eq!(activity.read().draft("1"), "first draft");
    assert_eq!(activity.read().draft("2"), "second draft");
    assert!(calls.is_empty());
    assert!(streams.is_empty());
}

#[gpui_kit::test]
fn closed_activity_discards_queued_pages_creates_sends_and_rejections(cx: &mut TestAppContext) {
    for rejected in [false, true] {
        let (activity, calls, streams, stream) = ready(cx);
        activity.edit_draft("private draft".into());
        activity.send();
        activity.create_channel("New room");
        activity.select_channel("2");
        drain(cx, &activity);
        for _ in 0..3 {
            let call = calls.try_recv().unwrap();
            let (status, value) = if rejected {
                (
                    StatusCode::UNAUTHORIZED,
                    serde_json::json!({"error":{"code":"unauthorized"}}),
                )
            } else if call.request.method() == reqwest::Method::GET {
                (StatusCode::OK, page(&[]))
            } else if call.request.url().path().ends_with("messages") {
                (StatusCode::CREATED, message("9", "1"))
            } else {
                (
                    StatusCode::CREATED,
                    serde_json::json!({"id":"3","name":"New room","type":"text"}),
                )
            };
            call.reply
                .try_send(Ok(Response::controlled(status, value.to_string())))
                .unwrap();
        }
        cx.executor().run_until_parked();
        assert_eq!(activity.updates().len(), 3);
        let survivor = activity.clone();
        activity.close();
        drain(cx, &survivor);
        survivor.start();
        survivor.select_channel("1");
        survivor.edit_draft("cannot reopen".into());
        survivor.send();
        survivor.create_channel("Cannot reopen");
        survivor.request_older();
        survivor.retry_older();
        cx.background_executor
            .advance_clock(Duration::from_secs(60));
        drain(cx, &survivor);
        assert!(calls.is_empty());
        assert!(streams.is_empty());
        assert!(stream.body.is_closed());
        assert!(survivor.read().drafts.is_empty());
        assert!(survivor.read().selected.is_none());
    }
}

#[gpui_kit::test]
fn current_read_and_stream_rejections_close_before_host_removes_workspace(cx: &mut TestAppContext) {
    for reject_stream in [false, true] {
        let (activity, calls, _, stream) = ready(cx);
        activity.edit_draft("private draft".into());
        if reject_stream {
            stream.body.try_send(Err(ApiError::AlreadyInvalid)).unwrap();
        } else {
            activity.select_channel("2");
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
        }
        cx.executor().run_until_parked();
        assert_eq!(
            activity.apply(activity.updates().try_recv().unwrap()),
            Some(SessionEnd::Rejected(7))
        );
        assert!(activity.read().drafts.is_empty());
        assert!(activity.read().channels.is_none());
        drain(cx, &activity);
        assert!(stream.body.is_closed());
    }
}

#[gpui_kit::test]
fn expiry_timer_remains_active_while_retrying(cx: &mut TestAppContext) {
    let (activity, _, streams, stream) = ready(cx);
    stream.body.close();
    drain(cx, &activity);
    activity.edit_draft("private draft".into());
    cx.background_executor
        .advance_clock(Duration::from_secs(1000));
    cx.executor().run_until_parked();
    assert_eq!(
        activity.apply(activity.updates().try_recv().unwrap()),
        Some(SessionEnd::Expired(7))
    );
    assert!(activity.read().drafts.is_empty());
    assert!(
        streams.is_empty(),
        "expiry gates retry before any protected work"
    );
}

#[gpui_kit::test]
fn pending_sends_are_per_channel_timeout_preserves_origin_and_never_reads_or_replays(
    cx: &mut TestAppContext,
) {
    let (activity, calls, _, stream) = ready(cx);
    activity.edit_draft("first draft".into());
    activity.send();
    activity.send();
    activity.edit_draft("locked".into());
    drain(cx, &activity);
    let first = calls.try_recv().unwrap();
    assert!(calls.is_empty());
    activity.select_channel("2");
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), page(&[]));
    drain(cx, &activity);
    activity.edit_draft("second draft".into());
    activity.send();
    drain(cx, &activity);
    calls
        .try_recv()
        .unwrap()
        .reply
        .try_send(Ok(Response::controlled(
            StatusCode::CREATED,
            message("20", "2").to_string(),
        )))
        .unwrap();
    drain(cx, &activity);
    assert_eq!(activity.read().draft("2"), "");
    cx.background_executor.advance_clock(Duration::from_secs(8));
    drain(cx, &activity);
    assert!(activity.read().send_pending.contains_key("1"));
    cx.background_executor.advance_clock(Duration::from_secs(1));
    drain(cx, &activity);
    assert!(!activity.read().send_pending.contains_key("1"));
    assert_eq!(activity.read().draft("1"), "first draft");
    assert!(first.reply.is_closed());
    activity.select_channel("1");
    stream.message("9", "1");
    drain(cx, &activity);
    assert_eq!(activity.read().draft("1"), "first draft");
    assert!(activity.read().send_feedback["1"].contains("may already"));
    assert!(calls.is_empty());
}
