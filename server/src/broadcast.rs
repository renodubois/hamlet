//! SSE broadcast transport.
//!
//! `Broadcaster` keeps the live SSE subscriber list and fans out one event
//! to every client. The transport itself is generic over `Serialize`, so
//! adding a new event variant only requires extending [`BroadcastEvent`].
//!
//! The `BroadcastEvent` enum and its event-only DTOs live here because
//! they are shaped entirely by what subscribers consume — they don't
//! belong to any single resource module. The handler-facing response
//! types (`MessageResponse`, `ChannelResponse`, `VoiceParticipant`)
//! are owned by their resource module and re-used here as variant
//! payloads.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use actix_sse::{Data, Event, Sse};
use actix_web::Responder;
use futures_util::future;
use serde::Serialize;
use tokio::{sync::mpsc, time::interval};
use tokio_stream::wrappers::ReceiverStream;

use crate::api::channels::ChannelResponse;
use crate::api::emoji::EmojiResponse;
use crate::api::messages::{EmbedResponse, MessageResponse};
use crate::error::AppError;
use crate::voice::VoiceParticipant;

#[derive(Debug)]
pub struct Broadcaster {
    inner: Mutex<BroadcasterInner>,
}

#[derive(Debug, Default)]
struct BroadcasterInner {
    clients: Vec<mpsc::Sender<Event>>,
}

impl Broadcaster {
    /// Bare instance — no ping loop. Tests use this so the 10s tick doesn't
    /// race with assertions.
    pub fn new() -> Arc<Self> {
        Arc::new(Broadcaster {
            inner: Mutex::new(BroadcasterInner::default()),
        })
    }

    /// Production constructor: also spawns the 10s ping loop that culls
    /// dead subscribers.
    pub fn create() -> Arc<Self> {
        let this = Self::new();
        Broadcaster::spawn_ping(Arc::clone(&this));
        this
    }

    fn spawn_ping(this: Arc<Self>) {
        actix_web::rt::spawn(async move {
            let mut interval = interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                this.remove_stale_clients().await;
            }
        });
    }

    async fn remove_stale_clients(&self) {
        let clients = match self.inner.lock() {
            Ok(g) => g.clients.clone(),
            Err(p) => p.into_inner().clients.clone(),
        };
        let mut ok_clients = Vec::new();
        for client in clients {
            if client.send(Event::Comment("ping".into())).await.is_ok() {
                ok_clients.push(client.clone());
            }
        }
        if let Ok(mut g) = self.inner.lock() {
            g.clients = ok_clients;
        }
    }

    /// Register a new SSE subscriber; returns the `Sse` responder that the
    /// handler hands back to actix-web.
    pub async fn subscribe(&self) -> impl Responder + use<> {
        let (tx, rx) = mpsc::channel(10);
        let _ = tx.send(Data::new("connected").into()).await;
        if let Ok(mut g) = self.inner.lock() {
            g.clients.push(tx);
        }
        Sse::from_infallible_stream(ReceiverStream::new(rx))
    }

    /// Serialize `event` as JSON and send it to every subscriber.
    pub async fn publish<E: Serialize + ?Sized>(&self, event: &E) -> Result<(), AppError> {
        let payload = serde_json::to_string(event)?;
        self.send_raw(&payload).await;
        Ok(())
    }

    async fn send_raw(&self, msg: &str) {
        let clients = match self.inner.lock() {
            Ok(g) => g.clients.clone(),
            Err(p) => p.into_inner().clients.clone(),
        };
        let send_futures = clients
            .iter()
            .map(|client| client.send(Data::new(msg).into()));
        future::join_all(send_futures).await;
    }

    pub fn test_client(&self) -> tokio::sync::mpsc::Receiver<actix_sse::Event> {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        if let Ok(mut g) = self.inner.lock() {
            g.clients.push(tx);
        }
        rx
    }
}

// --- event taxonomy ---

#[derive(Clone, Debug, Serialize)]
pub struct MessageDeletedEvent {
    pub id: i64,
    pub channel_id: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MessageEmbedsUpdatedEvent {
    pub id: i64,
    pub channel_id: i64,
    pub suppress_embeds: bool,
    pub embeds: Vec<EmbedResponse>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VoiceParticipantLeftEvent {
    pub channel_id: i64,
    pub user_id: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct VoiceParticipantSpeakingEvent {
    pub channel_id: i64,
    pub user_id: i64,
    pub speaking: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct UserTypingEvent {
    pub channel_id: i64,
    pub user_id: i64,
    pub username: String,
}

/// One discriminated-union of every SSE event the server publishes. The
/// `kind` tag is what the client switches on to dispatch.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum BroadcastEvent {
    Message(MessageResponse),
    MessageUpdated(MessageResponse),
    MessageDeleted(MessageDeletedEvent),
    MessageEmbedsUpdated(MessageEmbedsUpdatedEvent),
    ChannelCreated(ChannelResponse),
    ChannelsReordered(Vec<ChannelResponse>),
    EmojiCreated(EmojiResponse),
    EmojiUpdated(EmojiResponse),
    EmojiDeleted(EmojiResponse),
    VoiceParticipantJoined(VoiceParticipant),
    VoiceParticipantLeft(VoiceParticipantLeftEvent),
    VoiceParticipantSpeakingChanged(VoiceParticipantSpeakingEvent),
    UserTyping(UserTypingEvent),
}
