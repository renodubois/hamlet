//! Opens and parses one authenticated SSE connection, with deadlines and bounded event delivery.
//! Dropping the stream cancels it; workspace coordination owns reconnection.

use super::{ApiError, ApiFuture, AuthenticatedClient};
use crate::runtime::{Execution, Work};
use reqwest::{Client, Request, StatusCode};
use std::time::{Duration, Instant};

const FRAME_LIMIT: usize = 64 * 1024;
const DELIVERY_CAPACITY: usize = 256;
const READY_TIMEOUT: Duration = Duration::from_secs(8);
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveEvent {
    Ready,
    ChannelCreated(super::Channel),
    ChannelRenamed(super::Channel),
    ChannelDeleted(String),
    MessageCreated(super::Message),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamError {
    Api(ApiError),
    Ended,
    Overflow,
    ReadyTimeout,
    IdleTimeout,
}

/// Owns one task/body. Dropping the handle cancels, never detaches, its attempt.
pub struct EventStream {
    events: async_channel::Receiver<LiveEvent>,
    terminal: async_channel::Receiver<StreamError>,
    ended: Option<StreamError>,
    work: Option<Work>,
}
impl EventStream {
    /// A terminal result invalidates queued deliveries and remains observable.
    pub async fn next(&mut self) -> Result<LiveEvent, StreamError> {
        if let Some(error) = &self.ended {
            return Err(error.clone());
        }
        let result = tokio::select! {
            biased;
            terminal = self.terminal.recv() => Err(terminal.unwrap_or(StreamError::Ended)),
            event = self.events.recv() => match event {
                Ok(event) => Ok(event),
                Err(_) => Err(self.terminal.recv().await.unwrap_or(StreamError::Ended)),
            },
        };
        if let Err(error) = &result {
            self.ended = Some(error.clone());
        }
        result
    }
}
impl Drop for EventStream {
    fn drop(&mut self) {
        if let Some(work) = self.work.take() {
            work.abort();
        }
    }
}

impl AuthenticatedClient {
    pub(crate) fn events(&self, execution: &Execution) -> EventStream {
        let client = self.clone();
        let (send, events) = async_channel::bounded(DELIVERY_CAPACITY);
        let ready_deadline = execution.now() + READY_TIMEOUT;
        let clock = execution.clone();
        let (work, terminal) = execution.start(async move {
            match consume(client, send, clock, ready_deadline).await {
                Ok(()) => StreamError::Ended,
                Err(error) => error,
            }
        });
        EventStream {
            events,
            terminal,
            ended: None,
            work: Some(work),
        }
    }
}

pub(crate) trait StreamAdapter: Send + Sync {
    fn open(&self, request: Request) -> ApiFuture<Result<StreamResponse, ApiError>>;
}
impl StreamAdapter for Client {
    fn open(&self, request: Request) -> ApiFuture<Result<StreamResponse, ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            client
                .execute(request)
                .await
                .map(StreamResponse::Http)
                .map_err(|_| ApiError::Unavailable)
        })
    }
}
pub(crate) enum StreamResponse {
    Http(reqwest::Response),
    #[cfg(test)]
    Controlled {
        status: StatusCode,
        content_type: String,
        body: async_channel::Receiver<Result<Vec<u8>, ApiError>>,
    },
}
impl StreamResponse {
    fn validate(&self) -> Result<(), ApiError> {
        let (status, content_type) = match self {
            Self::Http(response) => (
                response.status(),
                response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or(""),
            ),
            #[cfg(test)]
            Self::Controlled {
                status,
                content_type,
                ..
            } => (*status, content_type.as_str()),
        };
        match status {
            StatusCode::UNAUTHORIZED => return Err(ApiError::AlreadyInvalid),
            StatusCode::INTERNAL_SERVER_ERROR => return Err(ApiError::ServerFailure),
            StatusCode::OK => {}
            _ => return Err(ApiError::Unavailable),
        }
        if !content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .eq_ignore_ascii_case("text/event-stream")
        {
            return Err(ApiError::InvalidResponse);
        }
        Ok(())
    }

    async fn chunk(&mut self) -> Result<Option<Vec<u8>>, ApiError> {
        match self {
            Self::Http(response) => response
                .chunk()
                .await
                .map(|chunk| chunk.map(|bytes| bytes.to_vec()))
                .map_err(|_| ApiError::Unavailable),
            #[cfg(test)]
            Self::Controlled { body, .. } => body.recv().await.ok().transpose(),
        }
    }
}
async fn consume(
    client: AuthenticatedClient,
    send: async_channel::Sender<LiveEvent>,
    clock: Execution,
    ready_deadline: Instant,
) -> Result<(), StreamError> {
    let transport = &client.0.server.0.transport;
    let request = transport
        .stream_client
        .get(client.0.server.endpoint("api/v1/events"))
        .bearer_auth(&client.0.token)
        .header("accept", "text/event-stream")
        .build()
        .map_err(|_| StreamError::Api(ApiError::Unavailable))?;
    if clock.now() >= ready_deadline {
        return Err(StreamError::ReadyTimeout);
    }
    let mut response = tokio::select! {
        biased;
        _ = clock.sleep(ready_deadline.saturating_duration_since(clock.now())) => return Err(StreamError::ReadyTimeout),
        response = transport.stream_adapter.open(request) => response.map_err(StreamError::Api)?,
    };
    response.validate().map_err(StreamError::Api)?;
    let mut parser = Parser::default();
    let mut ready = false;
    let mut idle_deadline = clock.now() + IDLE_TIMEOUT;
    loop {
        let (deadline, timeout) = if ready {
            (idle_deadline, StreamError::IdleTimeout)
        } else {
            (ready_deadline, StreamError::ReadyTimeout)
        };
        if clock.now() >= deadline {
            return Err(timeout);
        }
        let chunk = tokio::select! {
            biased;
            _ = clock.sleep(deadline.saturating_duration_since(clock.now())) => return Err(timeout),
            chunk = response.chunk() => chunk.map_err(StreamError::Api)?,
        };
        let Some(chunk) = chunk else {
            break;
        };
        if !chunk.is_empty() {
            idle_deadline = clock.now() + IDLE_TIMEOUT;
        }
        for byte in chunk {
            if let Some(event) = parser.push(byte).map_err(StreamError::Api)? {
                if matches!(event, LiveEvent::Ready) {
                    if ready {
                        return Err(StreamError::Api(ApiError::InvalidResponse));
                    }
                    ready = true;
                } else if !ready {
                    return Err(StreamError::Api(ApiError::InvalidResponse));
                }
                send.try_send(event).map_err(|error| match error {
                    async_channel::TrySendError::Full(_) => StreamError::Overflow,
                    async_channel::TrySendError::Closed(_) => StreamError::Ended,
                })?;
            }
        }
    }
    // EOF does not dispatch an unfinished frame, even if its data line is complete.
    Ok(())
}

#[derive(Default)]
struct Parser {
    line: Vec<u8>,
    data: String,
    kind: String,
    skip_lf: bool,
    started: bool,
    frame_bytes: usize,
    frame_ended: bool,
}
impl Parser {
    fn push(&mut self, byte: u8) -> Result<Option<LiveEvent>, ApiError> {
        // A paired LF still belongs to the preceding CR-terminated frame. Reset
        // only at the next frame's first byte, so CRLF bytes count too.
        if self.frame_ended && !(self.skip_lf && byte == b'\n') {
            self.frame_bytes = 0;
            self.frame_ended = false;
        }
        self.frame_bytes += 1;
        if self.frame_bytes > FRAME_LIMIT {
            return Err(ApiError::InvalidResponse);
        }
        if std::mem::take(&mut self.skip_lf) && byte == b'\n' {
            return Ok(None);
        }
        if byte != b'\r' && byte != b'\n' {
            self.line.push(byte);
            return Ok(None);
        }
        self.skip_lf = byte == b'\r';
        // Decode complete lines, preserving UTF-8 split across transport chunks.
        // WHATWG UTF-8 decoding replaces malformed sequences; strip just the first BOM.
        let bytes = std::mem::take(&mut self.line);
        let line = String::from_utf8_lossy(&bytes);
        let line = if !std::mem::replace(&mut self.started, true) {
            line.strip_prefix('\u{feff}').unwrap_or(&line)
        } else {
            &line
        };
        if line.is_empty() {
            self.frame_ended = true;
            let kind = std::mem::take(&mut self.kind);
            let mut data = std::mem::take(&mut self.data);
            if data.is_empty() {
                return Ok(None);
            }
            data.pop(); // Strip the final data-line LF, not meaningful whitespace.
            return decode(&kind, &data);
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => self.kind = value.to_owned(),
            "data" => {
                self.data.push_str(value);
                self.data.push('\n');
            }
            _ => {} // comments, IDs, retry and extension fields have no v1 semantics.
        }
        Ok(None)
    }
}

fn decode(kind: &str, data: &str) -> Result<Option<LiveEvent>, ApiError> {
    match kind {
        "ready" => {
            let value: serde_json::Value =
                serde_json::from_str(data).map_err(|_| ApiError::InvalidResponse)?;
            if !value.is_object() {
                return Err(ApiError::InvalidResponse);
            }
            Ok(Some(LiveEvent::Ready))
        }
        "change" => {
            // Decode the original JSON, not Value: Value collapses duplicate fields
            // and discriminators that the shared wire deserializer rejects.
            let event: hamlet_protocol::Event =
                serde_json::from_str(data).map_err(|_| ApiError::InvalidResponse)?;
            Ok(Some(match event {
                hamlet_protocol::Event::ChannelCreated { channel } => {
                    LiveEvent::ChannelCreated(super::channels::decode_channel(channel)?)
                }
                hamlet_protocol::Event::ChannelRenamed { channel } => {
                    LiveEvent::ChannelRenamed(super::channels::decode_channel(channel)?)
                }
                hamlet_protocol::Event::ChannelDeleted { channel_id } => {
                    if channel_id.is_empty()
                        || !channel_id.bytes().all(|byte| byte.is_ascii_digit())
                        || !matches!(channel_id.parse::<i64>(), Ok(id) if id > 0)
                    {
                        return Err(ApiError::InvalidResponse);
                    }
                    LiveEvent::ChannelDeleted(channel_id)
                }
                hamlet_protocol::Event::MessageCreated { message } => {
                    let channel = message.channel_id.clone();
                    LiveEvent::MessageCreated(super::messages::decode_message(message, &channel)?)
                }
                hamlet_protocol::Event::Unknown => return Ok(None),
            }))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
#[path = "tests/event_http.rs"]
mod http_tests;
#[cfg(test)]
#[path = "tests/events.rs"]
mod tests;
