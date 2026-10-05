//! In-memory channels, histories and per-channel drafts, with pure request/completion transitions.
//! Request identities reject stale results; HTTP confirmations and live creations merge by entity ID.

use crate::api::ApiError;
/// Pure originating-session identity and rejection outcome, never mutable auth state.
pub(super) struct Identity {
    pub generation: Option<u64>,
    pub expires_at: i64,
    pub rejected: Option<u64>,
}
impl Identity {
    pub fn session_generation(&self) -> Option<u64> {
        self.generation
    }
    pub fn expire(&mut self, now: i64) {
        if now >= self.expires_at {
            self.generation = None;
        }
    }
    fn protected_rejected(&mut self, generation: u64) {
        if self.generation == Some(generation) {
            self.rejected = Some(generation);
            self.generation = None;
        }
    }
}
use crate::api::{Channel, Message, Page};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Older {
    Available,
    Loading,
    Failed(String),
    Exhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Load<T> {
    Loading,
    Ready(T),
    Failed(String),
}

// Completion identities contain no endpoint or credential; dispatch captures the accepted client.
#[derive(Clone)]
pub(super) struct SendRequest {
    pub generation: u64,
    pub channel_id: String,
    pub text: String,
    serial: u64,
}

#[derive(PartialEq, Eq, Debug)]
pub(super) enum SendOutcome {
    Stale,
    Confirmed,
    Rejected,
    Uncertain,
    Invalidated,
}

pub(super) struct HistoryOutcome {
    pub added: usize,
}

#[derive(Clone)]
pub(super) struct ReadRequest {
    pub generation: u64,
    pub channel_id: Option<String>,
    pub before: Option<String>,
    pub serial: u64,
}

#[derive(Clone)]
pub(super) struct CreateRequest {
    pub generation: u64,
    pub name: String,
    serial: u64,
}

#[derive(Default)]
pub struct Conversation {
    pub channels: Option<Load<Vec<Channel>>>,
    pub selected: Option<String>,
    pub history: HashMap<String, Load<Vec<Message>>>,
    pub older: HashMap<String, Older>,
    cursors: HashMap<String, String>,
    channel_pending: bool,
    channel_serial: u64,
    read_serial: u64,
    pub create_pending: bool,
    pub create_feedback: Option<String>,
    create_serial: u64,
    pub drafts: HashMap<String, String>,
    pub send_pending: HashMap<String, u64>,
    pub send_feedback: HashMap<String, String>,
    pub uncertain: HashSet<String>,
    uncertain_notice: HashSet<String>,
    // Originating HTTP confirmations awaiting loaded history. Retain across failed or
    // canceled reads, then consume on this channel's first successful history load.
    confirmed: HashMap<String, Vec<Message>>,
    send_serial: u64,
}

impl Conversation {
    pub fn history_for_display(&self, id: &str) -> Option<&Load<Vec<Message>>> {
        self.history.get(id)
    }

    pub(super) fn clear(&mut self) {
        // Keep obsolete completions distinct even if the same session remains active.
        let next = self.create_serial.wrapping_add(1);
        let read_next = self.read_serial.wrapping_add(1);
        let channel_next = self.channel_serial.wrapping_add(1);
        let send_next = self.send_serial.wrapping_add(1);
        *self = Self::default();
        self.send_serial = send_next;
        self.create_serial = next;
        self.read_serial = read_next;
        self.channel_serial = channel_next;
    }

    pub(super) fn create(&mut self, session: &Identity, name: &str) -> Option<CreateRequest> {
        if self.create_pending || !matches!(self.channels, Some(Load::Ready(_))) {
            return None;
        }
        let normalized = name.trim();
        if !(1..=64).contains(&normalized.len())
            || !normalized
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b' ' || b == b'-' || b == b'_')
        {
            self.create_feedback = Some("Channel name must be 1–64 bytes after trimming, using ASCII letters, digits, spaces, hyphens or underscores.".into());
            return None;
        }
        session.session_generation()?;
        self.create_serial = self.create_serial.wrapping_add(1);
        self.create_pending = true;
        self.create_feedback = None;
        Some(CreateRequest {
            generation: session.session_generation()?,
            name: normalized.into(),
            serial: self.create_serial,
        })
    }

    pub(super) fn complete_create(
        &mut self,
        session: &mut Identity,
        request: &CreateRequest,
        result: Result<Channel, ApiError>,
        now: i64,
    ) -> (bool, Option<ReadRequest>) {
        if !self.create_pending
            || self.create_serial != request.serial
            || session.session_generation() != Some(request.generation)
        {
            return (false, None);
        }
        session.expire(now);
        if session.session_generation().is_none() {
            self.clear();
            return (false, None);
        }
        self.create_pending = false;
        match result {
            Ok(channel) => {
                let id = channel.id.clone();
                self.merge_channel(channel);
                self.create_feedback = None;
                (true, self.select(session, &id))
            }
            Err(ApiError::AlreadyInvalid) => {
                session.protected_rejected(request.generation);
                self.clear();
                (false, None)
            }
            Err(error) => {
                self.create_feedback = Some(match error {
                    ApiError::Conflict => "Channel name already exists. Choose another name.".into(),
                    ApiError::InvalidInput => "The server rejected this channel name. Check the name and try again.".into(),
                    ApiError::Unavailable | ApiError::InvalidResponse | ApiError::ServerFailure => "Could not confirm channel creation; it may have succeeded. Check the channel list before submitting again.".into(),
                    _ => error.description().into(),
                });
                (false, None)
            }
        }
    }

    /// Remote creations never change selection or allocate history.
    pub(super) fn merge_channel(&mut self, channel: Channel) -> usize {
        let Some(Load::Ready(channels)) = &mut self.channels else {
            return 0;
        };
        if channels.iter().any(|item| item.id == channel.id) {
            return 0;
        }
        // Match ORDER BY name_key ASC, id ASC without reordering the server snapshot.
        let key = (channel.name.to_ascii_lowercase(), channel.id.clone());
        let position = channels
            .partition_point(|item| (item.name.to_ascii_lowercase(), item.id.clone()) < key);
        channels.insert(position, channel);
        1
    }

    pub fn draft(&self, id: &str) -> &str {
        self.drafts.get(id).map(String::as_str).unwrap_or("")
    }

    pub(super) fn set_draft(&mut self, id: &str, text: String) {
        if !self.send_pending.contains_key(id) {
            self.drafts.insert(id.into(), text);
            if !self.uncertain_notice.contains(id) && !self.confirmed.contains_key(id) {
                self.send_feedback.remove(id);
            }
        }
    }

    pub(super) fn send(&mut self, session: &Identity) -> Option<SendRequest> {
        let id = self.selected.as_ref()?;
        if self.send_pending.contains_key(id) {
            return None;
        }
        let text = self.draft(id).to_owned();
        if text.trim().is_empty() || text.chars().count() > 4000 {
            self.send_feedback.insert(
                id.clone(),
                "Message must have text and be at most 4000 characters.".into(),
            );
            return None;
        }
        session.session_generation()?;
        self.send_serial = self.send_serial.wrapping_add(1);
        self.send_pending.insert(id.clone(), self.send_serial);
        self.uncertain_notice.remove(id);
        self.send_feedback.remove(id);
        Some(SendRequest {
            generation: session.session_generation()?,
            channel_id: id.clone(),
            text,
            serial: self.send_serial,
        })
    }

    pub(super) fn complete_send(
        &mut self,
        session: &mut Identity,
        request: &SendRequest,
        result: Result<Message, ApiError>,
        now: i64,
    ) -> SendOutcome {
        let id = &request.channel_id;
        if self.send_pending.get(id) != Some(&request.serial)
            || session.session_generation() != Some(request.generation)
        {
            return SendOutcome::Stale;
        }
        session.expire(now);
        if session.session_generation().is_none() {
            self.clear();
            return SendOutcome::Invalidated;
        }
        self.send_pending.remove(id);
        match result {
            Ok(message) if !message.id.is_empty() && message.channel_id == *id => {
                self.drafts.remove(id);
                self.send_feedback.remove(id);
                self.uncertain.remove(id);
                self.uncertain_notice.remove(id);
                self.merge_message(message.clone());
                // Loaded history accepts the confirmation directly, without an overlay.
                if !matches!(self.history.get(id), Some(Load::Ready(_))) {
                    self.confirmed.entry(id.clone()).or_default().push(message);
                }
                SendOutcome::Confirmed
            }
            Err(ApiError::AlreadyInvalid) => {
                session.protected_rejected(request.generation);
                self.clear();
                SendOutcome::Invalidated
            }
            Err(
                ApiError::InvalidInput
                | ApiError::NotFound
                | ApiError::Conflict
                | ApiError::InvalidCredentials,
            ) => {
                self.send_feedback.insert(id.clone(), "The server rejected this message. Check the text and channel before sending again.".into());
                SendOutcome::Rejected
            }
            _ => {
                self.uncertain.insert(id.clone());
                self.uncertain_notice.insert(id.clone());
                self.send_feedback.insert(id.clone(), "Could not confirm publication; this message may already have been published. Check the conversation before resending.".into());
                SendOutcome::Uncertain
            }
        }
    }

    /// Apply a validated creation only to loaded history. Events overlapping a history
    /// load may be missed; never allocate history for an unloaded channel's creation.
    pub(super) fn merge_message(&mut self, message: Message) -> usize {
        let Some(Load::Ready(messages)) = self.history.get_mut(&message.channel_id) else {
            return 0;
        };
        if messages.iter().any(|m| m.id == message.id) {
            return 0;
        }
        // The server orders by created_at DESC, id DESC. Compare parsed instants because
        // RFC3339 can spell UTC with differing fractional-second precision.
        let key = chrono::DateTime::parse_from_rfc3339(&message.created_at).ok();
        let at = messages.partition_point(|m| {
            let other = chrono::DateTime::parse_from_rfc3339(&m.created_at).ok();
            other > key
                || (other == key
                    && match (m.id.parse::<u64>(), message.id.parse::<u64>()) {
                        (Ok(a), Ok(b)) => a > b,
                        _ => m.id > message.id,
                    })
        });
        messages.insert(at, message);
        1
    }

    pub(super) fn start(&mut self, session: &Identity) -> Option<ReadRequest> {
        session.session_generation()?;
        if self.channels.is_some() {
            return None;
        }
        self.channels = Some(Load::Loading);
        self.channel_pending = true;
        self.channel_serial = self.channel_serial.wrapping_add(1);
        Some(ReadRequest {
            generation: session.session_generation()?,
            channel_id: None,
            before: None,
            serial: self.channel_serial,
        })
    }

    pub(super) fn is_current_channels(&self, request: &ReadRequest) -> bool {
        request.channel_id.is_none()
            && self.channel_pending
            && self.channel_serial == request.serial
    }

    pub(super) fn complete_channels(
        &mut self,
        session: &mut Identity,
        request: &ReadRequest,
        result: Result<Vec<Channel>, ApiError>,
    ) -> Option<ReadRequest> {
        if request.channel_id.is_some()
            || session.session_generation() != Some(request.generation)
            || !self.channel_pending
            || self.channel_serial != request.serial
        {
            return None;
        }
        self.channel_pending = false;
        match result {
            Ok(channels) => {
                self.channels = Some(Load::Ready(channels));
                let Some(Load::Ready(channels)) = &self.channels else {
                    unreachable!();
                };
                let selected = self
                    .selected
                    .as_ref()
                    .filter(|id| channels.iter().any(|c| &c.id == *id))
                    .cloned();
                let next = selected.or_else(|| channels.first().map(|c| c.id.clone()));
                if self.selected != next {
                    self.cancel_selected();
                    self.selected = None;
                }
                next.and_then(|id| self.select(session, &id))
            }
            Err(ApiError::AlreadyInvalid) => {
                session.protected_rejected(request.generation);
                self.clear();
                None
            }
            Err(error) => {
                self.channels = Some(Load::Failed(error.description().into()));
                None
            }
        }
    }

    pub(super) fn cancel_selected(&mut self) {
        if let Some(old) = &self.selected {
            if matches!(self.history.get(old), Some(Load::Loading)) {
                self.history.remove(old);
            }
            if matches!(self.older.get(old), Some(Older::Loading)) {
                self.older.insert(old.clone(), Older::Available);
            }
        }
        self.read_serial = self.read_serial.wrapping_add(1);
    }

    pub(super) fn select(&mut self, session: &Identity, id: &str) -> Option<ReadRequest> {
        let Some(Load::Ready(channels)) = &self.channels else {
            return None;
        };
        if !channels.iter().any(|channel| channel.id == id) {
            return None;
        }
        if self.selected.as_deref() != Some(id) {
            self.cancel_selected();
        }
        self.selected = Some(id.into());
        if matches!(self.history.get(id), Some(Load::Loading | Load::Ready(_))) {
            return None;
        }
        session.session_generation()?;
        self.history.insert(id.into(), Load::Loading);
        self.read_serial = self.read_serial.wrapping_add(1);
        Some(ReadRequest {
            generation: session.session_generation()?,
            channel_id: Some(id.into()),
            before: None,
            serial: self.read_serial,
        })
    }

    /// Begin one deliberate older-page read for the currently selected channel.
    pub(super) fn request_older(&mut self, session: &Identity) -> Option<ReadRequest> {
        let id = self.selected.as_ref()?;
        if !matches!(self.older.get(id), Some(Older::Available)) {
            return None;
        }
        session.session_generation()?;
        let before = self.cursors.get(id)?.clone();
        self.older.insert(id.clone(), Older::Loading);
        self.read_serial = self.read_serial.wrapping_add(1);
        Some(ReadRequest {
            generation: session.session_generation()?,
            channel_id: Some(id.clone()),
            before: Some(before),
            serial: self.read_serial,
        })
    }

    pub(super) fn retry_older(&mut self, session: &Identity) -> Option<ReadRequest> {
        let id = self.selected.as_ref()?;
        if !matches!(self.older.get(id), Some(Older::Failed(_))) {
            return None;
        }
        self.older.insert(id.clone(), Older::Available);
        self.request_older(session)
    }

    pub(super) fn complete_history(
        &mut self,
        session: &mut Identity,
        request: &ReadRequest,
        result: Result<Page, ApiError>,
    ) -> HistoryOutcome {
        let mut outcome = HistoryOutcome { added: 0 };
        let Some(id) = &request.channel_id else {
            return outcome;
        };
        if session.session_generation() != Some(request.generation) {
            return outcome;
        }
        let older = request.before.is_some();
        if self.selected.as_deref() != Some(id) || self.read_serial != request.serial {
            return outcome;
        }
        if older {
            if self.cursors.get(id) != request.before.as_ref()
                || !matches!(self.older.get(id), Some(Older::Loading))
            {
                return outcome;
            }
        } else if !matches!(self.history.get(id), Some(Load::Loading)) {
            return outcome;
        }
        match result {
            Ok(page) => {
                if !older {
                    self.history.insert(id.clone(), Load::Ready(Vec::new()));
                }
                for message in page.items {
                    outcome.added += self.merge_message(message);
                }
                if !older {
                    // HTTP confirmations belong to the originating write, unlike remote
                    // events. Consume the overlay once its history is loaded.
                    for message in self.confirmed.remove(id).unwrap_or_default() {
                        outcome.added += self.merge_message(message);
                    }
                }
                match page
                    .next_cursor
                    .filter(|cursor| !cursor.is_empty() && request.before.as_ref() != Some(cursor))
                {
                    Some(cursor) => {
                        self.cursors.insert(id.clone(), cursor);
                        self.older.insert(id.clone(), Older::Available);
                    }
                    None => {
                        self.cursors.remove(id);
                        self.older.insert(id.clone(), Older::Exhausted);
                    }
                }
                outcome
            }
            Err(ApiError::AlreadyInvalid) => {
                session.protected_rejected(request.generation);
                self.clear();
                outcome
            }
            Err(error) => {
                if older {
                    self.older
                        .insert(id.clone(), Older::Failed(error.description().into()));
                } else {
                    self.history
                        .insert(id.clone(), Load::Failed(error.description().into()));
                }
                outcome
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/state.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/live_state.rs"]
mod live_tests;
