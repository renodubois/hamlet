use super::*;
use live_support::*;

#[gpui_kit::test]
fn queued_a_to_b_to_a_rejection_cannot_settle_new_history_within_same_attempt(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams) = fixture(cx);
    activity.start();
    drain(cx, &activity);
    let stream = streams.try_recv().unwrap();
    stream.ready();
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), channels());
    drain(cx, &activity);
    calls
        .try_recv()
        .unwrap()
        .reply
        .try_send(Ok(crate::api::test_support::Response::controlled(
            reqwest::StatusCode::UNAUTHORIZED,
            r#"{"error":{"code":"unauthorized"}}"#,
        )))
        .unwrap();
    cx.executor().run_until_parked();
    let obsolete = activity.updates().try_recv().unwrap();
    activity.select_channel("2");
    drain(cx, &activity);
    let b = calls.try_recv().unwrap();
    activity.select_channel("1");
    drain(cx, &activity);
    let a = calls.try_recv().unwrap();
    assert!(activity.apply(obsolete).is_none());
    assert!(b.reply.is_closed());
    assert_eq!(activity.status(), "Connecting…");
    respond(a, page(&["8"]));
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["8"]);
    assert_eq!(activity.status(), "");
    assert!(!stream.body.is_closed());
    assert!(streams.is_empty());
    assert!(calls.is_empty());
}

#[gpui_kit::test]
fn initial_readiness_and_baseline_failures_keep_minimal_connecting_state(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams) = fixture(cx);
    activity.start();
    drain(cx, &activity);
    let timed_out = streams.try_recv().unwrap();
    cx.background_executor.advance_clock(Duration::from_secs(8));
    drain(cx, &activity);
    assert!(timed_out.body.is_closed());
    assert!(calls.is_empty());
    assert_eq!(activity.status(), "Connecting…");
    cx.background_executor.advance_clock(Duration::from_secs(1));
    drain(cx, &activity);
    let retry = streams.try_recv().unwrap();
    retry.ready();
    drain(cx, &activity);
    calls
        .try_recv()
        .unwrap()
        .reply
        .try_send(Err(ApiError::Unavailable))
        .unwrap();
    drain(cx, &activity);
    assert_eq!(activity.status(), "Connecting…");
    assert!(
        matches!(activity.read().channels, Some(Load::Loading)),
        "resource-specific failure must not leak into recovery UI"
    );
    assert!(retry.body.is_closed());
}

#[gpui_kit::test]
fn terminal_failures_and_required_reads_share_retry_but_older_failure_stays_local(
    cx: &mut gpui_kit::TestAppContext,
) {
    for failure in ["eof", "malformed", "transport", "idle", "selection"] {
        let (activity, calls, streams, stream) = ready(cx);
        activity.edit_draft("keep".into());
        match failure {
            "eof" => {
                stream.body.close();
            }
            "malformed" => {
                stream.frame("change", serde_json::json!({"type":"message_created"}));
            }
            "transport" => {
                stream.body.try_send(Err(ApiError::Unavailable)).unwrap();
            }
            "idle" => cx
                .background_executor
                .advance_clock(Duration::from_secs(45)),
            _ => {
                activity.select_channel("2");
                drain(cx, &activity);
                calls
                    .try_recv()
                    .unwrap()
                    .reply
                    .try_send(Err(ApiError::Unavailable))
                    .unwrap();
            }
        }
        drain(cx, &activity);
        assert_eq!(
            activity.status(),
            "Reconnecting… messages may be out of date",
            "{failure}"
        );
        assert_eq!(activity.read().draft("1"), "keep");
        assert!(stream.body.is_closed());
        assert!(calls.is_empty());
        assert!(streams.is_empty());
        cx.background_executor.advance_clock(Duration::from_secs(1));
        drain(cx, &activity);
        let retry = streams.try_recv().unwrap();
        assert!(calls.is_empty(), "every retry must wait for readiness");
        retry.ready();
        drain(cx, &activity);
        assert_eq!(
            calls.try_recv().unwrap().request.url().path(),
            "/api/v1/channels"
        );
        activity.close();
        drain(cx, &activity);
    }
    let (activity, calls, streams) = fixture(cx);
    activity.start();
    drain(cx, &activity);
    let stream = streams.try_recv().unwrap();
    stream.ready();
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), channels());
    drain(cx, &activity);
    let mut first = page(&["8"]);
    first["next_cursor"] = serde_json::json!("opaque older");
    respond(calls.try_recv().unwrap(), first);
    drain(cx, &activity);
    activity.request_older();
    drain(cx, &activity);
    let older = calls.try_recv().unwrap();
    older.reply.try_send(Err(ApiError::Unavailable)).unwrap();
    drain(cx, &activity);
    assert!(matches!(
        activity.read().older.get("1"),
        Some(Older::Failed(_))
    ));
    assert_eq!(activity.status(), "");
    assert!(!stream.body.is_closed());
    assert!(streams.is_empty());
    activity.retry_older();
    drain(cx, &activity);
    let retry = calls.try_recv().unwrap();
    assert_eq!(
        retry
            .request
            .url()
            .query_pairs()
            .find(|(key, _)| key == "before")
            .unwrap()
            .1,
        "opaque older"
    );
    stream.message("9", "1");
    respond(retry, page(&["8", "7"]));
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["9", "8", "7"]);
    assert_eq!(activity.read().older.get("1"), Some(&Older::Exhausted));
}

#[gpui_kit::test]
fn new_http_writes_remain_available_during_outage_without_premature_reads(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams, stream) = ready(cx);
    stream.body.close();
    drain(cx, &activity);
    activity.edit_draft("outage message".into());
    activity.send();
    activity.create_channel("Outage room");
    drain(cx, &activity);
    for _ in 0..2 {
        let call = calls.try_recv().unwrap();
        assert_eq!(call.request.method(), reqwest::Method::POST);
        let entity = if call.request.url().path().ends_with("messages") {
            message("9", "1")
        } else {
            serde_json::json!({"id":"3","name":"Outage room","type":"text"})
        };
        call.reply
            .try_send(Ok(crate::api::test_support::Response::controlled(
                reqwest::StatusCode::CREATED,
                entity.to_string(),
            )))
            .unwrap();
    }
    drain(cx, &activity);
    assert_eq!(activity.read().draft("1"), "");
    assert_eq!(activity.read().selected.as_deref(), Some("3"));
    assert!(activity.created().is_some());
    assert!(
        !activity.read().history.contains_key("3"),
        "deferred read must not remain stuck Loading"
    );
    assert!(calls.is_empty());
    cx.background_executor.advance_clock(Duration::from_secs(1));
    drain(cx, &activity);
    let retry = streams.try_recv().unwrap();
    retry.ready();
    drain(cx, &activity);
    let mut list = channels();
    list["items"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"id":"3","name":"Outage room","type":"text"}));
    respond(calls.try_recv().unwrap(), list);
    drain(cx, &activity);
    let selected = calls.try_recv().unwrap();
    assert_eq!(selected.request.url().path(), "/api/v1/channels/3/messages");
    respond(selected, page(&[]));
    drain(cx, &activity);
    assert_eq!(activity.status(), "");
    activity.select_channel("1");
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), page(&["8"]));
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["9", "8"]);
    assert!(calls.is_empty());
    assert!(streams.is_empty());
}

#[gpui_kit::test]
fn uncached_read_staging_overflow_abandons_live_attempt_before_read_completion(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams, stream) = ready(cx);
    activity.select_channel("2");
    drain(cx, &activity);
    let selected = calls.try_recv().unwrap();
    for id in 1..=257 {
        stream.message(&id.to_string(), "2");
        drain(cx, &activity);
    }
    assert_eq!(
        activity.status(),
        "Reconnecting… messages may be out of date"
    );
    assert!(selected.reply.is_closed());
    assert!(stream.body.is_closed());
    assert!(streams.is_empty(), "overflow uses the common delayed retry");
}

#[gpui_kit::test]
fn uncertain_send_does_not_read_retry_or_match_text_and_confirmation_merges_in_either_order(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams, stream) = ready(cx);
    for event_first in [true, false] {
        activity.edit_draft("same text".into());
        activity.send();
        drain(cx, &activity);
        let write = calls.try_recv().unwrap();
        let id = if event_first { "9" } else { "10" };
        if event_first {
            stream.message(id, "1");
            drain(cx, &activity);
        }
        assert!(activity.read().send_pending.contains_key("1"));
        write
            .reply
            .try_send(Ok(crate::api::test_support::Response::controlled(
                reqwest::StatusCode::CREATED,
                message(id, "1").to_string(),
            )))
            .unwrap();
        drain(cx, &activity);
        if !event_first {
            stream.message(id, "1");
            drain(cx, &activity);
        }
        assert_eq!(activity.read().draft("1"), "");
        assert!(!activity.read().send_feedback.contains_key("1"));
    }
    activity.edit_draft("same text".into());
    activity.send();
    drain(cx, &activity);
    calls
        .try_recv()
        .unwrap()
        .reply
        .try_send(Err(ApiError::Unavailable))
        .unwrap();
    drain(cx, &activity);
    stream.message("11", "1");
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["11", "10", "9", "8"]);
    assert_eq!(activity.read().draft("1"), "same text");
    assert!(activity.read().send_feedback["1"].contains("may already have been published"));
    assert!(calls.is_empty());
    assert!(streams.is_empty());
    assert_eq!(activity.status(), "");
}

#[gpui_kit::test]
fn queued_obsolete_read_rejection_cannot_end_recovery_and_required_read_failure_retries(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams, stream) = ready(cx);
    activity.select_channel("2");
    drain(cx, &activity);
    let read = calls.try_recv().unwrap();
    read.reply
        .try_send(Ok(crate::api::test_support::Response::controlled(
            reqwest::StatusCode::UNAUTHORIZED,
            r#"{"error":{"code":"unauthorized"}}"#,
        )))
        .unwrap();
    cx.executor().run_until_parked();
    stream.body.close();
    cx.executor().run_until_parked();
    drain(cx, &activity); // terminal has priority over the queued rejection
    assert_eq!(
        activity.status(),
        "Reconnecting… messages may be out of date"
    );
    cx.background_executor.advance_clock(Duration::from_secs(1));
    drain(cx, &activity);
    let retry = streams.try_recv().unwrap();
    retry.ready();
    drain(cx, &activity);
    calls
        .try_recv()
        .unwrap()
        .reply
        .try_send(Err(ApiError::Unavailable))
        .unwrap();
    drain(cx, &activity);
    assert!(retry.body.is_closed());
    assert_eq!(activity.read().selected.as_deref(), Some("2"));
    cx.background_executor.advance_clock(Duration::from_secs(1));
    drain(cx, &activity);
    let next = streams.try_recv().unwrap();
    next.ready();
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), serde_json::json!({"items":[]}));
    drain(cx, &activity);
    assert_eq!(activity.status(), "");
    assert!(activity.read().selected.is_none());
    assert!(
        calls.is_empty(),
        "empty channel baseline requires no history"
    );
}

#[gpui_kit::test]
fn executor_and_recovery_overflow_abandon_attempt_without_applying_queued_entities(
    cx: &mut gpui_kit::TestAppContext,
) {
    for bridge in [false, true] {
        let (activity, calls, streams, stream) = ready(cx);
        let active = if bridge {
            stream
        } else {
            stream.body.close();
            drain(cx, &activity);
            cx.background_executor.advance_clock(Duration::from_secs(1));
            drain(cx, &activity);
            let retry = streams.try_recv().unwrap();
            retry.ready();
            drain(cx, &activity);
            // Hold the channel baseline while the recovery buffer fills.
            assert_eq!(calls.len(), 1);
            retry
        };
        for n in 20..277 {
            active.message(&n.to_string(), "1");
            cx.executor().run_until_parked();
            if !bridge {
                drain(cx, &activity);
            }
        }
        drain(cx, &activity);
        assert_eq!(ids(&activity), ["8"]);
        assert_eq!(
            activity.status(),
            "Reconnecting… messages may be out of date"
        );
        assert!(active.body.is_closed());
        activity.close();
        drain(cx, &activity);
    }
}

#[gpui_kit::test]
fn readiness_precedes_baseline_and_buffered_creations_reconcile_without_followup_reads(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams) = fixture(cx);
    activity.start();
    drain(cx, &activity);
    assert!(calls.is_empty(), "no snapshot before stream readiness");
    let stream = streams.try_recv().expect("one session stream");
    assert_eq!(stream.request.url().path(), "/api/v1/events");
    assert_eq!(
        stream.request.headers()["authorization"],
        "Bearer synthetic"
    );
    assert_eq!(activity.status(), "Connecting…");
    stream.ready();
    drain(cx, &activity);
    let list = calls.try_recv().unwrap();
    assert_eq!(list.request.url().path(), "/api/v1/channels");
    stream.message("9", "1");
    drain(cx, &activity);
    respond(list, channels());
    drain(cx, &activity);
    let history = calls.try_recv().unwrap();
    stream.message("10", "1");
    drain(cx, &activity);
    respond(history, page(&["9", "8"]));
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["10", "9", "8"]);
    assert_eq!(activity.status(), "");
    stream.message("11", "1");
    stream.message("12", "2");
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["11", "10", "9", "8"]);
    assert!(!activity.read().history.contains_key("2"));
    activity.clone().start();
    cx.background_executor
        .advance_clock(Duration::from_secs(16));
    drain(cx, &activity);
    assert!(
        calls.is_empty(),
        "healthy live changes and repeated start never read"
    );
    assert!(streams.is_empty(), "clones/start never reconnect");
}

#[gpui_kit::test]
fn outage_keeps_write_identity_and_retargets_only_history_then_merges_confirmation(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams) = fixture(cx);
    activity.start();
    drain(cx, &activity);
    let stream = streams.try_recv().unwrap();
    stream.ready();
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), channels());
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), page(&["8"]));
    drain(cx, &activity);
    activity.edit_draft("same text".into());
    activity.send();
    drain(cx, &activity);
    let send = calls.try_recv().unwrap();
    stream.body.close();
    drain(cx, &activity);
    assert_eq!(
        activity.status(),
        "Reconnecting… messages may be out of date"
    );
    assert_eq!(ids(&activity), ["8"]);
    assert!(activity.read().send_pending.contains_key("1"));
    activity.select_channel("2");
    activity.edit_draft("other draft".into());
    activity.select_channel("1");
    drain(cx, &activity);
    assert!(calls.is_empty(), "pre-ready navigation must defer reads");
    cx.background_executor.advance_clock(Duration::from_secs(1));
    drain(cx, &activity);
    let recovery = streams.try_recv().unwrap();
    recovery.ready();
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), channels());
    drain(cx, &activity);
    let obsolete = calls.try_recv().unwrap();
    activity.select_channel("2");
    drain(cx, &activity);
    let other = calls.try_recv().unwrap();
    assert_eq!(other.request.url().path(), "/api/v1/channels/2/messages");
    activity.select_channel("1");
    drain(cx, &activity);
    let current = calls.try_recv().unwrap();
    assert_eq!(current.request.url().path(), "/api/v1/channels/1/messages");
    assert!(obsolete.reply.is_closed());
    assert!(other.reply.is_closed());
    send.reply
        .try_send(Ok(crate::api::test_support::Response::controlled(
            reqwest::StatusCode::CREATED,
            message("10", "1").to_string(),
        )))
        .unwrap();
    drain(cx, &activity);
    assert_eq!(activity.read().draft("1"), "");
    assert_eq!(activity.read().draft("2"), "other draft");
    assert!(
        !activity.read().send_feedback.contains_key("1"),
        "confirmed entity needs no catch-up warning"
    );
    recovery.message("10", "1");
    recovery.message("9", "1");
    respond(current, page(&["8"]));
    drain(cx, &activity);
    assert_eq!(ids(&activity), ["10", "9", "8"]);
    assert_eq!(activity.status(), "");
    assert!(
        calls.is_empty(),
        "write and recovery finish without follow-up reads"
    );
    assert!(
        streams.is_empty(),
        "navigation never restarts a ready attempt"
    );
}
