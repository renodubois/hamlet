//! Voice chat integration.
//!
//! All media goes through a LiveKit server (see `docker-compose.yml`). The Rust
//! side only mints JWT access tokens and reacts to LiveKit's server→server
//! webhooks so the rest of the app can render who's currently in each voice
//! channel.
//!
//! Room names are `channel-{channel_id}`. Participant `identity` is the user's
//! numeric id as a string — that's what lets the webhook handler map a LiveKit
//! event back to one of our users.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Debug)]
pub struct VoiceConfig {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
}

impl VoiceConfig {
    /// Read configuration from env (`LIVEKIT_URL`, `LIVEKIT_API_KEY`,
    /// `LIVEKIT_API_SECRET`). Returns `None` when any variable is missing so
    /// `cargo run` without LiveKit still boots — voice endpoints will then
    /// return 503.
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("LIVEKIT_URL").ok()?;
        let api_key = std::env::var("LIVEKIT_API_KEY").ok()?;
        let api_secret = std::env::var("LIVEKIT_API_SECRET").ok()?;
        if url.is_empty() || api_key.is_empty() || api_secret.is_empty() {
            return None;
        }
        Some(Self {
            url,
            api_key,
            api_secret,
        })
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct VoiceParticipant {
    pub user_id: i64,
    pub channel_id: i64,
    pub username: String,
    pub avatar_url: Option<String>,
}

/// In-memory tracking of who is currently connected to each voice channel.
/// LiveKit is the source of truth; this cache is fed by webhook events and
/// consumed by the sidebar UI (via `GET /voice/participants/{channel_id}` and
/// `voice_*` SSE events).
#[derive(Debug, Default)]
pub struct VoiceState {
    // channel_id -> user_id -> participant
    rooms: Mutex<HashMap<i64, HashMap<i64, VoiceParticipant>>>,
}

impl VoiceState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn participants(&self, channel_id: i64) -> Vec<VoiceParticipant> {
        let rooms = match self.rooms.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        rooms
            .get(&channel_id)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Insert a participant. Returns true if this user wasn't already in the
    /// room (so callers can skip broadcasting duplicate join events — LiveKit
    /// delivers webhooks at-least-once).
    pub fn add_participant(&self, p: VoiceParticipant) -> bool {
        let mut rooms = match self.rooms.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        rooms
            .entry(p.channel_id)
            .or_default()
            .insert(p.user_id, p)
            .is_none()
    }

    /// Remove a participant. Returns the removed entry if it existed.
    pub fn remove_participant(&self, channel_id: i64, user_id: i64) -> Option<VoiceParticipant> {
        let mut rooms = match self.rooms.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let entry = rooms.entry(channel_id);
        let std::collections::hash_map::Entry::Occupied(mut occ) = entry else {
            return None;
        };
        let removed = occ.get_mut().remove(&user_id);
        if occ.get().is_empty() {
            occ.remove();
        }
        removed
    }
}

/// Turn a channel id into the LiveKit room name. Kept as a function so the
/// webhook handler can parse it back out reliably.
pub fn room_name(channel_id: i64) -> String {
    format!("channel-{channel_id}")
}

/// Reverse of `room_name`. Returns `None` if the room isn't one of ours —
/// LiveKit may run with other tenants and we want to ignore those webhooks.
pub fn parse_channel_id(room_name: &str) -> Option<i64> {
    room_name.strip_prefix("channel-")?.parse().ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn p(channel_id: i64, user_id: i64) -> VoiceParticipant {
        VoiceParticipant {
            user_id,
            channel_id,
            username: format!("user{user_id}"),
            avatar_url: None,
        }
    }

    #[test]
    fn round_trip_room_name() {
        assert_eq!(room_name(42), "channel-42");
        assert_eq!(parse_channel_id("channel-42"), Some(42));
        assert_eq!(parse_channel_id("channel-"), None);
        assert_eq!(parse_channel_id("other-42"), None);
    }

    #[test]
    fn add_remove_and_list() {
        let state = VoiceState::new();
        assert!(state.add_participant(p(1, 10)));
        assert!(state.add_participant(p(1, 11)));
        // duplicate returns false
        assert!(!state.add_participant(p(1, 10)));

        let mut ids: Vec<i64> = state
            .participants(1)
            .into_iter()
            .map(|x| x.user_id)
            .collect();
        ids.sort();
        assert_eq!(ids, vec![10, 11]);

        let removed = state.remove_participant(1, 10).unwrap();
        assert_eq!(removed.user_id, 10);
        assert!(state.remove_participant(1, 10).is_none());
        assert_eq!(state.participants(1).len(), 1);

        // Removing the last participant drops the room entry too.
        state.remove_participant(1, 11);
        assert!(state.participants(1).is_empty());
    }
}
