use super::live_support::*;
use super::*;
use crate::api::test_support::Response;
use reqwest::StatusCode;

#[gpui_kit::test]
fn live_deletion_duplicate_late_deliveries_dialog_outcomes_and_session_guards(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, _, stream) = ready(cx);
    activity.edit_draft("discard".into());
    activity.send();
    drain(cx, &activity);
    let send = calls.try_recv().unwrap();
    activity.rename_channel("1", "renamed");
    drain(cx, &activity);
    let rename = calls.try_recv().unwrap();
    activity.delete_channel("1");
    drain(cx, &activity);
    let delete = calls.try_recv().unwrap();
    stream.frame(
        "change",
        serde_json::json!({"type":"channel_deleted","channel_id":"1"}),
    );
    drain(cx, &activity);
    let fallback = calls.try_recv().unwrap();
    assert_eq!(fallback.request.url().path(), "/api/v1/channels/2/messages");
    assert_eq!(activity.read().selected.as_deref(), Some("2"));
    assert!(activity.read().delete_pending);
    assert!(activity.read().rename_pending);
    assert_eq!(activity.read().delete_confirmed, 0);
    assert!(activity.read().drafts.is_empty());
    assert!(activity.read().send_pending.is_empty());
    for data in [
        serde_json::json!({"type":"channel_deleted","channel_id":"1"}),
        serde_json::json!({"type":"channel_renamed","channel":{"id":"1","name":"late","type":"text"}}),
        serde_json::json!({"type":"channel_created","channel":{"id":"1","name":"late","type":"text"}}),
    ] {
        stream.frame("change", data);
    }
    stream.message("9", "1");
    drain(cx, &activity);
    assert!(
        calls.is_empty(),
        "duplicate deletion causes no second navigation/read"
    );
    // The canceled send cannot allocate confirmation state after deletion.
    let _ = send.reply.try_send(Ok(Response::controlled(
        StatusCode::CREATED,
        message("9", "1").to_string(),
    )));
    rename
        .reply
        .try_send(Ok(Response::controlled(StatusCode::NOT_FOUND, "")))
        .unwrap();
    delete.reply.try_send(Err(ApiError::Unavailable)).unwrap();
    respond(fallback, serde_json::json!({"items":[],"next_cursor":null}));
    drain(cx, &activity);
    assert!(
        activity
            .read()
            .rename_feedback
            .as_ref()
            .unwrap()
            .contains("not found")
    );
    assert!(
        activity
            .read()
            .delete_feedback
            .as_ref()
            .unwrap()
            .contains("may have succeeded")
    );
    assert_eq!(activity.read().delete_confirmed, 0);
    assert!(!activity.read().history.contains_key("1"));
    assert!(
        matches!(&activity.read().channels, Some(Load::Ready(channels)) if channels.len() == 1 && channels[0].id == "2")
    );
    assert!(calls.is_empty());
    // No known replacement and a late history result cannot resurrect selection.
    activity.select_channel("2");
    stream.frame(
        "change",
        serde_json::json!({"type":"channel_deleted","channel_id":"2"}),
    );
    drain(cx, &activity);
    assert!(activity.read().selected.is_none());
    assert!(activity.read().history.is_empty());
    assert!(calls.is_empty());
    let old = activity.0.borrow().live.attempt();
    activity.close();
    let (replacement, replacement_calls, replacement_streams) = fixture_for_session(cx, 8);
    replacement.start();
    drain(cx, &replacement);
    let replacement_stream = replacement_streams.try_recv().unwrap();
    replacement_stream.ready();
    drain(cx, &replacement);
    replacement.apply(ChatUpdate(Update::Stream(
        old,
        Ok(LiveEvent::ChannelDeleted("2".into())),
    )));
    respond(replacement_calls.try_recv().unwrap(), channels());
    drain(cx, &replacement);
    assert!(
        matches!(&replacement.read().channels, Some(Load::Ready(channels)) if channels.len() == 2),
        "old-session deletion cannot tombstone the new session"
    );
    replacement.close();
}

#[gpui_kit::test]
fn remote_delete_rejects_held_history_and_successful_late_rename(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, _, stream) = ready(cx);
    activity.select_channel("2");
    drain(cx, &activity);
    let history = calls.try_recv().unwrap();
    activity.edit_draft("discard".into());
    activity.rename_channel("2", "aaa");
    drain(cx, &activity);
    let rename = calls.try_recv().unwrap();
    stream.frame(
        "change",
        serde_json::json!({"type":"channel_deleted","channel_id":"2"}),
    );
    drain(cx, &activity);
    assert_eq!(activity.read().selected.as_deref(), Some("1"));
    assert!(activity.read().rename_pending);
    let _ = history.reply.try_send(Ok(Response::controlled(
        StatusCode::OK,
        serde_json::json!({"items":[message("9", "2")],"next_cursor":"cursor"}).to_string(),
    )));
    rename
        .reply
        .try_send(Ok(Response::controlled(
            StatusCode::OK,
            r#"{"id":"2","name":"aaa","type":"text"}"#,
        )))
        .unwrap();
    drain(cx, &activity);
    assert_eq!(activity.read().rename_confirmed, 0);
    assert!(
        activity
            .read()
            .rename_feedback
            .as_ref()
            .unwrap()
            .contains("not found")
    );
    assert!(!activity.read().history.contains_key("2"));
    assert!(!activity.read().drafts.contains_key("2"));
    assert!(!activity.read().older.contains_key("2"));
    assert!(
        calls.is_empty(),
        "cached fallback and late outcomes cause no reads/replay"
    );
    activity.close();
}

#[gpui_kit::test]
fn deletion_during_initial_snapshot_filters_late_snapshot_and_create_confirmation(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (activity, calls, streams) = fixture(cx);
    activity.start();
    drain(cx, &activity);
    let stream = streams.try_recv().unwrap();
    stream.ready();
    stream.frame(
        "change",
        serde_json::json!({"type":"channel_deleted","channel_id":"1"}),
    );
    drain(cx, &activity);
    respond(calls.try_recv().unwrap(), channels());
    drain(cx, &activity);
    let history = calls.try_recv().unwrap();
    assert_eq!(history.request.url().path(), "/api/v1/channels/2/messages");
    respond(history, serde_json::json!({"items":[],"next_cursor":null}));
    drain(cx, &activity);
    activity.create_channel("new");
    drain(cx, &activity);
    let create = calls.try_recv().unwrap();
    stream.frame(
        "change",
        serde_json::json!({"type":"channel_deleted","channel_id":"3"}),
    );
    drain(cx, &activity);
    create
        .reply
        .try_send(Ok(Response::controlled(
            StatusCode::CREATED,
            r#"{"id":"3","name":"new","type":"text"}"#,
        )))
        .unwrap();
    drain(cx, &activity);
    assert_eq!(activity.read().selected.as_deref(), Some("2"));
    assert!(calls.is_empty());
    assert!(activity.created().is_none());
    assert!(
        matches!(&activity.read().channels, Some(Load::Ready(channels)) if channels.len() == 1)
    );
    activity.close();
}
