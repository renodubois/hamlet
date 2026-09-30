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
// Temporary import compatibility; server data is API-owned.
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refresh {
    Running,
    Incomplete(String),
}

#[derive(Clone, Debug)]
struct Catchup {
    id: String,
    items: Vec<Message>,
    cursors: HashSet<String>,
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
    pub next: Option<ReadRequest>,
}

#[derive(Clone)]
pub(super) struct ReadRequest {
    pub generation: u64,
    pub channel_id: Option<String>,
    pub before: Option<String>,
    pub serial: u64,
    refresh: bool,
}

impl ReadRequest {
    pub fn is_catchup(&self) -> bool {
        self.refresh
    }
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
    pub refreshing: HashMap<String, Refresh>,
    catchup: Option<Catchup>,
    channel_pending: bool,
    channel_serial: u64,
    pub channel_error: Option<String>,
    read_serial: u64,
    pub create_pending: bool,
    pub create_feedback: Option<String>,
    create_serial: u64,
    pub drafts: HashMap<String, String>,
    pub send_pending: HashMap<String, u64>,
    pub send_feedback: HashMap<String, String>,
    pub uncertain: HashSet<String>,
    uncertain_notice: HashSet<String>,
    confirmed: HashMap<String, Vec<Message>>,
    confirmed_refresh: HashSet<String>,
    confirmed_during_refresh: HashSet<String>,
    send_serial: u64,
}

impl Conversation {
    pub fn channel_refreshing(&self) -> bool {
        self.channel_pending
    }

    pub(super) fn clear(&mut self) {
        // Keep serials distinct even when the same session remains active after a reset.
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
                let Some(Load::Ready(channels)) = &mut self.channels else {
                    return (false, None);
                };
                // Match the server's ORDER BY name_key ASC, id ASC; do not sort by display case.
                let key = (channel.name.to_ascii_lowercase(), channel.id.clone());
                let position = channels.partition_point(|item| {
                    (item.name.to_ascii_lowercase(), item.id.clone()) < key
                });
                let id = channel.id.clone();
                channels.insert(position, channel);
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
                self.send_feedback.insert(id.clone(), "Message accepted by server; waiting for conversation catch-up. Refresh conversation if catch-up is incomplete.".into());
                // Preserve the previous head as the continuity boundary. A confirmation
                // must not make catch-up stop before intervening publications are read.
                self.confirmed.entry(id.clone()).or_default().push(message);
                if matches!(self.refreshing.get(id), Some(Refresh::Running)) {
                    self.confirmed_during_refresh.insert(id.clone());
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

    pub(super) fn reconcile_confirmed(&mut self, session: &Identity) -> Option<ReadRequest> {
        let id = self.selected.as_ref()?;
        if !self.confirmed.contains_key(id) || self.confirmed_refresh.contains(id) {
            return None;
        }
        let id = id.clone();
        let request = self.refresh_history(session);
        if request.as_ref().is_some_and(|request| request.refresh) {
            self.confirmed_refresh.insert(id);
        }
        request
    }

    /// After any history read, run only the reconciliation that can safely start now.
    /// Incomplete catch-up always requires a deliberate retry, never an automatic loop.
    pub(super) fn after_history_read(
        &mut self,
        session: &Identity,
        succeeded: bool,
    ) -> Option<ReadRequest> {
        let id = self.selected.as_ref()?;
        if !succeeded || matches!(self.refreshing.get(id), Some(Refresh::Incomplete(_))) {
            return None;
        }
        self.reconcile_uncertain(session)
            .or_else(|| self.reconcile_confirmed(session))
    }

    fn merge_confirmed(&mut self, id: &str, message: Message) -> usize {
        let Some(Load::Ready(messages)) = self.history.get_mut(id) else {
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

    pub(super) fn reconcile_uncertain(&mut self, session: &Identity) -> Option<ReadRequest> {
        let id = self.selected.clone()?;
        if !self.uncertain.contains(&id) {
            return None;
        }
        let request = self.refresh_history(session);
        if request.is_some() {
            self.uncertain.remove(&id);
        }
        request
    }

    pub(super) fn start(&mut self, session: &Identity) -> Option<ReadRequest> {
        session.session_generation()?;
        if self.channels.is_some() {
            return None;
        }
        self.channels = Some(Load::Loading);
        self.read_channels(session)
    }

    pub(super) fn refresh_channels(&mut self, session: &Identity) -> Option<ReadRequest> {
        if self.channel_pending {
            return None;
        }
        if !matches!(self.channels, Some(Load::Ready(_))) {
            self.channels = Some(Load::Loading);
        }
        self.read_channels(session)
    }

    fn read_channels(&mut self, session: &Identity) -> Option<ReadRequest> {
        session.session_generation()?;
        self.channel_pending = true;
        self.channel_error = None;
        self.channel_serial = self.channel_serial.wrapping_add(1);
        Some(ReadRequest {
            generation: session.session_generation()?,
            channel_id: None,
            before: None,
            serial: self.channel_serial,
            refresh: false,
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
                self.channels = Some(Load::Ready(channels));
                next.and_then(|id| self.select(session, &id))
            }
            Err(ApiError::AlreadyInvalid) => {
                session.protected_rejected(request.generation);
                self.clear();
                None
            }
            Err(error) => {
                if matches!(self.channels, Some(Load::Ready(_))) {
                    self.channel_error = Some(error.description().into());
                } else {
                    self.channels = Some(Load::Failed(error.description().into()));
                }
                None
            }
        }
    }

    fn cancel_selected(&mut self) {
        if let Some(old) = &self.selected {
            if matches!(self.history.get(old), Some(Load::Loading)) {
                self.history.remove(old);
            }
            if matches!(self.older.get(old), Some(Older::Loading)) {
                self.older.insert(old.clone(), Older::Available);
            }
            if matches!(self.refreshing.get(old), Some(Refresh::Running)) {
                self.refreshing.remove(old);
                self.confirmed_refresh.remove(old);
                self.confirmed_during_refresh.remove(old);
            }
        }
        self.catchup = None;
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
        if self.uncertain.contains(id) && matches!(self.history.get(id), Some(Load::Ready(_))) {
            return self.reconcile_uncertain(session);
        }
        if self.confirmed.contains_key(id) && matches!(self.history.get(id), Some(Load::Ready(_))) {
            return self.reconcile_confirmed(session);
        }
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
            refresh: false,
        })
    }

    /// Begin a newest-first reconciliation; only commit when overlap or exhaustion proves continuity.
    pub(super) fn refresh_history(&mut self, session: &Identity) -> Option<ReadRequest> {
        let id = self.selected.clone()?;
        if matches!(self.history.get(&id), Some(Load::Loading))
            || matches!(self.refreshing.get(&id), Some(Refresh::Running))
        {
            return None;
        }
        if !matches!(self.history.get(&id), Some(Load::Ready(_))) {
            return self.select(session, &id);
        }
        session.session_generation()?;
        // Cancel an older read before starting reconciliation; its late completion is ignored.
        if matches!(self.older.get(&id), Some(Older::Loading)) {
            self.older.insert(id.clone(), Older::Available);
        }
        self.read_serial = self.read_serial.wrapping_add(1);
        self.catchup = Some(Catchup {
            id: id.clone(),
            items: Vec::new(),
            cursors: HashSet::new(),
        });
        self.refreshing.insert(id.clone(), Refresh::Running);
        Some(ReadRequest {
            generation: session.session_generation()?,
            channel_id: Some(id),
            before: None,
            serial: self.read_serial,
            refresh: true,
        })
    }

    /// Begin one deliberate older-page read for the currently selected channel.
    pub(super) fn request_older(&mut self, session: &Identity) -> Option<ReadRequest> {
        let id = self.selected.as_ref()?;
        if matches!(self.refreshing.get(id), Some(Refresh::Running))
            || !matches!(self.older.get(id), Some(Older::Available))
        {
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
            refresh: false,
        })
    }

    pub(super) fn retry_older(&mut self, session: &Identity) -> Option<ReadRequest> {
        let id = self.selected.as_ref()?;
        if matches!(self.refreshing.get(id), Some(Refresh::Running))
            || !matches!(self.older.get(id), Some(Older::Failed(_)))
        {
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
        let mut outcome = HistoryOutcome {
            added: 0,
            next: None,
        };
        let Some(id) = &request.channel_id else {
            return outcome;
        };
        if session.session_generation() != Some(request.generation) {
            return outcome;
        }
        let older = request.before.is_some() && !request.refresh;
        if self.selected.as_deref() != Some(id) || self.read_serial != request.serial {
            return outcome;
        }
        if request.refresh {
            if !matches!(self.refreshing.get(id), Some(Refresh::Running))
                || self.catchup.as_ref().is_none_or(|c| c.id != *id)
            {
                return outcome;
            }
        } else if older {
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
                if request.refresh {
                    let catchup = self.catchup.as_mut().unwrap();
                    let existing = match self.history.get(id) {
                        Some(Load::Ready(items)) => items,
                        _ => return outcome,
                    };
                    // The newest previously loaded ID is the continuity boundary. Meeting
                    // an older ID alone could skip an intervening message that disappeared.
                    let overlap = existing
                        .first()
                        .is_some_and(|head| page.items.iter().any(|m| m.id == head.id));
                    let mut staged_ids: HashSet<_> =
                        catchup.items.iter().map(|m| m.id.clone()).collect();
                    for item in page.items {
                        if staged_ids.insert(item.id.clone()) {
                            catchup.items.push(item);
                        }
                    }
                    let cursor = page
                        .next_cursor
                        .filter(|c| !c.is_empty() && request.before.as_ref() != Some(c));
                    if overlap || cursor.is_none() {
                        if overlap || existing.is_empty() {
                            let staged = std::mem::take(&mut catchup.items);
                            let mut ids: HashSet<_> = staged.iter().map(|m| m.id.clone()).collect();
                            let mut merged = staged;
                            merged.extend(
                                existing
                                    .iter()
                                    .filter(|m| ids.insert(m.id.clone()))
                                    .cloned(),
                            );
                            outcome.added = merged.len() - existing.len();
                            self.history.insert(id.clone(), Load::Ready(merged));
                            self.refreshing.remove(id);
                        } else {
                            self.refreshing.insert(id.clone(), Refresh::Incomplete("Could not establish continuity with loaded messages. Refresh again to retry.".into()));
                        }
                        self.catchup = None;
                    } else if let Some(cursor) = cursor {
                        if !catchup.cursors.insert(cursor.clone()) {
                            self.refreshing.insert(
                                id.clone(),
                                Refresh::Incomplete(
                                    "Server repeated a history cursor. Refresh again to retry."
                                        .into(),
                                ),
                            );
                            self.catchup = None;
                        } else {
                            self.read_serial = self.read_serial.wrapping_add(1);
                            outcome.next = Some(ReadRequest {
                                generation: request.generation,
                                channel_id: Some(id.clone()),
                                before: Some(cursor),
                                serial: self.read_serial,
                                refresh: true,
                            });
                        }
                    }
                    if outcome.next.is_none() {
                        let confirmed_refresh = self.confirmed_refresh.remove(id);
                        if !matches!(self.refreshing.get(id), Some(Refresh::Incomplete(_))) {
                            // A confirmation after the first page was requested might be absent
                            // from its snapshot. Start a fresh catch-up against the old head.
                            let needs_new_catchup = self.confirmed_during_refresh.remove(id);
                            if confirmed_refresh && !needs_new_catchup {
                                for confirmed in self.confirmed.remove(id).unwrap_or_default() {
                                    outcome.added += self.merge_confirmed(id, confirmed);
                                }
                                self.send_feedback.remove(id);
                            }
                        }
                    }
                    return outcome;
                }
                let existing = if older {
                    match self.history.remove(id) {
                        Some(Load::Ready(items)) => items,
                        _ => return outcome,
                    }
                } else {
                    Vec::new()
                };
                let seen: HashSet<_> = existing.iter().map(|item| item.id.as_str()).collect();
                let mut page_ids = HashSet::new();
                let additions: Vec<_> = page
                    .items
                    .into_iter()
                    .filter(|item| {
                        !seen.contains(item.id.as_str()) && page_ids.insert(item.id.clone())
                    })
                    .collect();
                outcome.added = additions.len();
                let mut messages = existing;
                messages.extend(additions);
                self.history.insert(id.clone(), Load::Ready(messages));
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
                if request.refresh {
                    self.catchup = None;
                    self.confirmed_refresh.remove(id);
                    self.refreshing
                        .insert(id.clone(), Refresh::Incomplete(error.description().into()));
                } else if older {
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
