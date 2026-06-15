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

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use serde::Serialize;

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

#[derive(Clone, Debug, Serialize)]
pub struct VoiceParticipant {
    pub user_id: i64,
    pub channel_id: i64,
    pub username: String,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ScreenShareKey {
    channel_id: i64,
    sharer_user_id: i64,
    participant_identity: String,
    track_sid: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ScreenShareOwnerKey {
    channel_id: i64,
    sharer_user_id: i64,
    participant_identity: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ScreenShareStream {
    pub channel_id: i64,
    pub sharer_user_id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub participant_identity: String,
    pub track_sid: String,
    pub track_name: String,
    pub source: String,
    pub started_at: i64,
}

impl ScreenShareStream {
    fn key(&self) -> ScreenShareKey {
        ScreenShareKey {
            channel_id: self.channel_id,
            sharer_user_id: self.sharer_user_id,
            participant_identity: self.participant_identity.clone(),
            track_sid: self.track_sid.clone(),
        }
    }

    fn owner_key(&self) -> ScreenShareOwnerKey {
        ScreenShareOwnerKey {
            channel_id: self.channel_id,
            sharer_user_id: self.sharer_user_id,
            participant_identity: self.participant_identity.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AddScreenShareStreamResult {
    Added,
    Replaced(ScreenShareStream),
    Unchanged,
}

/// In-memory tracking of who is currently connected to each voice channel.
/// LiveKit is the source of truth; this cache is fed by webhook events and
/// consumed by the sidebar UI (via `GET /voice/participants/{channel_id}` and
/// `voice_*` SSE events).
#[derive(Debug, Default)]
pub struct VoiceState {
    // channel_id -> user_id -> participant
    rooms: Mutex<HashMap<i64, HashMap<i64, VoiceParticipant>>>,
    // (channel_id, sharer_user_id, LiveKit participant identity, track sid) -> stream.
    // At most one stream per (channel, sharer, participant identity) is kept;
    // a new track from the same owner in the same room replaces the old one.
    screen_shares: Mutex<HashMap<ScreenShareKey, ScreenShareStream>>,
    // Exact tracks that have already stopped. LiveKit webhooks are delivered
    // at-least-once and can arrive out of order, so a delayed publish for a
    // track whose unpublish/participant cleanup already arrived must not
    // resurrect a dead stream.
    stopped_screen_shares: Mutex<HashSet<ScreenShareKey>>,
    // Last participant-level disconnect/abort timestamp per screen-share owner.
    // This catches delayed track_published webhooks that arrive after the
    // participant has left but before we ever saw the matching track sid.
    stopped_screen_share_owners: Mutex<HashMap<ScreenShareOwnerKey, i64>>,
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

    /// Record that a participant has connected. A fresh join after a previous
    /// leave allows new screen-share tracks from the same identity while stale
    /// joins older than the disconnect keep the participant-level stop marker.
    pub fn mark_participant_connected(
        &self,
        channel_id: i64,
        sharer_user_id: i64,
        participant_identity: &str,
        connected_at: i64,
    ) {
        let owner_key = ScreenShareOwnerKey {
            channel_id,
            sharer_user_id,
            participant_identity: participant_identity.to_owned(),
        };
        let mut stopped_owners = match self.stopped_screen_share_owners.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let should_clear = stopped_owners
            .get(&owner_key)
            .is_some_and(|stopped_at| connected_at > *stopped_at);
        if should_clear {
            stopped_owners.remove(&owner_key);
        }
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

    pub fn screen_share_streams(&self, channel_id: Option<i64>) -> Vec<ScreenShareStream> {
        let streams = match self.screen_shares.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut out: Vec<ScreenShareStream> = streams
            .values()
            .filter(|stream| channel_id.is_none_or(|id| stream.channel_id == id))
            .cloned()
            .collect();
        out.sort_by(|a, b| {
            a.started_at
                .cmp(&b.started_at)
                .then_with(|| a.channel_id.cmp(&b.channel_id))
                .then_with(|| a.sharer_user_id.cmp(&b.sharer_user_id))
                .then_with(|| a.participant_identity.cmp(&b.participant_identity))
                .then_with(|| a.track_sid.cmp(&b.track_sid))
        });
        out
    }

    /// Insert an active screen-share stream.
    ///
    /// Duplicate LiveKit publish webhooks for the exact same track are ignored.
    /// A different screen-share track from the same sharer in the same channel
    /// replaces the previous one so a user cannot accumulate active streams.
    pub fn add_screen_share_stream(&self, stream: ScreenShareStream) -> AddScreenShareStreamResult {
        let mut streams = match self.screen_shares.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let key = stream.key();
        let stopped_tracks = match self.stopped_screen_shares.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if stopped_tracks.contains(&key) {
            return AddScreenShareStreamResult::Unchanged;
        }
        drop(stopped_tracks);

        let owner_key = stream.owner_key();
        let stopped_owners = match self.stopped_screen_share_owners.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if stopped_owners
            .get(&owner_key)
            .is_some_and(|stopped_at| stream.started_at <= *stopped_at)
        {
            return AddScreenShareStreamResult::Unchanged;
        }
        drop(stopped_owners);

        if streams.contains_key(&key) {
            return AddScreenShareStreamResult::Unchanged;
        }

        let replaced_key = streams.iter().find_map(|(existing_key, existing)| {
            (existing.owner_key() == owner_key).then(|| existing_key.clone())
        });
        let replaced = replaced_key.and_then(|existing_key| streams.remove(&existing_key));
        streams.insert(key, stream);

        match replaced {
            Some(previous) => AddScreenShareStreamResult::Replaced(previous),
            None => AddScreenShareStreamResult::Added,
        }
    }

    /// Remove an exact active stream. Duplicate/unordered unpublish webhooks
    /// return `None` so callers can skip broadcasting redundant stop events.
    /// The stopped track key is still remembered so a delayed publish for the
    /// same track cannot resurrect a dead stream.
    pub fn remove_screen_share_stream(
        &self,
        channel_id: i64,
        sharer_user_id: i64,
        participant_identity: &str,
        track_sid: &str,
    ) -> Option<ScreenShareStream> {
        let key = ScreenShareKey {
            channel_id,
            sharer_user_id,
            participant_identity: participant_identity.to_owned(),
            track_sid: track_sid.to_owned(),
        };
        let removed = {
            let mut streams = match self.screen_shares.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            streams.remove(&key)
        };
        let mut stopped = match self.stopped_screen_shares.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        stopped.insert(key);
        removed
    }

    /// Remove all active screen-share streams for a leaving/aborted participant
    /// and remember both exact stopped tracks and a participant-level stop time.
    pub fn remove_screen_share_streams_for_participant(
        &self,
        channel_id: i64,
        sharer_user_id: i64,
        participant_identity: &str,
        stopped_at: i64,
    ) -> Vec<ScreenShareStream> {
        let owner_key = ScreenShareOwnerKey {
            channel_id,
            sharer_user_id,
            participant_identity: participant_identity.to_owned(),
        };
        let removed = {
            let mut streams = match self.screen_shares.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let keys = streams
                .iter()
                .filter(|(_, stream)| stream.owner_key() == owner_key)
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|key| streams.remove(&key))
                .collect::<Vec<_>>()
        };

        let mut stopped_tracks = match self.stopped_screen_shares.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        stopped_tracks.extend(removed.iter().map(ScreenShareStream::key));
        drop(stopped_tracks);

        let mut stopped_owners = match self.stopped_screen_share_owners.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        stopped_owners
            .entry(owner_key)
            .and_modify(|existing| *existing = (*existing).max(stopped_at))
            .or_insert(stopped_at);

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

    fn screen_share(channel_id: i64, user_id: i64, track_sid: &str) -> ScreenShareStream {
        ScreenShareStream {
            channel_id,
            sharer_user_id: user_id,
            username: format!("user{user_id}"),
            display_name: None,
            avatar_url: None,
            participant_identity: user_id.to_string(),
            track_sid: track_sid.to_owned(),
            track_name: "screen".to_owned(),
            source: "screen_share".to_owned(),
            started_at: channel_id + user_id,
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

    #[test]
    fn screen_share_streams_allow_multiple_sharers_and_channels() {
        let state = VoiceState::new();

        assert_eq!(
            state.add_screen_share_stream(screen_share(1, 10, "TR_alice")),
            AddScreenShareStreamResult::Added
        );
        assert_eq!(
            state.add_screen_share_stream(screen_share(1, 11, "TR_bob")),
            AddScreenShareStreamResult::Added
        );
        assert_eq!(
            state.add_screen_share_stream(screen_share(2, 10, "TR_alice_other_channel")),
            AddScreenShareStreamResult::Added
        );

        let channel_one = state.screen_share_streams(Some(1));
        assert_eq!(channel_one.len(), 2);
        assert_eq!(
            channel_one
                .iter()
                .map(|stream| stream.track_sid.as_str())
                .collect::<Vec<_>>(),
            vec!["TR_alice", "TR_bob"]
        );

        let all_tracks = state
            .screen_share_streams(None)
            .into_iter()
            .map(|stream| stream.track_sid)
            .collect::<Vec<_>>();
        assert_eq!(all_tracks.len(), 3);
        assert!(all_tracks.contains(&"TR_alice".to_owned()));
        assert!(all_tracks.contains(&"TR_bob".to_owned()));
        assert!(all_tracks.contains(&"TR_alice_other_channel".to_owned()));
    }

    #[test]
    fn screen_share_streams_replace_same_user_in_same_channel() {
        let state = VoiceState::new();
        let first = screen_share(1, 10, "TR_first");
        let second = screen_share(1, 10, "TR_second");

        assert_eq!(
            state.add_screen_share_stream(first.clone()),
            AddScreenShareStreamResult::Added
        );
        assert_eq!(
            state.add_screen_share_stream(first),
            AddScreenShareStreamResult::Unchanged
        );
        assert_eq!(
            state.add_screen_share_stream(second),
            AddScreenShareStreamResult::Replaced(screen_share(1, 10, "TR_first"))
        );

        let streams = state.screen_share_streams(Some(1));
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].track_sid, "TR_second");
    }

    #[test]
    fn screen_share_stop_before_publish_does_not_resurrect_track() {
        let state = VoiceState::new();

        assert!(
            state
                .remove_screen_share_stream(1, 10, "10", "TR_delayed")
                .is_none()
        );
        assert_eq!(
            state.add_screen_share_stream(screen_share(1, 10, "TR_delayed")),
            AddScreenShareStreamResult::Unchanged
        );
        assert!(state.screen_share_streams(Some(1)).is_empty());
    }

    #[test]
    fn participant_cleanup_removes_streams_and_blocks_older_delayed_publishes() {
        let state = VoiceState::new();
        let mut active = screen_share(1, 10, "TR_active");
        active.started_at = 10;
        assert_eq!(
            state.add_screen_share_stream(active.clone()),
            AddScreenShareStreamResult::Added
        );

        let removed = state.remove_screen_share_streams_for_participant(1, 10, "10", 20);
        assert_eq!(removed, vec![active]);
        assert!(state.screen_share_streams(Some(1)).is_empty());

        let mut delayed = screen_share(1, 10, "TR_delayed_old");
        delayed.started_at = 15;
        assert_eq!(
            state.add_screen_share_stream(delayed),
            AddScreenShareStreamResult::Unchanged
        );

        let mut fresh = screen_share(1, 10, "TR_fresh_new");
        fresh.started_at = 25;
        assert_eq!(
            state.add_screen_share_stream(fresh),
            AddScreenShareStreamResult::Added
        );
    }
}
