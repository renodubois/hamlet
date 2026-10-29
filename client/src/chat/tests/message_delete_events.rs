//! Observed message deletion through the session-owned Chat request/event seam.
use super::*;
use gpui_kit::TestAppContext;
use live_support::*;

const A: &str = "000000000000001";
const B: &str = "000000000000002";
const C: &str = "000000000000003";
const M: &str = "000000000000008";
const N: &str = "000000000000009";

fn channel_list() -> serde_json::Value {
    serde_json::json!({"items": [
        {"id":A,"name":"General","type":"text"},
        {"id":B,"name":"Other","type":"text"},
        {"id":C,"name":"Unloaded","type":"text"}
    ]})
}
fn history(channel: &str, ids: &[&str], cursor: Option<&str>) -> serde_json::Value {
    serde_json::json!({"items":ids.iter().map(|id| message(id, channel)).collect::<Vec<_>>(),"next_cursor":cursor})
}
fn displayed(chat: &ChatHandle, channel: &str) -> Vec<String> {
    let state = chat.read();
    let Some(Load::Ready(rows)) = state.history(channel) else {
        panic!("loaded history")
    };
    rows.iter().map(|m| m.id.clone()).collect()
}
fn started(
    cx: &mut TestAppContext,
) -> (
    ChatHandle,
    async_channel::Receiver<Call>,
    async_channel::Receiver<Stream>,
    Stream,
) {
    let (chat, calls, streams) = fixture(cx);
    chat.start();
    drain(cx, &chat);
    let stream = streams.try_recv().unwrap();
    stream.ready();
    respond(calls.try_recv().unwrap(), channel_list());
    drain(cx, &chat);
    (chat, calls, streams, stream)
}

#[gpui_kit::test]
fn observed_deletions_block_late_newest_older_and_creation_events(cx: &mut TestAppContext) {
    let (chat, calls, _, stream) = started(cx);
    let newest = calls.try_recv().unwrap();
    deleted(&stream, A, M);
    deleted(&stream, C, M);
    drain(cx, &chat);
    assert!(chat.read().history(C).is_none());
    assert!(calls.is_empty());
    respond(newest, history(A, &[M, N], Some("opaque+/=&?")));
    drain(cx, &chat);
    stream.message(M, A);
    stream.message(M, C);
    drain(cx, &chat);
    assert_eq!(displayed(&chat, A), [N]);
    assert_eq!(chat.read().older(A), Some(&OlderPages::Available));
    chat.request_older();
    drain(cx, &chat);
    let older = calls.try_recv().unwrap();
    deleted(&stream, A, N);
    drain(cx, &chat);
    assert!(displayed(&chat, A).is_empty());
    assert_eq!(chat.read().older(A), Some(&OlderPages::Loading));
    respond(older, history(A, &[M, N], Some("next-opaque")));
    drain(cx, &chat);
    assert!(displayed(&chat, A).is_empty());
    assert_eq!(chat.read().older(A), Some(&OlderPages::Available));
    chat.select_channel(C);
    drain(cx, &chat);
    respond(calls.try_recv().unwrap(), history(C, &[M, N], None));
    drain(cx, &chat);
    assert_eq!(
        displayed(&chat, C),
        [N],
        "unloaded observations survive until ordinary history reads"
    );
    assert!(
        calls.is_empty(),
        "no removal backfill or reconciliation read"
    );
}

#[gpui_kit::test]
fn deletion_and_send_confirmation_orders_never_restore_loaded_rows_or_confirmation_overlays(
    cx: &mut TestAppContext,
) {
    for loaded in [false, true] {
        for deletion_first in [false, true] {
            let (chat, calls, _, stream) = started(cx);
            let newest = calls.try_recv().unwrap();
            if loaded {
                respond(newest, history(A, &[], None));
                drain(cx, &chat);
            } else {
                newest.reply.try_send(Err(ApiError::Unavailable)).unwrap();
                drain(cx, &chat);
            }
            chat.edit_draft("published text".into());
            chat.send();
            drain(cx, &chat);
            let send = calls.try_recv().unwrap();
            // The originating send completes while another channel is selected.
            chat.select_channel(B);
            drain(cx, &chat);
            respond(calls.try_recv().unwrap(), history(B, &[], None));
            drain(cx, &chat);
            if deletion_first {
                deleted(&stream, A, M);
                drain(cx, &chat);
            }
            send.reply
                .try_send(Ok(crate::api::test_support::Response::controlled(
                    reqwest::StatusCode::CREATED,
                    message(M, A).to_string(),
                )))
                .unwrap();
            drain(cx, &chat);
            if !deletion_first {
                deleted(&stream, A, M);
                drain(cx, &chat);
            }
            stream.message(M, A);
            drain(cx, &chat);
            assert!(!chat.read().is_sending(A));
            assert_eq!(
                chat.read().draft(A),
                "",
                "publication remains confirmed even when deleted"
            );
            assert!(chat.read().send_feedback(A).is_none());
            chat.select_channel(A);
            drain(cx, &chat);
            if !loaded {
                respond(calls.try_recv().unwrap(), history(A, &[M, N], None));
                drain(cx, &chat);
                assert_eq!(
                    displayed(&chat, A),
                    [N],
                    "neither late snapshot nor confirmation overlay resurrects"
                );
            } else {
                assert!(displayed(&chat, A).is_empty());
            }
            assert!(calls.is_empty(), "no automatic read or write retry");
            chat.close();
        }
    }
}

#[gpui_kit::test]
fn observations_survive_reconnect_without_reads_but_end_with_the_chat_lifetime(
    cx: &mut TestAppContext,
) {
    let (chat, calls, streams) = fixture(cx);
    chat.start();
    drain(cx, &chat);
    let stream = streams.try_recv().unwrap();
    stream.ready();
    // The message and even its channel are not loaded when deletion is observed.
    deleted(&stream, A, M);
    drain(cx, &chat);
    respond(calls.try_recv().unwrap(), channel_list());
    drain(cx, &chat);
    respond(
        calls.try_recv().unwrap(),
        history(A, &[M, N], Some("retained-cursor")),
    );
    drain(cx, &chat);
    assert_eq!(displayed(&chat, A), [N]);
    stream.body.close();
    drain(cx, &chat);
    cx.background_executor.advance_clock(Duration::from_secs(3));
    drain(cx, &chat);
    let replacement = streams.try_recv().unwrap();
    replacement.ready();
    replacement.message(M, A);
    drain(cx, &chat);
    assert_eq!(displayed(&chat, A), [N]);
    assert_eq!(chat.read().older(A), Some(&OlderPages::Available));
    assert!(
        calls.is_empty(),
        "reconnect and readiness are not synchronization"
    );
    // A delivery already queued at session end must not alter the cleared Chat.
    deleted(&replacement, A, N);
    cx.executor().run_until_parked();
    let late = chat.updates().try_recv().unwrap();
    chat.close();
    assert!(chat.apply(late).is_none());
    assert!(
        chat.read()
            .conversations()
            .values()
            .all(|c| c.history.is_none())
    );
    cx.executor().run_until_parked();
    assert!(replacement.body.is_closed());
    let (fresh, calls, _, _) = started(cx);
    respond(calls.try_recv().unwrap(), history(A, &[M, N], None));
    drain(cx, &fresh);
    assert_eq!(
        displayed(&fresh, A),
        [N, M],
        "observations do not leak into a new chat lifetime"
    );
}

#[gpui_kit::test]
fn deletion_removes_active_and_inactive_rows_without_reads_or_unloaded_allocation(
    cx: &mut TestAppContext,
) {
    let (chat, calls, _, stream) = started(cx);
    respond(
        calls.try_recv().unwrap(),
        history(A, &[M, N], Some("cursor-for-deleted-row")),
    );
    drain(cx, &chat);
    chat.select_channel(B);
    drain(cx, &chat);
    respond(calls.try_recv().unwrap(), history(B, &[M], None));
    drain(cx, &chat);
    chat.edit_draft("preserve composer".into());
    deleted(&stream, A, M);
    deleted(&stream, B, M);
    deleted(&stream, C, M);
    deleted(&stream, A, M);
    drain(cx, &chat);
    assert_eq!(displayed(&chat, A), [N]);
    assert!(displayed(&chat, B).is_empty());
    assert!(chat.read().history(C).is_none());
    assert_eq!(chat.read().draft(B), "preserve composer");
    assert_eq!(chat.read().selected.as_deref(), Some(B));
    assert_eq!(chat.read().older(A), Some(&OlderPages::Available));
    assert!(calls.is_empty(), "removal must not read or backfill");
    chat.select_channel(A);
    chat.request_older();
    drain(cx, &chat);
    let older = calls.try_recv().unwrap();
    assert_eq!(
        older
            .request
            .url()
            .query_pairs()
            .find(|(key, _)| key == "before")
            .unwrap()
            .1,
        "cursor-for-deleted-row"
    );
}
