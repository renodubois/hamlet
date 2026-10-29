//! Confirmation intentions at the Chat request/event seam.
use super::*;
use gpui_kit::TestAppContext;
use live_support::*;
use message_delete_support::{ids, ready};

#[gpui_kit::test]
fn captured_confirmation_is_request_free_until_confirmed_and_http_removal_blocks_resurrection(
    cx: &mut TestAppContext,
) {
    let (chat, calls, _, stream) = ready(cx);
    assert!(!chat.open_message_deletion("000000000000001", "missing"));
    assert!(chat.open_message_deletion("000000000000001", "000000000000008"));
    assert_eq!(
        chat.read().message_deletion.as_ref().unwrap().preview,
        "same text"
    );
    assert!(calls.is_empty());
    chat.cancel_message_deletion();
    assert!(chat.read().message_deletion.is_none());
    assert!(calls.is_empty());
    assert!(chat.open_message_deletion("000000000000001", "000000000000008"));
    chat.select_channel("000000000000002");
    drain(cx, &chat);
    respond(calls.try_recv().unwrap(), serde_json::json!({"items":[]}));
    drain(cx, &chat);
    chat.confirm_message_deletion();
    chat.confirm_message_deletion();
    chat.cancel_message_deletion();
    drain(cx, &chat);
    let call = calls.try_recv().unwrap();
    assert_eq!(call.request.method(), reqwest::Method::DELETE);
    assert_eq!(
        call.request.url().path(),
        "/api/v1/channels/000000000000001/messages/000000000000008"
    );
    assert_eq!(call.request.headers()["authorization"], "Bearer synthetic");
    assert!(call.request.body().is_none());
    assert!(calls.is_empty());
    assert_eq!(ids(&chat), ["000000000000008"]);
    assert!(chat.read().message_deletion.as_ref().unwrap().pending);
    call.reply
        .try_send(Ok(crate::api::test_support::Response::controlled(
            reqwest::StatusCode::NO_CONTENT,
            "",
        )))
        .unwrap();
    drain(cx, &chat);
    assert!(ids(&chat).is_empty());
    assert!(chat.read().message_deletion.is_none());
    stream.message("000000000000008", "000000000000001");
    drain(cx, &chat);
    assert!(ids(&chat).is_empty());
    assert!(calls.is_empty());
}
