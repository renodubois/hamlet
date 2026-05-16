use std::io::{BufRead, BufReader};

use iced::futures::stream::BoxStream;
use iced::futures::{SinkExt, StreamExt};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, COOKIE};
use thiserror::Error;

use crate::auth::AuthenticatedRequest;
use crate::protocol::BroadcastEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeEvent {
    Connected,
    Disconnected,
    Broadcast(BroadcastEvent),
    Malformed(String),
    AuthExpired,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RealtimeError {
    #[error("realtime transport is not connected yet: {0}")]
    NotImplemented(&'static str),
    #[error("could not parse realtime event: {0}")]
    Parse(String),
    #[error("realtime authentication expired")]
    AuthExpired,
    #[error("fake realtime failure: {0}")]
    Fake(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeConnectionState {
    Disconnected,
    Connecting,
    Connected,
    BackingOff { attempt: u32, delay_ms: u64 },
    AuthExpired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeCall {
    Connect {
        server_url: String,
        has_session: bool,
    },
    Disconnect,
    DrainEvents,
}

pub trait RealtimeClient {
    fn connect(&self, request: AuthenticatedRequest) -> Result<(), RealtimeError>;
    fn disconnect(&self) -> Result<(), RealtimeError>;
    fn drain_events(&self) -> Result<Vec<RealtimeEvent>, RealtimeError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectPolicy {
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl ReconnectPolicy {
    pub const fn new(base_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            base_delay_ms,
            max_delay_ms,
        }
    }

    pub fn delay_for_attempt(self, attempt: u32) -> u64 {
        let exponent = attempt.saturating_sub(1).min(10);
        let multiplier = 1_u64.checked_shl(exponent).unwrap_or(u64::MAX);

        self.base_delay_ms
            .saturating_mul(multiplier)
            .min(self.max_delay_ms)
    }
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self::new(500, 30_000)
    }
}

pub fn event_source_stream(request: &AuthenticatedRequest) -> BoxStream<'static, RealtimeEvent> {
    let request = request.clone();

    iced::stream::channel(100, async move |mut output| {
        std::thread::spawn(move || {
            run_event_source(request, |event| {
                iced::futures::executor::block_on(output.send(event)).is_ok()
            });
        });
    })
    .boxed()
}

pub fn parse_sse_events(input: &str) -> Vec<Result<RealtimeEvent, RealtimeError>> {
    input.split("\n\n").filter_map(parse_sse_block).collect()
}

fn run_event_source(request: AuthenticatedRequest, mut publish: impl FnMut(RealtimeEvent) -> bool) {
    let Some(session_token) = request.session_token else {
        let _ = publish(RealtimeEvent::AuthExpired);
        return;
    };
    let endpoint = format!("{}/messages/subscribe", request.server_url);
    let response = Client::new()
        .get(endpoint)
        .header(ACCEPT, "text/event-stream")
        .header(COOKIE, format!("session={session_token}"))
        .send();
    let Ok(response) = response else {
        let _ = publish(RealtimeEvent::Disconnected);
        return;
    };

    if response.status() == StatusCode::UNAUTHORIZED {
        let _ = publish(RealtimeEvent::AuthExpired);
        return;
    }

    if !response.status().is_success() {
        let _ = publish(RealtimeEvent::Disconnected);
        return;
    }

    let reader = BufReader::new(response);
    let mut block = String::new();

    for line in reader.lines() {
        let Ok(line) = line else {
            let _ = publish(RealtimeEvent::Disconnected);
            return;
        };

        if line.is_empty() {
            if !publish_sse_block(&block, &mut publish) {
                return;
            }
            block.clear();
        } else {
            block.push_str(&line);
            block.push('\n');
        }
    }

    let _ = publish(RealtimeEvent::Disconnected);
}

fn publish_sse_block(block: &str, publish: &mut impl FnMut(RealtimeEvent) -> bool) -> bool {
    let framed = format!("{block}\n");

    for event in parse_sse_events(&framed) {
        let event = match event {
            Ok(event) => event,
            Err(error) => RealtimeEvent::Malformed(error.to_string()),
        };

        if !publish(event) {
            return false;
        }
    }

    true
}

fn parse_sse_block(block: &str) -> Option<Result<RealtimeEvent, RealtimeError>> {
    let mut data_lines = Vec::new();

    for raw_line in block.lines() {
        let line = raw_line.trim_end_matches('\r');

        if line.starts_with(':') || line.is_empty() {
            continue;
        }

        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim_start().to_string());
        }
    }

    if data_lines.is_empty() {
        return None;
    }

    let data = data_lines.join("\n");

    if data == "connected" {
        return Some(Ok(RealtimeEvent::Connected));
    }

    Some(
        serde_json::from_str::<BroadcastEvent>(&data)
            .map(RealtimeEvent::Broadcast)
            .map_err(|error| RealtimeError::Parse(error.to_string())),
    )
}
