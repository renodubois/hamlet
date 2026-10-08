//! Best-effort live delivery through the session-owned chat interface.
use super::*;
use live_support::*;

#[gpui_kit::test]
fn initial_reads_complete_without_stream_readiness(cx: &mut gpui_kit::TestAppContext) {
    let (activity, calls, streams) = fixture(cx);
    activity.start();
    drain(cx, &activity);
    let stream = streams.try_recv().unwrap();
    assert_eq!(stream.request.url().path(), "/api/v1/events");
    assert_eq!(
        stream.request.headers()["authorization"],
        "Bearer synthetic"
    );
    respond(
        calls.try_recv().expect("channels do not wait for Ready"),
        channels(),
    );
    drain(cx, &activity);
    respond(
        calls.try_recv().expect("history does not wait for Ready"),
        page(&["8"]),
    );
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["8"]);
    assert_eq!(activity.status(), "Connecting…");
    stream.ready();
    drain(cx, &activity);
    assert_eq!(activity.status(), "");
    activity.clone().start();
    assert!(calls.is_empty());
    assert!(streams.is_empty());
}

#[gpui_kit::test]
fn every_terminal_failure_retries_once_after_three_seconds_without_reads(
    cx: &mut gpui_kit::TestAppContext,
) {
    for failure in ["eof", "transport", "parser", "idle", "overflow"] {
        let (activity, calls, streams, mut stream) = ready(cx);
        activity.edit_draft("keep".into());
        // A non-tick-aligned failure must still retry exactly three seconds later.
        cx.background_executor
            .advance_clock(Duration::from_millis(250));
        drain(cx, &activity);
        for _ in 0..2 {
            match failure {
                "eof" => {
                    stream.body.close();
                }
                "transport" => {
                    stream.body.try_send(Err(ApiError::Unavailable)).unwrap();
                }
                "parser" => stream.frame("change", serde_json::json!({"type":"message_created"})),
                "idle" => cx
                    .background_executor
                    .advance_clock(Duration::from_secs(45)),
                "overflow" => {
                    // Let the bounded executor delivery fill, without draining it.
                    for id in 20..277 {
                        stream.message(&id.to_string(), "1");
                        cx.executor().run_until_parked();
                    }
                }
                _ => panic!("unexpected failure: {failure}"),
            }
            drain(cx, &activity);
            assert_eq!(
                activity.status(),
                "Live updates disconnected — reconnecting."
            );
            assert_eq!(
                ids(&activity),
                ["8"],
                "terminal delivery discards queued creations"
            );
            assert_eq!(activity.read().draft("1"), "keep");
            assert!(stream.body.is_closed());
            assert!(calls.is_empty());
            cx.background_executor
                .advance_clock(Duration::from_millis(2999));
            drain(cx, &activity);
            assert!(streams.is_empty(), "no early retry: {failure}");
            cx.background_executor
                .advance_clock(Duration::from_millis(1));
            drain(cx, &activity);
            stream = streams.try_recv().expect("retry at three seconds");
            assert!(streams.is_empty(), "only one retry attempt");
            assert!(calls.is_empty(), "reconnect never reads");
            stream.ready();
            drain(cx, &activity);
            assert_eq!(activity.status(), "");
            assert!(calls.is_empty(), "Ready never reloads");
        }
        activity.close();
        drain(cx, &activity);
    }
}

#[gpui_kit::test]
fn ordinary_reads_survive_stream_failure_and_read_errors_leave_the_stream_alone(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams) = fixture(cx);
    activity.start();
    drain(cx, &activity);
    let stream = streams.try_recv().unwrap();
    let list = calls.try_recv().unwrap();
    stream.body.close();
    drain(cx, &activity);
    assert!(!list.reply.is_closed());
    respond(list, channels());
    drain(cx, &activity);
    let history = calls.try_recv().unwrap();
    cx.background_executor.advance_clock(Duration::from_secs(3));
    drain(cx, &activity);
    let retry = streams.try_recv().unwrap();
    retry.ready();
    drain(cx, &activity);
    respond(history, page(&["8"]));
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["8"]);
    activity.select_channel("2");
    drain(cx, &activity);
    calls
        .try_recv()
        .unwrap()
        .reply
        .try_send(Err(ApiError::Unavailable))
        .unwrap();
    drain(cx, &activity);
    assert!(matches!(
        activity.read().history.get("2"),
        Some(Load::Failed(_))
    ));
    assert_eq!(activity.status(), "");
    assert!(!retry.body.is_closed());
    // Ordinary navigation retries this failed history, even while disconnected.
    retry.body.close();
    drain(cx, &activity);
    activity.select_channel("1");
    activity.select_channel("2");
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), page(&[]));
    drain(cx, &activity);
    assert!(matches!(
        activity.read().history.get("2"),
        Some(Load::Ready(_))
    ));
    assert!(streams.is_empty());
    assert!(calls.is_empty());

    let (failed, calls, streams) = fixture(cx);
    failed.start();
    drain(cx, &failed);
    let healthy = streams.try_recv().unwrap();
    healthy.ready();
    calls
        .try_recv()
        .unwrap()
        .reply
        .try_send(Err(ApiError::Unavailable))
        .unwrap();
    drain(cx, &failed);
    assert!(matches!(failed.read().channels, Some(Load::Failed(_))));
    assert_eq!(failed.status(), "");
    assert!(!healthy.body.is_closed());
    assert!(streams.is_empty());
    assert!(calls.is_empty());
}

#[gpui_kit::test]
fn disconnect_keeps_pending_writes_and_only_originating_confirmation_clears_inputs(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams, stream) = ready(cx);
    activity.edit_draft("same text".into());
    activity.send();
    activity.create_channel("New room");
    drain(cx, &activity);
    let first = calls.try_recv().unwrap();
    let second = calls.try_recv().unwrap();
    let (send, create) = if first.request.url().path().ends_with("/messages") {
        (first, second)
    } else {
        (second, first)
    };
    stream.message("9", "1");
    drain(cx, &activity);
    assert_eq!(activity.read().draft("1"), "same text");
    assert!(activity.read().send_pending.contains_key("1"));
    stream.body.close();
    drain(cx, &activity);
    assert!(!send.reply.is_closed());
    assert!(!create.reply.is_closed());
    assert!(activity.read().create_pending);
    cx.background_executor.advance_clock(Duration::from_secs(3));
    drain(cx, &activity);
    let retry = streams.try_recv().unwrap();
    retry.ready();
    drain(cx, &activity);
    assert!(activity.read().send_pending.contains_key("1"));
    assert!(activity.read().create_pending);
    assert!(activity.created().is_none());
    assert!(calls.is_empty());
    confirm(send, message("9", "1"));
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["9", "8"]);
    assert_eq!(activity.read().draft("1"), "");
    retry.body.close();
    drain(cx, &activity);
    confirm(
        create,
        serde_json::json!({"id":"3","name":"New room","type":"text"}),
    );
    drain(cx, &activity);
    assert_eq!(activity.created().unwrap().1, "New room");
    assert_eq!(activity.read().selected.as_deref(), Some("3"));
    let selected = calls.try_recv().unwrap();
    assert_eq!(selected.request.url().path(), "/api/v1/channels/3/messages");
    respond(selected, page(&[]));
    drain(cx, &activity);
    activity.select_channel("1");
    activity.edit_draft("uncertain text".into());
    activity.send();
    drain(cx, &activity);
    calls
        .try_recv()
        .unwrap()
        .reply
        .try_send(Err(ApiError::Unavailable))
        .unwrap();
    drain(cx, &activity);
    cx.background_executor.advance_clock(Duration::from_secs(3));
    drain(cx, &activity);
    let retry = streams.try_recv().unwrap();
    retry.ready();
    retry.message("10", "1");
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["10", "9", "8"]);
    assert_eq!(activity.read().draft("1"), "uncertain text");
    assert!(activity.read().send_feedback["1"].contains("may already have been published"));
    assert!(
        calls.is_empty(),
        "uncertainty causes neither resend nor special reads"
    );
    assert!(streams.is_empty());
}

#[gpui_kit::test]
fn replacing_reads_may_miss_events_without_buffering_or_restarting(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams) = fixture(cx);
    activity.start();
    drain(cx, &activity);
    let stream = streams.try_recv().unwrap();
    stream.ready();
    stream.frame("change", serde_json::json!({"type":"channel_created","channel":{"id":"3","name":"Overlap room","type":"text"}}));
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), channels());
    drain(cx, &activity);
    let history = calls.try_recv().unwrap();
    // Individually consumed events exceed the former recovery/read staging bounds.
    for id in 20..300 {
        stream.message(&id.to_string(), "1");
        drain(cx, &activity);
    }
    stream.message("999", "2");
    drain(cx, &activity);
    respond(history, page(&["8"]));
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["8"], "read-overlap losses are accepted");
    assert!(
        matches!(&activity.read().channels, Some(Load::Ready(channels)) if channels.len() == 2)
    );
    assert!(!activity.read().history.contains_key("2"));
    stream.message("9", "1");
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["9", "8"]);
    assert_eq!(activity.status(), "");
    assert!(!stream.body.is_closed());
    assert!(calls.is_empty());
    assert!(streams.is_empty());
}

#[gpui_kit::test]
fn obsolete_attempts_and_navigation_rejections_cannot_change_current_state(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams, stream) = ready(cx);
    stream.message("9", "1");
    cx.executor().run_until_parked();
    let obsolete_event = activity.updates().try_recv().unwrap();
    stream.body.close();
    drain(cx, &activity);
    cx.background_executor.advance_clock(Duration::from_secs(3));
    drain(cx, &activity);
    let retry = streams.try_recv().unwrap();
    retry.ready();
    drain(cx, &activity);
    assert!(activity.apply(obsolete_event).is_none());
    assert_eq!(ids(&activity), ["8"]);
    activity.select_channel("2");
    drain(cx, &activity);
    let stale = calls.try_recv().unwrap();
    reject(stale);
    cx.executor().run_until_parked();
    let obsolete_read = activity.updates().try_recv().unwrap();
    activity.select_channel("1");
    activity.select_channel("2");
    drain(cx, &activity);
    let current = calls.try_recv().unwrap();
    assert!(activity.apply(obsolete_read).is_none());
    respond(current, page(&[]));
    drain(cx, &activity);
    assert_eq!(activity.read().selected.as_deref(), Some("2"));
    assert_eq!(activity.status(), "");
    assert!(!retry.body.is_closed());
    activity.close();
    cx.background_executor
        .advance_clock(Duration::from_secs(60));
    drain(cx, &activity);
    assert!(streams.is_empty());
    assert!(calls.is_empty());
}

#[gpui_kit::test]
fn stream_failure_does_not_obsolete_authoritative_read_rejection(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams, stream) = ready(cx);
    activity.select_channel("2");
    drain(cx, &activity);
    reject(calls.try_recv().unwrap());
    stream.body.close();
    cx.executor().run_until_parked();
    // The terminal lane wins, but the independent read remains authoritative.
    assert!(
        activity
            .apply(activity.updates().try_recv().unwrap())
            .is_none()
    );
    assert_eq!(
        activity.status(),
        "Live updates disconnected — reconnecting."
    );
    assert_eq!(
        activity.apply(activity.updates().try_recv().unwrap()),
        Some(SessionEnd::Rejected(7))
    );
    cx.background_executor
        .advance_clock(Duration::from_secs(60));
    drain(cx, &activity);
    assert!(activity.read().channels.is_none());
    assert!(streams.is_empty(), "rejection cancels the pending retry");
}

#[gpui_kit::test]
fn replaced_session_rejects_old_deliveries_and_closed_retry_cannot_reopen(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (old, calls, old_streams, stream) = ready(cx);
    stream.message("9", "1");
    old.select_channel("2");
    drain(cx, &old);
    reject(calls.try_recv().unwrap());
    stream.message("10", "1");
    cx.executor().run_until_parked();
    let first = old.updates().try_recv().unwrap();
    let second = old.updates().try_recv().unwrap();
    stream.body.close();
    drain(cx, &old);
    old.close();
    let (current, calls, streams) = fixture_for_session(cx, 8);
    current.start();
    drain(cx, &current);
    let stream = streams.try_recv().unwrap();
    stream.ready();
    respond(calls.try_recv().unwrap(), channels());
    drain(cx, &current);
    respond(calls.try_recv().unwrap(), page(&["8"]));
    drain(cx, &current);
    for obsolete in [first, second] {
        assert!(current.apply(obsolete).is_none());
    }
    cx.background_executor.advance_clock(Duration::from_secs(3));
    drain(cx, &old);
    drain(cx, &current);
    assert_eq!(ids(&current), ["8"]);
    assert_eq!(current.status(), "");
    assert!(old.read().channels.is_none());
    assert!(old_streams.is_empty());
    assert!(calls.is_empty());
    assert!(streams.is_empty());
}

#[gpui_kit::test]
fn readiness_timeout_retains_independently_loaded_history_and_retries_without_gets(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams) = fixture(cx);
    activity.start();
    drain(cx, &activity);
    let opening = streams.try_recv().unwrap();
    respond(calls.try_recv().unwrap(), channels());
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), page(&["8"]));
    drain(cx, &activity);
    cx.background_executor.advance_clock(Duration::from_secs(8));
    drain(cx, &activity);
    assert!(opening.body.is_closed());
    assert_eq!(ids(&activity), ["8"]);
    assert_eq!(
        activity.status(),
        "Live updates disconnected — reconnecting."
    );
    cx.background_executor.advance_clock(Duration::from_secs(3));
    drain(cx, &activity);
    let retry = streams.try_recv().unwrap();
    retry.ready();
    drain(cx, &activity);
    assert_eq!(activity.status(), "");
    assert_eq!(ids(&activity), ["8"]);
    assert!(calls.is_empty());
    assert!(streams.is_empty());
}

fn reject(call: Call) {
    call.reply
        .try_send(Ok(crate::api::test_support::Response::controlled(
            reqwest::StatusCode::UNAUTHORIZED,
            r#"{"error":{"code":"unauthorized"}}"#,
        )))
        .unwrap();
}

fn confirm(call: Call, value: serde_json::Value) {
    call.reply
        .try_send(Ok(crate::api::test_support::Response::controlled(
            reqwest::StatusCode::CREATED,
            value.to_string(),
        )))
        .unwrap();
}
