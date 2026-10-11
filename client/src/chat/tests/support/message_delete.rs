//! Valid wire identities for message-confirmation request/event scenarios.
use super::{live_support::*, *};
use gpui_kit::TestAppContext;
pub fn ready(
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
    drain(cx, &chat);
    respond(
        calls.try_recv().unwrap(),
        serde_json::json!({"items":[
            {"id":"000000000000001","name":"General","type":"text"},
            {"id":"000000000000002","name":"Other","type":"text"}
        ]}),
    );
    drain(cx, &chat);
    respond(
        calls.try_recv().unwrap(),
        serde_json::json!({"items":[message("000000000000008", "000000000000001")]}),
    );
    drain(cx, &chat);
    (chat, calls, streams, stream)
}
pub fn ids(chat: &ChatHandle) -> Vec<String> {
    let state = chat.read();
    let Some(Load::Ready(rows)) = state.history_for_display("000000000000001") else {
        panic!("ready history");
    };
    rows.iter().map(|m| m.id.clone()).collect()
}
