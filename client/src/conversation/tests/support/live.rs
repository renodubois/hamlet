use crate::api::test_support::{RequestAdapter, Response, StreamAdapter, StreamResponse};
use crate::api::{ApiError, ApiFuture, HttpTransport};
use crate::conversation::ConversationHandle;
use crate::runtime::Execution;
use gpui_kit::TestAppContext;
use reqwest::{Request, StatusCode};
use std::sync::Arc;

pub struct Call {
    pub request: Request,
    pub reply: async_channel::Sender<Result<Response, ApiError>>,
}
struct Requests(async_channel::Sender<Call>);
impl RequestAdapter for Requests {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        let (reply, receive) = async_channel::bounded(1);
        self.0.try_send(Call { request, reply }).unwrap();
        Box::pin(async move { receive.recv().await.unwrap_or(Err(ApiError::Unavailable)) })
    }
}
pub struct Stream {
    pub request: Request,
    pub body: async_channel::Sender<Result<Vec<u8>, ApiError>>,
}
impl Stream {
    pub fn frame(&self, event: &str, data: serde_json::Value) {
        self.body
            .try_send(Ok(format!("event: {event}\ndata: {data}\n\n").into_bytes()))
            .unwrap();
    }
    pub fn ready(&self) {
        self.frame("ready", serde_json::json!({}));
    }
    pub fn message(&self, id: &str, channel: &str) {
        self.frame(
            "change",
            serde_json::json!({"type":"message_created", "message": message(id, channel)}),
        );
    }
}
struct Streams(async_channel::Sender<Stream>);
impl StreamAdapter for Streams {
    fn open(&self, request: Request) -> ApiFuture<Result<StreamResponse, ApiError>> {
        let (send, body) = async_channel::bounded(512);
        self.0
            .try_send(Stream {
                request,
                body: send,
            })
            .unwrap();
        Box::pin(async move {
            Ok(StreamResponse::Controlled {
                status: StatusCode::OK,
                content_type: "text/event-stream".into(),
                body,
            })
        })
    }
}
pub fn fixture(
    cx: &TestAppContext,
) -> (
    ConversationHandle,
    async_channel::Receiver<Call>,
    async_channel::Receiver<Stream>,
) {
    let (send, calls) = async_channel::unbounded();
    let (open, streams) = async_channel::unbounded();
    let client = HttpTransport::with_adapters(Arc::new(Requests(send)), Arc::new(Streams(open)))
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
        streams,
    )
}
pub fn respond(call: Call, value: serde_json::Value) {
    call.reply
        .try_send(Ok(Response::controlled(StatusCode::OK, value.to_string())))
        .unwrap();
}
pub fn drain(cx: &mut TestAppContext, activity: &ConversationHandle) {
    loop {
        cx.executor().run_until_parked();
        let Ok(update) = activity.updates().try_recv() else {
            break;
        };
        assert!(activity.apply(update).is_none());
    }
}
pub fn message(id: &str, channel: &str) -> serde_json::Value {
    serde_json::json!({"id":id,"channel_id":channel,"author":{"id":"u","display_name":"Ada"},"text":"same text","created_at":"2026-01-01T00:00:00Z"})
}
pub fn page(ids: &[&str]) -> serde_json::Value {
    serde_json::json!({"items": ids.iter().map(|id| message(id, "1")).collect::<Vec<_>>(), "next_cursor": null})
}
pub fn channels() -> serde_json::Value {
    serde_json::json!({"items":[{"id":"1","name":"General","type":"text"},{"id":"2","name":"Other","type":"text"}]})
}
pub fn ready(
    cx: &mut TestAppContext,
) -> (
    ConversationHandle,
    async_channel::Receiver<Call>,
    async_channel::Receiver<Stream>,
    Stream,
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
    (activity, calls, streams, stream)
}

pub fn ids(activity: &ConversationHandle) -> Vec<String> {
    let state = activity.read();
    let Some(crate::conversation::Load::Ready(messages)) = state.history_for_display("1") else {
        panic!("history not ready")
    };
    messages.iter().map(|message| message.id.clone()).collect()
}
