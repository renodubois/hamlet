//! Request/event ordering, failure, and lifetime behavior through Chat intentions.
use super::*;
use gpui_kit::TestAppContext;
use live_support::*;
use message_delete_support::{ids, ready};

fn success(call: Call) {
    call.reply
        .try_send(Ok(crate::api::test_support::Response::controlled(
            reqwest::StatusCode::NO_CONTENT,
            "",
        )))
        .unwrap();
}

#[gpui_kit::test]
fn event_and_http_orders_duplicates_and_late_failure_cannot_reopen_confirmation(
    cx: &mut TestAppContext,
) {
    for event_first in [false, true] {
        for outcome in [Ok(()), Err(ApiError::NotFound), Err(ApiError::Unavailable)] {
            let (chat, calls, _, stream) = ready(cx);
            assert!(chat.open_message_deletion("000000000000001", "000000000000008"));
            chat.confirm_message_deletion();
            drain(cx, &chat);
            let call = calls.try_recv().unwrap();
            if event_first {
                deleted(&stream, "000000000000001", "000000000000008");
                drain(cx, &chat);
            }
            match outcome {
                Ok(()) => success(call),
                Err(error) => call.reply.try_send(Err(error)).unwrap(),
            }
            drain(cx, &chat);
            if !event_first {
                deleted(&stream, "000000000000001", "000000000000008");
                drain(cx, &chat);
            }
            deleted(&stream, "000000000000001", "000000000000008");
            stream.message("000000000000008", "000000000000001");
            drain(cx, &chat);
            assert!(ids(&chat).is_empty());
            assert!(chat.read().message_deletion.is_none());
            assert!(calls.is_empty());
            chat.close();
        }
    }
}

#[gpui_kit::test]
fn definite_failure_uncertainty_timeout_and_reconnect_allow_only_deliberate_retry(
    cx: &mut TestAppContext,
) {
    let (chat, calls, streams, stream) = ready(cx);
    assert!(chat.open_message_deletion("000000000000001", "000000000000008"));
    for (error, feedback) in [
        (ApiError::Forbidden, "own messages"),
        (ApiError::NotFound, "not found"),
        (ApiError::InvalidInput, "rejected"),
        (ApiError::ServerFailure, "could not delete"),
        (ApiError::Unavailable, "may have succeeded"),
        (ApiError::InvalidResponse, "may have succeeded"),
    ] {
        chat.confirm_message_deletion();
        drain(cx, &chat);
        let call = calls.try_recv().unwrap();
        call.reply.try_send(Err(error)).unwrap();
        drain(cx, &chat);
        let state = chat.read();
        let dialog = state.message_deletion.as_ref().unwrap();
        assert!(!dialog.pending);
        assert!(dialog.feedback.as_ref().unwrap().contains(feedback));
        assert_eq!(ids(&chat), ["000000000000008"]);
        assert!(calls.is_empty());
    }
    chat.confirm_message_deletion();
    drain(cx, &chat);
    let timeout = calls.try_recv().unwrap();
    cx.background_executor
        .advance_clock(Duration::from_secs(10));
    drain(cx, &chat);
    assert!(
        chat.read()
            .message_deletion
            .as_ref()
            .unwrap()
            .feedback
            .as_ref()
            .unwrap()
            .contains("may have succeeded")
    );
    assert!(
        timeout
            .reply
            .try_send(Ok(crate::api::test_support::Response::controlled(
                reqwest::StatusCode::NO_CONTENT,
                ""
            )))
            .is_err()
    );
    stream.body.close();
    drain(cx, &chat);
    cx.background_executor.advance_clock(Duration::from_secs(3));
    drain(cx, &chat);
    streams.try_recv().unwrap().ready();
    drain(cx, &chat);
    assert!(calls.is_empty(), "no automatic retries or catch-up reads");
    chat.confirm_message_deletion();
    drain(cx, &chat);
    success(calls.try_recv().unwrap());
    drain(cx, &chat);
    assert!(ids(&chat).is_empty());
    assert!(chat.read().message_deletion.is_none());
}

#[gpui_kit::test]
fn captured_target_removal_session_end_and_late_outcomes_leave_no_orphan(cx: &mut TestAppContext) {
    for pending in [false, true] {
        for channel_removed in [false, true] {
            let (chat, calls, _, stream) = ready(cx);
            assert!(chat.open_message_deletion("000000000000001", "000000000000008"));
            let call = if pending {
                chat.confirm_message_deletion();
                drain(cx, &chat);
                Some(calls.try_recv().unwrap())
            } else {
                None
            };
            if channel_removed {
                stream.frame(
                    "change",
                    serde_json::json!({"type":"channel_deleted","channel_id":"000000000000001"}),
                );
                drain(cx, &chat);
                respond(calls.try_recv().unwrap(), serde_json::json!({"items":[]}));
                drain(cx, &chat);
                assert_eq!(chat.read().selected.as_deref(), Some("000000000000002"));
            } else {
                chat.close();
            }
            assert!(chat.read().message_deletion.is_none());
            if let Some(call) = call {
                let _ = call
                    .reply
                    .try_send(Ok(crate::api::test_support::Response::controlled(
                        reqwest::StatusCode::NO_CONTENT,
                        "",
                    )));
                drain(cx, &chat);
            }
            assert!(chat.read().message_deletion.is_none());
            assert!(calls.is_empty());
        }
    }
}

#[gpui_kit::test]
fn authenticated_rejection_after_live_success_still_invalidates_originating_session(
    cx: &mut TestAppContext,
) {
    let (chat, calls, _, stream) = ready(cx);
    assert!(chat.open_message_deletion("000000000000001", "000000000000008"));
    chat.confirm_message_deletion();
    drain(cx, &chat);
    let call = calls.try_recv().unwrap();
    deleted(&stream, "000000000000001", "000000000000008");
    drain(cx, &chat);
    call.reply.try_send(Err(ApiError::SessionRejected)).unwrap();
    cx.executor().run_until_parked();
    assert_eq!(
        chat.apply(chat.updates().try_recv().unwrap()),
        Some(SessionEnd::Rejected(7))
    );
    assert!(chat.read().message_deletion.is_none());
    assert!(chat.read().channels.is_none());
}

#[gpui_kit::test]
fn repeated_http_deliveries_cannot_change_a_new_confirmation_or_deliberate_retry(
    cx: &mut TestAppContext,
) {
    let (chat, calls, _, stream) = ready(cx);
    assert!(chat.open_message_deletion("000000000000001", "000000000000008"));
    chat.confirm_message_deletion();
    drain(cx, &chat);
    success(calls.try_recv().unwrap());
    cx.executor().run_until_parked();
    let outcome = chat.updates().try_recv().unwrap();
    let replay = outcome.clone();
    assert!(chat.apply(outcome).is_none());
    assert!(chat.apply(replay.clone()).is_none());
    assert!(chat.read().message_deletion.is_none());
    stream.message("000000000000009", "000000000000001");
    drain(cx, &chat);
    assert!(chat.open_message_deletion("000000000000001", "000000000000009"));
    chat.confirm_message_deletion();
    drain(cx, &chat);
    assert!(chat.apply(replay).is_none());
    assert!(chat.read().message_deletion.as_ref().unwrap().pending);
    calls
        .try_recv()
        .unwrap()
        .reply
        .try_send(Err(ApiError::Unavailable))
        .unwrap();
    cx.executor().run_until_parked();
    let failure = chat.updates().try_recv().unwrap();
    let replay = failure.clone();
    assert!(chat.apply(failure).is_none());
    chat.confirm_message_deletion();
    drain(cx, &chat);
    assert!(chat.apply(replay).is_none());
    let state = chat.read();
    let dialog = state.message_deletion.as_ref().unwrap();
    assert!(dialog.pending);
    assert!(dialog.feedback.is_none());
    drop(state);
    success(calls.try_recv().unwrap());
    drain(cx, &chat);
    assert!(chat.read().message_deletion.is_none());
    assert!(ids(&chat).is_empty());
}
