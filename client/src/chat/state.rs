//! In-memory channels and per-channel conversations, with pure request/completion transitions.
//! Request identities reject stale results; HTTP confirmations and live creations merge by entity ID.

use super::message_deletion::MessageDeletion;
use crate::api::{ApiError, Channel, Message, Page};
use std::collections::{HashMap, HashSet};

/// The originating session of this chat, and whether it may still start protected work.
/// Pure identity and rejection outcome, never mutable authentication state.
pub(super) struct Identity {
    /// The accepted session generation this chat belongs to; constant for its lifetime.
    pub session: u64,
    pub expires_at: i64,
    active: bool,
    rejected: bool,
}
impl Identity {
    pub fn new(session: u64, expires_at: i64) -> Self {
        Self {
            session,
            expires_at,
            active: true,
            rejected: false,
        }
    }
    /// The generation new requests capture, or `None` once expired, rejected or closed.
    pub fn session_generation(&self) -> Option<u64> {
        self.active.then_some(self.session)
    }
    pub fn expire(&mut self, now: i64) {
        if now >= self.expires_at {
            self.active = false;
        }
    }
    pub fn close(&mut self) {
        self.active = false;
    }
    /// An authoritative 401 for a request captured under the still-active session.
    pub fn protected_rejected(&mut self, generation: u64) {
        if self.session_generation() == Some(generation) {
            self.rejected = true;
            self.active = false;
        }
    }
    /// Reports a rejection once, so the coordinator can end the session by identity.
    pub fn take_rejected(&mut self) -> bool {
        std::mem::take(&mut self.rejected)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Load<T> {
    Loading,
    Ready(T),
    Failed(String),
}

/// Whether a loaded history can page further back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OlderPages {
    Available,
    Loading,
    Failed(String),
    Exhausted,
}

/// Dialog-facing status of one kind of channel write: create, rename or delete.
#[derive(Default)]
pub struct Operation {
    pub pending: bool,
    pub feedback: Option<String>,
    /// Advances on each confirmed success; a dialog closes once it moves past its snapshot.
    pub revision: u64,
    serial: u64,
}
impl Operation {
    fn begin(&mut self, serial: u64) {
        self.serial = serial;
        self.pending = true;
        self.feedback = None;
    }
    /// Consumes the pending status if `serial` is the current request.
    fn finish(&mut self, serial: u64) -> bool {
        if !self.pending || self.serial != serial {
            return false;
        }
        self.pending = false;
        true
    }
    fn confirm(&mut self) {
        self.feedback = None;
        self.revision = self.revision.wrapping_add(1);
    }
    fn fail(&mut self, feedback: impl Into<String>) {
        self.feedback = Some(feedback.into());
    }
    pub(super) fn reset_feedback(&mut self) {
        if !self.pending {
            self.feedback = None;
        }
    }
}

/// Everything the chat holds for one channel. Dropped together when the channel is deleted.
#[derive(Default)]
pub struct Conversation {
    /// `None` until the channel is first selected.
    pub history: Option<Load<Vec<Message>>>,
    pub older: Option<OlderPages>,
    cursor: Option<String>,
    pub draft: Option<String>,
    /// The serial of the one in-flight send, if any.
    sending: Option<u64>,
    pub send_feedback: Option<String>,
    /// A send whose publication could not be confirmed. Never resent automatically.
    pub uncertain: bool,
    uncertain_notice: bool,
    // Originating HTTP confirmations awaiting loaded history. Retained across failed or
    // canceled reads, then consumed on this channel's first successful history load.
    unloaded_confirmations: Vec<Message>,
    // Chat-lifetime observations, including identities not loaded at event time.
    deleted_messages: HashSet<String>,
}
impl Conversation {
    pub fn is_sending(&self) -> bool {
        self.sending.is_some()
    }
}

// Completion identities contain no endpoint or credential; dispatch captures the accepted client.
#[derive(Clone)]
pub(super) struct ChannelsRead {
    pub generation: u64,
    serial: u64,
}

#[derive(Clone)]
pub(super) struct HistoryRead {
    pub generation: u64,
    pub channel_id: String,
    pub before: Option<String>,
    serial: u64,
}

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
pub(super) struct CreateRequest {
    pub generation: u64,
    pub name: String,
    serial: u64,
}

#[derive(Clone)]
pub(super) struct RenameRequest {
    pub generation: u64,
    pub channel_id: String,
    pub name: String,
    serial: u64,
}

#[derive(Clone)]
pub(super) struct ChannelDeleteRequest {
    pub generation: u64,
    pub channel_id: String,
    serial: u64,
}

const CHANNEL_NAME_RULE: &str = "Channel name must be 1–64 bytes after trimming, using ASCII letters, digits, spaces, hyphens or underscores.";
const CHANNEL_NAME_EXISTS: &str = "Channel name already exists. Choose another name.";
const CHANNEL_NAME_REJECTED: &str =
    "The server rejected this channel name. Check the name and try again.";
const CHANNEL_NOT_FOUND: &str = "Channel not found.";

/// The trimmed name, if it satisfies the server's channel-name rule.
fn channel_name(name: &str) -> Option<&str> {
    let name = name.trim();
    ((1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b' ' || b == b'-' || b == b'_'))
    .then_some(name)
}

/// Server order: name_key ASC, id ASC.
fn channel_key(channel: &Channel) -> (String, String) {
    (channel.name.to_ascii_lowercase(), channel.id.clone())
}

#[derive(Default)]
pub struct ChatState {
    pub channels: Option<Load<Vec<Channel>>>,
    pub selected: Option<String>,
    pub(super) conversations: HashMap<String, Conversation>,
    // Chat-lifetime channel deletions; filter late lists, events and confirmations.
    deleted_channels: HashSet<String>,
    pub create: Operation,
    /// The name of the most recently confirmed creation, paired with `create.revision`.
    pub(super) last_created: Option<String>,
    pub rename: Operation,
    pub channel_delete: Operation,
    pub message_deletion: Option<MessageDeletion>,
    channels_read: Option<u64>,
    // The current selected-history read; advancing it abandons any read in flight.
    read_serial: u64,
    // One counter for every request identity, kept across `clear` so obsolete completions
    // never match a newer request.
    next_serial: u64,
}

impl ChatState {
    pub fn selected_channel(&self) -> Option<&Channel> {
        let id = self.selected.as_deref()?;
        let Some(Load::Ready(channels)) = &self.channels else {
            return None;
        };
        channels.iter().find(|channel| channel.id == id)
    }

    pub fn conversation(&self, id: &str) -> Option<&Conversation> {
        self.conversations.get(id)
    }

    pub fn history(&self, id: &str) -> Option<&Load<Vec<Message>>> {
        self.conversation(id)?.history.as_ref()
    }

    pub fn older(&self, id: &str) -> Option<&OlderPages> {
        self.conversation(id)?.older.as_ref()
    }

    pub fn draft(&self, id: &str) -> &str {
        self.conversation(id)
            .and_then(|conversation| conversation.draft.as_deref())
            .unwrap_or("")
    }

    pub fn is_sending(&self, id: &str) -> bool {
        self.conversation(id).is_some_and(Conversation::is_sending)
    }

    pub fn send_feedback(&self, id: &str) -> Option<&str> {
        self.conversation(id)?.send_feedback.as_deref()
    }

    /// Every channel's held state, for asserting chat-wide cleanup.
    #[cfg(test)]
    pub(crate) fn conversations(&self) -> &HashMap<String, Conversation> {
        &self.conversations
    }

    fn conversation_mut(&mut self, id: &str) -> &mut Conversation {
        self.conversations.entry(id.to_owned()).or_default()
    }

    pub(super) fn next_serial(&mut self) -> u64 {
        self.next_serial = self.next_serial.wrapping_add(1);
        self.next_serial
    }

    fn channel_is_loaded(&self, id: &str) -> bool {
        matches!(&self.channels, Some(Load::Ready(channels)) if channels.iter().any(|channel| channel.id == id))
    }

    pub(super) fn clear(&mut self) {
        let next_serial = self.next_serial;
        *self = Self::default();
        self.next_serial = next_serial;
    }

    /// Clears state after an authoritative 401 for a request from this session.
    fn rejected(&mut self, session: &mut Identity, generation: u64) {
        session.protected_rejected(generation);
        self.clear();
    }

    pub(super) fn channel_delete(
        &mut self,
        session: &Identity,
        id: &str,
    ) -> Option<ChannelDeleteRequest> {
        if self.channel_delete.pending
            || self.deleted_channels.contains(id)
            || !self.channel_is_loaded(id)
        {
            return None;
        }
        let generation = session.session_generation()?;
        let serial = self.next_serial();
        self.channel_delete.begin(serial);
        Some(ChannelDeleteRequest {
            generation,
            channel_id: id.into(),
            serial,
        })
    }

    pub(super) fn complete_channel_delete(
        &mut self,
        session: &mut Identity,
        request: &ChannelDeleteRequest,
        result: Result<(), ApiError>,
    ) -> Option<HistoryRead> {
        if session.session_generation() != Some(request.generation)
            || !self.channel_delete.finish(request.serial)
        {
            return None;
        }
        match result {
            Ok(()) => {
                self.channel_delete.confirm();
                return self.merge_channel_deletion(session, &request.channel_id);
            }
            Err(ApiError::SessionRejected) => self.rejected(session, request.generation),
            Err(error) => self.channel_delete.fail(match error {
                ApiError::Conflict => "Cannot delete the last active channel. Create another channel first.",
                ApiError::NotFound => CHANNEL_NOT_FOUND,
                ApiError::InvalidInput => "The server rejected this channel deletion.",
                _ => "Could not confirm channel deletion; it may have succeeded. Check the channel list before submitting again.",
            }),
        }
        None
    }

    /// Shared local/live cleanup. Dialog operations keep their own HTTP outcome;
    /// a delivered deletion is never proof that an originating request succeeded.
    pub(super) fn merge_channel_deletion(
        &mut self,
        session: &Identity,
        id: &str,
    ) -> Option<HistoryRead> {
        if !self.deleted_channels.insert(id.to_owned()) {
            return None;
        }
        if let Some(Load::Ready(channels)) = &mut self.channels {
            channels.retain(|channel| channel.id != id);
        }
        if self
            .message_deletion
            .as_ref()
            .is_some_and(|d| d.channel_id == id)
        {
            self.message_deletion = None;
        }
        self.conversations.remove(id);
        if self.selected.as_deref() == Some(id) {
            self.cancel_selected();
            self.selected = None;
            let next = match &self.channels {
                Some(Load::Ready(channels)) => channels.first().map(|channel| channel.id.clone()),
                _ => None,
            };
            return next.and_then(|id| self.select(session, &id));
        }
        None
    }

    pub(super) fn rename(
        &mut self,
        session: &Identity,
        id: &str,
        name: &str,
    ) -> Option<RenameRequest> {
        if self.rename.pending || !self.channel_is_loaded(id) {
            return None;
        }
        let generation = session.session_generation()?;
        let Some(name) = channel_name(name) else {
            self.rename.fail(CHANNEL_NAME_RULE);
            return None;
        };
        let serial = self.next_serial();
        self.rename.begin(serial);
        Some(RenameRequest {
            generation,
            channel_id: id.into(),
            name: name.into(),
            serial,
        })
    }

    pub(super) fn complete_rename(
        &mut self,
        session: &mut Identity,
        request: &RenameRequest,
        result: Result<Channel, ApiError>,
    ) {
        if session.session_generation() != Some(request.generation)
            || !self.rename.finish(request.serial)
        {
            return;
        }
        match result {
            Ok(channel) if channel.id == request.channel_id => {
                if self.deleted_channels.contains(&channel.id) {
                    self.rename.fail(CHANNEL_NOT_FOUND);
                } else {
                    self.merge_rename(channel);
                    self.rename.confirm();
                }
            }
            Err(ApiError::SessionRejected) => self.rejected(session, request.generation),
            result => self.rename.fail(match result {
                Err(ApiError::Conflict) => CHANNEL_NAME_EXISTS,
                Err(ApiError::InvalidInput) => CHANNEL_NAME_REJECTED,
                Err(ApiError::NotFound) => CHANNEL_NOT_FOUND,
                _ => "Could not confirm channel rename; it may have succeeded. Check the channel list before submitting again.",
            }),
        }
    }

    pub(super) fn create(&mut self, session: &Identity, name: &str) -> Option<CreateRequest> {
        if self.create.pending || !matches!(self.channels, Some(Load::Ready(_))) {
            return None;
        }
        let Some(name) = channel_name(name) else {
            self.create.fail(CHANNEL_NAME_RULE);
            return None;
        };
        let generation = session.session_generation()?;
        let serial = self.next_serial();
        self.create.begin(serial);
        Some(CreateRequest {
            generation,
            name: name.into(),
            serial,
        })
    }

    pub(super) fn complete_create(
        &mut self,
        session: &mut Identity,
        request: &CreateRequest,
        result: Result<Channel, ApiError>,
        now: i64,
    ) -> Option<HistoryRead> {
        if !self.create.pending
            || self.create.serial != request.serial
            || session.session_generation() != Some(request.generation)
        {
            return None;
        }
        session.expire(now);
        if session.session_generation().is_none() {
            self.clear();
            return None;
        }
        self.create.finish(request.serial);
        match result {
            Ok(channel) => {
                let id = channel.id.clone();
                if self.deleted_channels.contains(&id) {
                    self.create.fail(CHANNEL_NOT_FOUND);
                    return None;
                }
                self.merge_channel(channel);
                self.create.confirm();
                self.last_created = Some(request.name.clone());
                self.select(session, &id)
            }
            Err(ApiError::SessionRejected) => {
                self.rejected(session, request.generation);
                None
            }
            Err(error) => {
                self.create.fail(match error {
                    ApiError::Conflict => CHANNEL_NAME_EXISTS,
                    ApiError::InvalidInput => CHANNEL_NAME_REJECTED,
                    ApiError::Unavailable
                    | ApiError::InvalidResponse
                    | ApiError::ServerFailure
                    | ApiError::UnexpectedStatus => "Could not confirm channel creation; it may have succeeded. Check the channel list before submitting again.",
                    _ => error.description(),
                });
                None
            }
        }
    }

    /// Rename only locally known identities, without changing conversation or operation state.
    pub(super) fn merge_rename(&mut self, channel: Channel) {
        if self.deleted_channels.contains(&channel.id) {
            return;
        }
        let Some(Load::Ready(channels)) = &mut self.channels else {
            return;
        };
        let Some(existing) = channels.iter_mut().find(|item| item.id == channel.id) else {
            return;
        };
        *existing = channel;
        channels.sort_by_key(channel_key);
    }

    /// Remote creations never change selection or allocate history.
    pub(super) fn merge_channel(&mut self, channel: Channel) -> usize {
        let Some(Load::Ready(channels)) = &mut self.channels else {
            return 0;
        };
        if self.deleted_channels.contains(&channel.id)
            || channels.iter().any(|item| item.id == channel.id)
        {
            return 0;
        }
        // Insert in server order without reordering the server snapshot.
        let key = channel_key(&channel);
        let position = channels.partition_point(|item| channel_key(item) < key);
        channels.insert(position, channel);
        1
    }

    pub(super) fn set_draft(&mut self, id: &str, text: String) {
        if self.deleted_channels.contains(id) || self.is_sending(id) {
            return;
        }
        let conversation = self.conversation_mut(id);
        conversation.draft = Some(text);
        if !conversation.uncertain_notice && conversation.unloaded_confirmations.is_empty() {
            conversation.send_feedback = None;
        }
    }

    pub(super) fn send(&mut self, session: &Identity) -> Option<SendRequest> {
        let id = self.selected.clone()?;
        if self.is_sending(&id) {
            return None;
        }
        let text = self.draft(&id).to_owned();
        if text.trim().is_empty() || text.chars().count() > 4000 {
            self.conversation_mut(&id).send_feedback =
                Some("Message must have text and be at most 4000 characters.".into());
            return None;
        }
        let generation = session.session_generation()?;
        let serial = self.next_serial();
        let conversation = self.conversation_mut(&id);
        conversation.sending = Some(serial);
        conversation.uncertain_notice = false;
        conversation.send_feedback = None;
        Some(SendRequest {
            generation,
            channel_id: id,
            text,
            serial,
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
        if self.conversation(id).and_then(|c| c.sending) != Some(request.serial)
            || session.session_generation() != Some(request.generation)
        {
            return SendOutcome::Stale;
        }
        session.expire(now);
        if session.session_generation().is_none() {
            self.clear();
            return SendOutcome::Invalidated;
        }
        self.conversation_mut(id).sending = None;
        match result {
            Ok(message) if !message.id.is_empty() && message.channel_id == *id => {
                let conversation = self.conversation_mut(id);
                conversation.draft = None;
                conversation.send_feedback = None;
                conversation.uncertain = false;
                conversation.uncertain_notice = false;
                self.merge_message(message.clone());
                // Loaded history accepts the confirmation directly, without an overlay.
                let conversation = self.conversation_mut(id);
                if !conversation.deleted_messages.contains(&message.id)
                    && !matches!(conversation.history, Some(Load::Ready(_)))
                {
                    conversation.unloaded_confirmations.push(message);
                }
                SendOutcome::Confirmed
            }
            Err(ApiError::SessionRejected) => {
                self.rejected(session, request.generation);
                SendOutcome::Invalidated
            }
            Err(
                ApiError::InvalidInput
                | ApiError::NotFound
                | ApiError::Conflict
                | ApiError::InvalidCredentials
                | ApiError::InvalidRequest,
            ) => {
                self.conversation_mut(id).send_feedback = Some(
                    "The server rejected this message. Check the text and channel before sending again.".into(),
                );
                SendOutcome::Rejected
            }
            _ => {
                let conversation = self.conversation_mut(id);
                conversation.uncertain = true;
                conversation.uncertain_notice = true;
                conversation.send_feedback = Some("Could not confirm publication; this message may already have been published. Check the chat before resending.".into());
                SendOutcome::Uncertain
            }
        }
    }

    /// Record before removing, including for unloaded or currently replacing history.
    /// Cursors and read/send operations are independent of displayed message rows.
    pub(super) fn merge_message_deletion(&mut self, channel_id: &str, message_id: &str) {
        if self
            .message_deletion
            .as_ref()
            .is_some_and(|d| d.channel_id == channel_id && d.message_id == message_id)
        {
            self.message_deletion = None;
        }
        if self.deleted_channels.contains(channel_id) {
            return;
        }
        let conversation = self.conversation_mut(channel_id);
        conversation.deleted_messages.insert(message_id.into());
        if let Some(Load::Ready(messages)) = &mut conversation.history {
            messages.retain(|message| message.id != message_id);
        }
        conversation
            .unloaded_confirmations
            .retain(|message| message.id != message_id);
    }

    /// Apply a validated creation only to loaded history. Events overlapping a history
    /// load may be missed; never allocate history for an unloaded channel's creation.
    pub(super) fn merge_message(&mut self, message: Message) -> usize {
        let Some(conversation) = self.conversations.get_mut(&message.channel_id) else {
            return 0;
        };
        if conversation.deleted_messages.contains(&message.id) {
            return 0;
        }
        let Some(Load::Ready(messages)) = &mut conversation.history else {
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

    pub(super) fn start(&mut self, session: &Identity) -> Option<ChannelsRead> {
        let generation = session.session_generation()?;
        if self.channels.is_some() {
            return None;
        }
        self.channels = Some(Load::Loading);
        let serial = self.next_serial();
        self.channels_read = Some(serial);
        Some(ChannelsRead { generation, serial })
    }

    pub(super) fn is_current_channels(&self, request: &ChannelsRead) -> bool {
        self.channels_read == Some(request.serial)
    }

    pub(super) fn complete_channels(
        &mut self,
        session: &mut Identity,
        request: &ChannelsRead,
        result: Result<Vec<Channel>, ApiError>,
    ) -> Option<HistoryRead> {
        if session.session_generation() != Some(request.generation)
            || !self.is_current_channels(request)
        {
            return None;
        }
        self.channels_read = None;
        match result {
            Ok(mut channels) => {
                channels.retain(|channel| !self.deleted_channels.contains(&channel.id));
                let selected = self
                    .selected
                    .as_ref()
                    .filter(|id| channels.iter().any(|c| &c.id == *id))
                    .cloned();
                let next = selected.or_else(|| channels.first().map(|c| c.id.clone()));
                self.channels = Some(Load::Ready(channels));
                if self.selected != next {
                    self.cancel_selected();
                    self.selected = None;
                }
                next.and_then(|id| self.select(session, &id))
            }
            Err(ApiError::SessionRejected) => {
                self.rejected(session, request.generation);
                None
            }
            Err(error) => {
                self.channels = Some(Load::Failed(error.description().into()));
                None
            }
        }
    }

    pub(super) fn cancel_selected(&mut self) {
        if let Some(conversation) = self
            .selected
            .clone()
            .and_then(|id| self.conversations.get_mut(&id))
        {
            if matches!(conversation.history, Some(Load::Loading)) {
                conversation.history = None;
            }
            if matches!(conversation.older, Some(OlderPages::Loading)) {
                conversation.older = Some(OlderPages::Available);
            }
        }
        self.read_serial = self.next_serial();
    }

    pub(super) fn select(&mut self, session: &Identity, id: &str) -> Option<HistoryRead> {
        if !self.channel_is_loaded(id) {
            return None;
        }
        if self.selected.as_deref() != Some(id) {
            self.cancel_selected();
        }
        self.selected = Some(id.into());
        if matches!(self.history(id), Some(Load::Loading | Load::Ready(_))) {
            return None;
        }
        let generation = session.session_generation()?;
        self.conversation_mut(id).history = Some(Load::Loading);
        self.read_serial = self.next_serial();
        Some(HistoryRead {
            generation,
            channel_id: id.into(),
            before: None,
            serial: self.read_serial,
        })
    }

    /// Begin one deliberate older-page read for the currently selected channel.
    pub(super) fn request_older(&mut self, session: &Identity) -> Option<HistoryRead> {
        let id = self.selected.clone()?;
        if !matches!(self.older(&id), Some(OlderPages::Available)) {
            return None;
        }
        let generation = session.session_generation()?;
        let before = self.conversation(&id)?.cursor.clone()?;
        self.conversation_mut(&id).older = Some(OlderPages::Loading);
        self.read_serial = self.next_serial();
        Some(HistoryRead {
            generation,
            channel_id: id,
            before: Some(before),
            serial: self.read_serial,
        })
    }

    pub(super) fn retry_older(&mut self, session: &Identity) -> Option<HistoryRead> {
        let id = self.selected.clone()?;
        if !matches!(self.older(&id), Some(OlderPages::Failed(_))) {
            return None;
        }
        self.conversation_mut(&id).older = Some(OlderPages::Available);
        self.request_older(session)
    }

    pub(super) fn complete_history(
        &mut self,
        session: &mut Identity,
        request: &HistoryRead,
        result: Result<Page, ApiError>,
    ) -> HistoryOutcome {
        let mut outcome = HistoryOutcome { added: 0 };
        let id = &request.channel_id;
        if session.session_generation() != Some(request.generation)
            || self.selected.as_ref() != Some(id)
            || self.read_serial != request.serial
        {
            return outcome;
        }
        let older = request.before.is_some();
        let Some(conversation) = self.conversations.get_mut(id) else {
            return outcome;
        };
        let current = if older {
            conversation.cursor == request.before
                && matches!(conversation.older, Some(OlderPages::Loading))
        } else {
            matches!(conversation.history, Some(Load::Loading))
        };
        if !current {
            return outcome;
        }
        match result {
            Ok(page) => {
                let mut pending = Vec::new();
                if !older {
                    conversation.history = Some(Load::Ready(Vec::new()));
                    // HTTP confirmations belong to the originating write, unlike remote
                    // events. Consume the overlay once its history is loaded.
                    pending = std::mem::take(&mut conversation.unloaded_confirmations);
                }
                match page
                    .next_cursor
                    .filter(|cursor| !cursor.is_empty() && request.before.as_ref() != Some(cursor))
                {
                    Some(cursor) => {
                        conversation.cursor = Some(cursor);
                        conversation.older = Some(OlderPages::Available);
                    }
                    None => {
                        conversation.cursor = None;
                        conversation.older = Some(OlderPages::Exhausted);
                    }
                }
                for message in page.items.into_iter().chain(pending) {
                    outcome.added += self.merge_message(message);
                }
                outcome
            }
            Err(ApiError::SessionRejected) => {
                self.rejected(session, request.generation);
                outcome
            }
            Err(error) => {
                let failure = error.description().to_owned();
                if older {
                    conversation.older = Some(OlderPages::Failed(failure));
                } else {
                    conversation.history = Some(Load::Failed(failure));
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
