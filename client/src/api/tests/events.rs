use super::*;
use crate::api::HttpTransport;
use gpui_kit::TestAppContext;
use std::sync::Arc;

struct Call {
    request: reqwest::Request,
    reply: async_channel::Sender<Result<StreamResponse, ApiError>>,
}
struct Controlled(async_channel::Sender<Call>);
impl StreamAdapter for Controlled {
    fn open(&self, request: reqwest::Request) -> ApiFuture<Result<StreamResponse, ApiError>> {
        let (reply, receive) = async_channel::bounded(1);
        self.0.try_send(Call { request, reply }).unwrap();
        Box::pin(async move { receive.recv().await.unwrap_or(Err(ApiError::Unavailable)) })
    }
}
fn fixture(cx: &TestAppContext) -> (HttpTransport, Execution, async_channel::Receiver<Call>) {
    let (send, calls) = async_channel::bounded(8);
    (
        HttpTransport::with_stream_adapter(Arc::new(Controlled(send))),
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
        calls,
    )
}
fn respond(call: Call) -> async_channel::Sender<Result<Vec<u8>, ApiError>> {
    let (send, body) = async_channel::bounded(512);
    assert!(
        call.reply
            .try_send(Ok(StreamResponse::Controlled {
                status: reqwest::StatusCode::OK,
                content_type: "text/event-stream".into(),
                body,
            }))
            .is_ok()
    );
    send
}
fn next(cx: &mut TestAppContext, stream: &mut EventStream) -> Result<LiveEvent, StreamError> {
    cx.executor().run_until_parked();
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    match std::pin::pin!(stream.next())
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("expected a finite ready delivery"),
    }
}

fn opened(
    cx: &mut TestAppContext,
) -> (
    EventStream,
    async_channel::Sender<Result<Vec<u8>, ApiError>>,
) {
    let (transport, execution, calls) = fixture(cx);
    let client = transport
        .server("https://example.com")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    let mut stream = client.events(&execution);
    cx.executor().run_until_parked();
    let body = respond(calls.try_recv().unwrap());
    body.try_send(Ok(b"event: ready\ndata: {}\n\n".to_vec()))
        .unwrap();
    assert_eq!(next(cx, &mut stream), Ok(LiveEvent::Ready));
    (stream, body)
}
const CHANNEL: &str = r#"{"type":"channel_created","channel":{"id":"123","name":"general","type":"text","extra":1},"extra":true}"#;
#[gpui_kit::test]
fn rename_uses_shared_channel_validation(cx: &mut TestAppContext) {
    let renamed = CHANNEL.replace("channel_created", "channel_renamed");
    let (mut stream, body) = opened(cx);
    body.try_send(Ok(change(&renamed))).unwrap();
    assert_eq!(
        next(cx, &mut stream),
        Ok(LiveEvent::ChannelRenamed(super::super::Channel {
            id: "123".into(),
            name: "general".into()
        }))
    );
    for invalid in [
        renamed.replace("\"123\"", "\"\""),
        renamed.replace("general", ""),
        renamed.replace("text", "voice"),
        r#"{"type":"channel_renamed"}"#.into(),
    ] {
        let (mut stream, body) = opened(cx);
        body.try_send(Ok(change(&invalid))).unwrap();
        assert_eq!(
            next(cx, &mut stream),
            Err(StreamError::Api(ApiError::InvalidResponse))
        );
    }
}

const MESSAGE: &str = r#"{"type":"message_created","message":{"id":"456","channel_id":"123","author":{"id":"789","display_name":"Ada","extra":true},"text":"雪\"\\\nnext","created_at":"2026-01-01T01:00:00+01:00","extra":1}}"#;
fn change(value: &str) -> Vec<u8> {
    format!("event: change\ndata: {value}\n\n").into_bytes()
}

#[gpui_kit::test]
fn creations_use_the_http_entity_conversion_and_validation(cx: &mut TestAppContext) {
    let (mut stream, body) = opened(cx);
    body.try_send(Ok(change(CHANNEL))).unwrap();
    assert_eq!(
        next(cx, &mut stream),
        Ok(LiveEvent::ChannelCreated(super::super::Channel {
            id: "123".into(),
            name: "general".into()
        }))
    );
    body.try_send(Ok(change(MESSAGE))).unwrap();
    let Ok(LiveEvent::MessageCreated(message)) = next(cx, &mut stream) else {
        panic!("message delivery")
    };
    assert_eq!(message.id, "456");
    assert_eq!(message.channel_id, "123");
    assert_eq!(message.author_id, "789");
    assert_eq!(message.author_name, "Ada");
    assert_eq!(message.text, "雪\"\\\nnext");
    assert_eq!(message.created_at, "2026-01-01T00:00:00+00:00");
    for invalid in [
        CHANNEL.replace("\"123\"", "\"\""),
        CHANNEL.replace("general", ""),
        CHANNEL.replace("text", "voice"),
        MESSAGE.replace("\"456\"", "\"\""),
        MESSAGE.replace("\"123\"", "\"\""),
        MESSAGE.replace("\"789\"", "\"\""),
        MESSAGE.replace("Ada", ""),
        MESSAGE.replace("2026-01-01T01:00:00+01:00", "invalid"),
    ] {
        let (mut stream, body) = opened(cx);
        body.try_send(Ok(change(&invalid))).unwrap();
        assert_eq!(
            next(cx, &mut stream),
            Err(StreamError::Api(ApiError::InvalidResponse))
        );
    }
}

#[gpui_kit::test]
fn standard_sse_survives_every_byte_split_bom_crlf_cr_and_multiline_data(cx: &mut TestAppContext) {
    let input = format!(
        "\u{feff}: comment\r\nevent: ignored\r\nevent: ready\r\ndata: {{\r\ndata: }}\r\n\r\n: heartbeat\r\rretry: 100\rid: not-replayed\runknown-field\revent: change\rdata:{}\r\r",
        MESSAGE
    );
    for split in 0..=input.len() {
        let (transport, execution, calls) = fixture(cx);
        let client = transport
            .server("https://example.com")
            .unwrap()
            .restore_candidate("synthetic".into())
            .unwrap();
        let mut stream = client.events(&execution);
        cx.executor().run_until_parked();
        let body = respond(calls.try_recv().unwrap());
        body.try_send(Ok(input.as_bytes()[..split].to_vec()))
            .unwrap();
        cx.executor().run_until_parked();
        body.try_send(Ok(input.as_bytes()[split..].to_vec()))
            .unwrap();
        assert_eq!(next(cx, &mut stream), Ok(LiveEvent::Ready), "split {split}");
        let Ok(LiveEvent::MessageCreated(message)) = next(cx, &mut stream) else {
            panic!("split {split}")
        };
        assert_eq!(message.text, "雪\"\\\nnext");
        // A complete data line without the dispatching blank line is discarded.
        let mut partial = change(CHANNEL);
        partial.pop();
        body.try_send(Ok(partial)).unwrap();
        cx.executor().run_until_parked();
        drop(body);
        assert_eq!(next(cx, &mut stream), Err(StreamError::Ended));
    }
}

#[gpui_kit::test]
fn unknown_kinds_and_types_are_ignored_but_supported_malformed_frames_end_attempt(
    cx: &mut TestAppContext,
) {
    let (mut stream, body) = opened(cx);
    let mut bytes = b"event: future\ndata: not-json\n\nevent: change\ndata: {\"type\":\"future_change\",\"arbitrary\":true}\n\ndata: default message ignored\n\n".to_vec();
    bytes.extend(change(CHANNEL));
    body.try_send(Ok(bytes)).unwrap();
    assert!(matches!(
        next(cx, &mut stream),
        Ok(LiveEvent::ChannelCreated(_))
    ));
    for invalid in [
        "event: ready\ndata: nope\n\n",
        "event: ready\ndata: []\n\n",
        "event: change\ndata: nope\n\n",
        "event: change\ndata: {}\n\n",
        "event: change\ndata: {\"type\":1}\n\n",
        "event: change\ndata: {\"type\":\"message_created\"}\n\n",
        "event: change\ndata: {\"type\":\"channel_created\",\"channel\":null}\n\n",
        "event: change\ndata\n\n",
        "event: ready\ndata: {}\n\n", // readiness is once, not a new attempt
    ] {
        let (mut stream, body) = opened(cx);
        body.try_send(Ok(invalid.as_bytes().to_vec())).unwrap();
        assert_eq!(
            next(cx, &mut stream),
            Err(StreamError::Api(ApiError::InvalidResponse)),
            "{invalid}"
        );
        assert_eq!(
            next(cx, &mut stream),
            Err(StreamError::Api(ApiError::InvalidResponse))
        );
        assert!(body.is_closed());
    }
    // A supported creation before readiness cannot establish synchronization.
    let (transport, execution, calls) = fixture(cx);
    let mut stream = transport
        .server("https://example.com")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap()
        .events(&execution);
    cx.executor().run_until_parked();
    let body = respond(calls.try_recv().unwrap());
    body.try_send(Ok(change(CHANNEL))).unwrap();
    assert_eq!(
        next(cx, &mut stream),
        Err(StreamError::Api(ApiError::InvalidResponse))
    );
}

#[gpui_kit::test]
fn frame_limit_is_per_frame_accepts_maximum_escaped_messages_and_bounds_incomplete_data(
    cx: &mut TestAppContext,
) {
    // U+0001 is six JSON bytes per character; more than any unescaped UTF-8 scalar.
    let text = "\u{0001}".repeat(4000);
    let mut value: serde_json::Value = serde_json::from_str(MESSAGE).unwrap();
    value["message"]["text"] = text.clone().into();
    let frame = change(&value.to_string());
    assert!(frame.len() > 24_000 && frame.len() < 65_536);
    let (mut stream, body) = opened(cx);
    let chunk = frame.repeat(4);
    assert!(chunk.len() > 65_536);
    body.try_send(Ok(chunk)).unwrap();
    for _ in 0..4 {
        let Ok(LiveEvent::MessageCreated(message)) = next(cx, &mut stream) else {
            panic!("legal frame")
        };
        assert_eq!(message.text, text);
    }
    // Other conforming senders may escape astral scalars as twelve-byte surrogate pairs.
    let astral = format!(
        r#"{{"type":"message_created","message":{{"id":"456","channel_id":"123","author":{{"id":"789","display_name":"Ada"}},"text":"{}","created_at":"2026-01-01T00:00:00Z"}}}}"#,
        "\\ud83d\\ude00".repeat(4000)
    );
    let frame = change(&astral);
    assert!(frame.len() > 48_000 && frame.len() < 65_536);
    body.try_send(Ok(frame.repeat(2))).unwrap();
    for _ in 0..2 {
        let Ok(LiveEvent::MessageCreated(message)) = next(cx, &mut stream) else {
            panic!("escaped astral frame")
        };
        assert_eq!(message.text, "😀".repeat(4000));
    }
    for prefix in [
        b"data: ".as_slice(),
        b": comment ",
        b"event: future\ndata: ",
    ] {
        let (mut stream, body) = opened(cx);
        let mut oversized = prefix.to_vec();
        oversized.resize(65_537, b'x');
        for chunk in oversized.chunks(997) {
            body.try_send(Ok(chunk.to_vec())).unwrap();
        }
        assert_eq!(
            next(cx, &mut stream),
            Err(StreamError::Api(ApiError::InvalidResponse))
        );
        assert!(body.is_closed());
    }
    // Also count completed field/comment lines, not only the final partial line.
    let (mut stream, body) = opened(cx);
    body.try_send(Ok(b":x\r\n".repeat(16_385))).unwrap();
    assert_eq!(
        next(cx, &mut stream),
        Err(StreamError::Api(ApiError::InvalidResponse))
    );
}

#[gpui_kit::test]
fn exact_frame_bound_counts_all_line_endings_and_resets_between_frames(cx: &mut TestAppContext) {
    for ending in ["\n", "\r", "\r\n"] {
        for size in [65_536, 65_537] {
            let (mut stream, body) = opened(cx);
            let mut frame = format!(
                ":{}{ending}{ending}",
                "x".repeat(size - 1 - 2 * ending.len())
            )
            .into_bytes();
            assert_eq!(frame.len(), size);
            frame.extend(change(CHANNEL));
            body.try_send(Ok(frame)).unwrap();
            if size == 65_536 {
                assert!(matches!(
                    next(cx, &mut stream),
                    Ok(LiveEvent::ChannelCreated(_))
                ));
            } else {
                assert_eq!(
                    next(cx, &mut stream),
                    Err(StreamError::Api(ApiError::InvalidResponse))
                );
            }
        }
    }
}

#[gpui_kit::test]
fn malformed_known_json_cannot_hide_duplicate_entity_fields(cx: &mut TestAppContext) {
    for duplicate in [
        CHANNEL.replace("\"id\":\"123\"", "\"id\":\"\",\"id\":\"123\""),
        CHANNEL.replace(
            "\"type\":\"channel_created\"",
            "\"type\":\"channel_created\",\"type\":\"future\"",
        ),
        CHANNEL.replace(
            "\"type\":\"channel_created\"",
            "\"type\":\"future\",\"type\":\"channel_created\"",
        ),
        r#"{"type":"future","type":"future"}"#.into(),
    ] {
        let (mut stream, body) = opened(cx);
        body.try_send(Ok(change(&duplicate))).unwrap();
        assert_eq!(
            next(cx, &mut stream),
            Err(StreamError::Api(ApiError::InvalidResponse))
        );
    }
}

#[gpui_kit::test]
fn unknown_changes_are_ignored_without_blocking_supported_deliveries(cx: &mut TestAppContext) {
    let (mut stream, body) = opened(cx);
    let mut input = change(r#"{"type":"future","payload":{"nested":[1,null,true]}}"#);
    input.extend(change(
        r#"{"type":"unknown","message":"not a known payload"}"#,
    ));
    input.extend(change(CHANNEL));
    body.try_send(Ok(input)).unwrap();
    assert!(matches!(
        next(cx, &mut stream),
        Ok(LiveEvent::ChannelCreated(_))
    ));

    for invalid in [
        r#"{}"#,
        r#"{"type":null}"#,
        r#"{"type":"future","payload":}"#,
        r#"{"type":"message_created"}"#,
    ] {
        let (mut stream, body) = opened(cx);
        body.try_send(Ok(change(invalid))).unwrap();
        assert_eq!(
            next(cx, &mut stream),
            Err(StreamError::Api(ApiError::InvalidResponse)),
            "{invalid}"
        );
    }
}

#[gpui_kit::test]
fn readiness_validates_its_object_and_unknown_prefixes_cannot_replace_it(cx: &mut TestAppContext) {
    for data in ["nope", "[]", "null", "1", "\"string\"", "{}"] {
        let (transport, execution, calls) = fixture(cx);
        let mut stream = transport
            .server("https://example.com")
            .unwrap()
            .restore_candidate("synthetic".into())
            .unwrap()
            .events(&execution);
        cx.executor().run_until_parked();
        let body = respond(calls.try_recv().unwrap());
        let input = format!(
            "event: unknown\ndata: nonsense\n\nevent: change\ndata: {{\"type\":\"future\"}}\n\nevent: ready\ndata: {data}\n\n"
        );
        for byte in input.bytes() {
            body.try_send(Ok(vec![byte])).unwrap();
            cx.executor().run_until_parked();
        }
        assert_eq!(
            next(cx, &mut stream),
            if data == "{}" {
                Ok(LiveEvent::Ready)
            } else {
                Err(StreamError::Api(ApiError::InvalidResponse))
            }
        );
    }
    let (transport, execution, calls) = fixture(cx);
    let mut stream = transport
        .server("https://example.com")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap()
        .events(&execution);
    cx.executor().run_until_parked();
    let body = respond(calls.try_recv().unwrap());
    body.try_send(Ok(b"event: ready\ndata: {\"extra\":true}\n\n".to_vec()))
        .unwrap();
    assert_eq!(next(cx, &mut stream), Ok(LiveEvent::Ready));
}

#[gpui_kit::test]
fn delivery_is_bounded_and_overflow_preempts_the_backlog_and_closes_body(cx: &mut TestAppContext) {
    let (mut stream, body) = opened(cx);
    body.try_send(Ok(change(CHANNEL).repeat(256))).unwrap();
    for _ in 0..256 {
        assert!(matches!(
            next(cx, &mut stream),
            Ok(LiveEvent::ChannelCreated(_))
        ));
    }
    body.try_send(Ok(change(CHANNEL).repeat(257))).unwrap();
    // The 257th supported update must terminate, not wait behind queued updates.
    assert_eq!(next(cx, &mut stream), Err(StreamError::Overflow));
    assert!(body.is_closed());
    assert_eq!(next(cx, &mut stream), Err(StreamError::Overflow));
}

#[gpui_kit::test]
fn connection_and_readiness_share_an_eight_second_submission_deadline(cx: &mut TestAppContext) {
    use std::time::Duration;
    for send_headers in [false, true] {
        let (transport, execution, calls) = fixture(cx);
        let mut stream = transport
            .server("https://example.com")
            .unwrap()
            .restore_candidate("synthetic".into())
            .unwrap()
            .events(&execution);
        cx.executor().run_until_parked();
        let call = calls.try_recv().unwrap();
        cx.background_executor.advance_clock(Duration::from_secs(6));
        let reply = call.reply.clone();
        let body = if send_headers {
            let body = respond(call);
            body.try_send(Ok(b": heartbeat\n\n".to_vec())).unwrap();
            Some(body)
        } else {
            None
        };
        cx.executor().run_until_parked();
        cx.background_executor.advance_clock(Duration::from_secs(2));
        assert_eq!(next(cx, &mut stream), Err(StreamError::ReadyTimeout));
        assert!(reply.is_closed());
        if let Some(body) = body {
            assert!(body.is_closed());
        }
    }
}

#[gpui_kit::test]
fn idle_deadline_counts_byte_progress_including_heartbeats_not_total_body_time(
    cx: &mut TestAppContext,
) {
    use std::time::Duration;
    let (mut stream, body) = opened(cx);
    for _ in 0..4 {
        cx.background_executor
            .advance_clock(Duration::from_secs(40));
        body.try_send(Ok(b": heartbeat\n\n".to_vec())).unwrap();
        cx.executor().run_until_parked();
        body.try_send(Ok(change(CHANNEL))).unwrap();
        assert!(matches!(
            next(cx, &mut stream),
            Ok(LiveEvent::ChannelCreated(_))
        ));
    }
    // Even a partial comment is byte progress; an empty transport chunk is not.
    cx.background_executor
        .advance_clock(Duration::from_secs(44));
    body.try_send(Ok(b":".to_vec())).unwrap();
    cx.executor().run_until_parked();
    cx.background_executor
        .advance_clock(Duration::from_secs(44));
    body.try_send(Ok(vec![])).unwrap();
    cx.executor().run_until_parked();
    cx.background_executor.advance_clock(Duration::from_secs(1));
    assert_eq!(next(cx, &mut stream), Err(StreamError::IdleTimeout));
    assert!(body.is_closed());
}

#[gpui_kit::test]
fn dropping_an_attempt_cancels_pending_handshake_or_body_without_reconnecting(
    cx: &mut TestAppContext,
) {
    let (transport, execution, calls) = fixture(cx);
    let client = transport
        .server("https://example.com")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    let stream = client.events(&execution);
    cx.executor().run_until_parked();
    let call = calls.try_recv().unwrap();
    drop(stream);
    cx.executor().run_until_parked();
    assert!(call.reply.is_closed());
    let stream = client.events(&execution);
    cx.executor().run_until_parked();
    let body = respond(calls.try_recv().unwrap());
    cx.executor().run_until_parked();
    drop(stream);
    cx.executor().run_until_parked();
    assert!(body.is_closed());
    let (mut stream, body) = opened(cx);
    // Canceling just a next() wait does not cancel the owned attempt.
    {
        use std::{
            future::Future,
            task::{Context, Poll, Waker},
        };
        let mut wait = std::pin::pin!(stream.next());
        assert!(matches!(
            wait.as_mut().poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
    }
    body.try_send(Ok(change(CHANNEL))).unwrap();
    assert!(matches!(
        next(cx, &mut stream),
        Ok(LiveEvent::ChannelCreated(_))
    ));
    drop(stream);
    cx.executor().run_until_parked();
    assert!(body.is_closed());
    cx.background_executor
        .advance_clock(std::time::Duration::from_secs(100));
    cx.executor().run_until_parked();
    assert!(calls.try_recv().is_err());
}

#[gpui_kit::test]
fn eof_and_transport_failure_are_not_authoritative_authentication_rejection(
    cx: &mut TestAppContext,
) {
    let (mut stream, body) = opened(cx);
    drop(body);
    assert_eq!(next(cx, &mut stream), Err(StreamError::Ended));
    let (mut stream, body) = opened(cx);
    body.try_send(Err(ApiError::Unavailable)).unwrap();
    assert_eq!(
        next(cx, &mut stream),
        Err(StreamError::Api(ApiError::Unavailable))
    );
    assert!(body.is_closed());
}

#[gpui_kit::test]
fn handshake_rejects_status_and_content_type_before_reading_body(cx: &mut TestAppContext) {
    let (transport, execution, calls) = fixture(cx);
    let client = transport
        .server("https://example.com")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    for (status, content_type, expected) in [
        (401, "text/event-stream", ApiError::AlreadyInvalid),
        (500, "text/event-stream", ApiError::ServerFailure),
        (307, "text/event-stream", ApiError::Unavailable),
        (204, "text/event-stream", ApiError::Unavailable),
        (200, "application/json", ApiError::InvalidResponse),
        (200, "text/event-streaming", ApiError::InvalidResponse),
        (200, "", ApiError::InvalidResponse),
    ] {
        let mut stream = client.events(&execution);
        cx.executor().run_until_parked();
        let call = calls.try_recv().unwrap();
        let (_send, body) = async_channel::bounded(1);
        assert!(
            call.reply
                .try_send(Ok(StreamResponse::Controlled {
                    status: reqwest::StatusCode::from_u16(status).unwrap(),
                    content_type: content_type.into(),
                    body,
                }))
                .is_ok()
        );
        assert_eq!(next(cx, &mut stream), Err(StreamError::Api(expected)));
    }
}

#[gpui_kit::test]
fn ready_is_delivered_on_the_immutable_authenticated_binding(cx: &mut TestAppContext) {
    let (transport, execution, calls) = fixture(cx);
    let server = transport.server("https://one.example:8443").unwrap();
    let old = server.restore_candidate("synthetic-old".into()).unwrap();
    let clone = old.clone();
    let new = transport
        .server("http://127.0.0.1:4321")
        .unwrap()
        .restore_candidate("synthetic-new".into())
        .unwrap();
    for (client, url, token) in [
        (
            clone,
            "https://one.example:8443/api/v1/events",
            "Bearer synthetic-old",
        ),
        (
            new,
            "http://127.0.0.1:4321/api/v1/events",
            "Bearer synthetic-new",
        ),
        (
            old,
            "https://one.example:8443/api/v1/events",
            "Bearer synthetic-old",
        ),
    ] {
        let mut stream = client.events(&execution);
        cx.executor().run_until_parked();
        let call = calls.try_recv().unwrap();
        assert_eq!(call.request.url().as_str(), url);
        assert_eq!(call.request.method(), reqwest::Method::GET);
        assert_eq!(call.request.headers()["authorization"], token);
        assert_eq!(call.request.headers()["accept"], "text/event-stream");
        assert!(call.request.headers().get("last-event-id").is_none());
        let body = respond(call);
        body.try_send(Ok(b"event: ready\ndata: {}\n\n".to_vec()))
            .unwrap();
        assert_eq!(next(cx, &mut stream), Ok(LiveEvent::Ready));
    }
}
