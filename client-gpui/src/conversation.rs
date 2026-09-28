use crate::session::{AppSession, AuthError};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Channel {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    pub author_id: String,
    pub author_name: String,
    pub text: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    // Server order: newest first, with the server's tie breaker intact.
    pub items: Vec<Message>,
    pub next_cursor: Option<String>,
}

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

pub struct HistoryOutcome {
    pub added: usize,
    pub next: Option<ReadRequest>,
    pub prepend: bool,
}

#[derive(Clone, Debug)]
pub struct ReadRequest {
    pub generation: u64,
    pub server: String,
    pub token: String,
    pub channel_id: Option<String>,
    pub before: Option<String>,
    pub serial: u64,
    refresh: bool,
}

#[derive(Clone, Debug)]
pub struct CreateRequest {
    pub generation: u64,
    pub server: String,
    pub token: String,
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
}

impl Conversation {
    pub fn channel_refreshing(&self) -> bool {
        self.channel_pending
    }

    pub fn clear(&mut self) {
        // Keep serials distinct even when the same session remains active after a reset.
        let next = self.create_serial.wrapping_add(1);
        let read_next = self.read_serial.wrapping_add(1);
        let channel_next = self.channel_serial.wrapping_add(1);
        *self = Self::default();
        self.create_serial = next;
        self.read_serial = read_next;
        self.channel_serial = channel_next;
    }

    pub fn create(&mut self, session: &AppSession, name: &str) -> Option<CreateRequest> {
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
        let active = session.active.as_ref()?;
        self.create_serial = self.create_serial.wrapping_add(1);
        self.create_pending = true;
        self.create_feedback = None;
        Some(CreateRequest {
            generation: session.session_generation()?,
            server: active.server.clone(),
            token: active.token().to_owned(),
            name: normalized.into(),
            serial: self.create_serial,
        })
    }

    pub fn complete_create(
        &mut self,
        session: &mut AppSession,
        request: &CreateRequest,
        result: Result<Channel, AuthError>,
        now: i64,
    ) -> (bool, Option<ReadRequest>) {
        if !self.create_pending
            || self.create_serial != request.serial
            || session.session_generation() != Some(request.generation)
            || session.active.as_ref().is_none_or(|active| {
                active.server != request.server || active.token() != request.token
            })
        {
            return (false, None);
        }
        session.expire(now);
        if session.active.is_none() {
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
            Err(AuthError::AlreadyInvalid) => {
                session.protected_rejected(request.generation);
                self.clear();
                (false, None)
            }
            Err(error) => {
                self.create_feedback = Some(match error {
                    AuthError::Conflict => "Channel name already exists. Choose another name.".into(),
                    AuthError::InvalidInput => "The server rejected this channel name. Check the name and try again.".into(),
                    AuthError::Unavailable | AuthError::InvalidResponse | AuthError::ServerFailure => "Could not confirm channel creation; it may have succeeded. Check the channel list before submitting again.".into(),
                    _ => error.description().into(),
                });
                (false, None)
            }
        }
    }

    pub fn start(&mut self, session: &AppSession) -> Option<ReadRequest> {
        session.active.as_ref()?;
        if self.channels.is_some() {
            return None;
        }
        self.channels = Some(Load::Loading);
        self.read_channels(session)
    }

    pub fn refresh_channels(&mut self, session: &AppSession) -> Option<ReadRequest> {
        if self.channel_pending {
            return None;
        }
        if !matches!(self.channels, Some(Load::Ready(_))) {
            self.channels = Some(Load::Loading);
        }
        self.read_channels(session)
    }

    fn read_channels(&mut self, session: &AppSession) -> Option<ReadRequest> {
        let active = session.active.as_ref()?;
        self.channel_pending = true;
        self.channel_error = None;
        self.channel_serial = self.channel_serial.wrapping_add(1);
        Some(ReadRequest {
            generation: session.session_generation()?,
            server: active.server.clone(),
            token: active.token().to_owned(),
            channel_id: None,
            before: None,
            serial: self.channel_serial,
            refresh: false,
        })
    }

    pub fn complete_channels(
        &mut self,
        session: &mut AppSession,
        request: &ReadRequest,
        result: Result<Vec<Channel>, AuthError>,
    ) -> Option<ReadRequest> {
        if request.channel_id.is_some()
            || session.session_generation() != Some(request.generation)
            || !self.channel_pending
            || self.channel_serial != request.serial
            || session.active.as_ref().is_none_or(|active| {
                active.server != request.server || active.token() != request.token
            })
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
            Err(AuthError::AlreadyInvalid) => {
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
            }
        }
        self.catchup = None;
        self.read_serial = self.read_serial.wrapping_add(1);
    }

    pub fn select(&mut self, session: &AppSession, id: &str) -> Option<ReadRequest> {
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
        let active = session.active.as_ref()?;
        self.history.insert(id.into(), Load::Loading);
        self.read_serial = self.read_serial.wrapping_add(1);
        Some(ReadRequest {
            generation: session.session_generation()?,
            server: active.server.clone(),
            token: active.token().to_owned(),
            channel_id: Some(id.into()),
            before: None,
            serial: self.read_serial,
            refresh: false,
        })
    }

    /// Begin a newest-first reconciliation; only commit when overlap or exhaustion proves continuity.
    pub fn refresh_history(&mut self, session: &AppSession) -> Option<ReadRequest> {
        let id = self.selected.clone()?;
        if matches!(self.history.get(&id), Some(Load::Loading))
            || matches!(self.refreshing.get(&id), Some(Refresh::Running))
        {
            return None;
        }
        if !matches!(self.history.get(&id), Some(Load::Ready(_))) {
            return self.select(session, &id);
        }
        let active = session.active.as_ref()?;
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
            server: active.server.clone(),
            token: active.token().into(),
            channel_id: Some(id),
            before: None,
            serial: self.read_serial,
            refresh: true,
        })
    }

    /// Begin one deliberate older-page read for the currently selected channel.
    pub fn request_older(&mut self, session: &AppSession) -> Option<ReadRequest> {
        let id = self.selected.as_ref()?;
        if matches!(self.refreshing.get(id), Some(Refresh::Running))
            || !matches!(self.older.get(id), Some(Older::Available))
        {
            return None;
        }
        let active = session.active.as_ref()?;
        let before = self.cursors.get(id)?.clone();
        self.older.insert(id.clone(), Older::Loading);
        self.read_serial = self.read_serial.wrapping_add(1);
        Some(ReadRequest {
            generation: session.session_generation()?,
            server: active.server.clone(),
            token: active.token().to_owned(),
            channel_id: Some(id.clone()),
            before: Some(before),
            serial: self.read_serial,
            refresh: false,
        })
    }

    pub fn retry_older(&mut self, session: &AppSession) -> Option<ReadRequest> {
        let id = self.selected.as_ref()?;
        if matches!(self.refreshing.get(id), Some(Refresh::Running))
            || !matches!(self.older.get(id), Some(Older::Failed(_)))
        {
            return None;
        }
        self.older.insert(id.clone(), Older::Available);
        self.request_older(session)
    }

    pub fn complete_history(
        &mut self,
        session: &mut AppSession,
        request: &ReadRequest,
        result: Result<Page, AuthError>,
    ) -> HistoryOutcome {
        let mut outcome = HistoryOutcome {
            added: 0,
            next: None,
            prepend: false,
        };
        let Some(id) = &request.channel_id else {
            return outcome;
        };
        if session.session_generation() != Some(request.generation)
            || session.active.as_ref().is_none_or(|active| {
                active.server != request.server || active.token() != request.token
            })
        {
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
                                server: request.server.clone(),
                                token: request.token.clone(),
                                channel_id: Some(id.clone()),
                                before: Some(cursor),
                                serial: self.read_serial,
                                refresh: true,
                            });
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
                outcome.prepend = older;
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
            Err(AuthError::AlreadyInvalid) => {
                session.protected_rejected(request.generation);
                self.clear();
                outcome
            }
            Err(error) => {
                if request.refresh {
                    self.catchup = None;
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
mod tests {
    use super::*;
    use crate::session::{ApiFuture, AuthApi, Login, User};
    use std::sync::Arc;

    struct Stub;
    impl AuthApi for Stub {
        fn signup(&self, _: String, _: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
            Box::pin(async { unreachable!() })
        }
        fn login(&self, _: String, _: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
            Box::pin(async { unreachable!() })
        }
        fn logout(&self, _: String, _: String) -> ApiFuture<Result<(), AuthError>> {
            Box::pin(async { Ok(()) })
        }
        fn channels(&self, _: String, _: String) -> ApiFuture<Result<Vec<Channel>, AuthError>> {
            Box::pin(async { unreachable!() })
        }
        fn create_channel(
            &self,
            _: String,
            _: String,
            _: String,
        ) -> ApiFuture<Result<Channel, AuthError>> {
            Box::pin(async { unreachable!() })
        }
        fn history(
            &self,
            _: String,
            _: String,
            _: String,
        ) -> ApiFuture<Result<Vec<Message>, AuthError>> {
            Box::pin(async { unreachable!() })
        }
    }
    fn logged_in() -> AppSession {
        let mut session = AppSession::new(Arc::new(Stub));
        session.username = "ada".into();
        session.password = "password".into();
        let request = session.submit().unwrap();
        session.complete_login(
            request,
            Ok(Login {
                user: User {
                    id: "u".into(),
                    username: "ada".into(),
                },
                token: "token".into(),
                expires_at: 100,
            }),
            0,
        );
        session
    }
    fn channels() -> Vec<Channel> {
        vec![
            Channel {
                id: "z".into(),
                name: "Zebra".into(),
            },
            Channel {
                id: "a".into(),
                name: "alpha".into(),
            },
        ]
    }
    #[test]
    fn ordered_channels_first_selection_and_only_selected_history() {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let list = view.start(&session).unwrap();
        let first = view
            .complete_channels(&mut session, &list, Ok(channels()))
            .unwrap();
        assert_eq!(view.selected.as_deref(), Some("z"));
        assert_eq!(first.channel_id.as_deref(), Some("z"));
        assert_eq!(view.history.len(), 1);
        assert!(view.select(&session, "unknown").is_none());
        assert!(view.select(&session, "z").is_none());
        assert_eq!(
            view.select(&session, "a").unwrap().channel_id.as_deref(),
            Some("a")
        );
    }
    #[test]
    fn empty_failed_and_tied_order_without_resorting() {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let list = view.start(&session).unwrap();
        assert!(
            view.complete_channels(&mut session, &list, Ok(vec![]))
                .is_none()
        );
        assert!(view.selected.is_none());
        view.clear();
        let list = view.start(&session).unwrap();
        view.complete_channels(&mut session, &list, Err(AuthError::Unavailable));
        assert!(matches!(view.channels, Some(Load::Failed(_))));
        view.clear();
        let list = view.start(&session).unwrap();
        let request = view
            .complete_channels(&mut session, &list, Ok(channels()))
            .unwrap();
        let messages = ["9", "8"]
            .map(|id| Message {
                id: id.into(),
                channel_id: "z".into(),
                author_id: "u".into(),
                author_name: "Ada".into(),
                text: id.into(),
                created_at: "same".into(),
            })
            .to_vec();
        view.complete_history(
            &mut session,
            &request,
            Ok(Page {
                items: messages.clone(),
                next_cursor: None,
            }),
        );
        assert_eq!(view.history.get("z"), Some(&Load::Ready(messages)));
    }
    #[test]
    fn out_of_order_reads_and_expired_sessions_do_not_replace_selection() {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let list = view.start(&session).unwrap();
        let old = view
            .complete_channels(&mut session, &list, Ok(channels()))
            .unwrap();
        let current = view.select(&session, "a").unwrap();
        view.complete_history(&mut session, &old, Err(AuthError::Unavailable));
        assert_eq!(view.selected.as_deref(), Some("a"));
        assert!(
            !view.history.contains_key("z"),
            "canceled reads do not cache stale failures"
        );
        view.complete_history(
            &mut session,
            &current,
            Ok(Page {
                items: vec![],
                next_cursor: None,
            }),
        );
        assert_eq!(view.history.get("a"), Some(&Load::Ready(vec![])));
        session.logout();
        view.clear();
        view.complete_history(&mut session, &old, Err(AuthError::AlreadyInvalid));
        assert!(view.history.is_empty());
        assert!(view.selected.is_none());
        assert!(session.active.is_none());
    }

    #[test]
    fn old_server_responses_cannot_repopulate_a_new_session() {
        let mut session = logged_in();
        let mut conversation = Conversation::default();
        let old_channels = conversation.start(&session).unwrap();
        let old_history = conversation
            .complete_channels(&mut session, &old_channels, Ok(channels()))
            .unwrap();
        session.change_server("https://another.example".into());
        conversation.clear();
        session.username = "bob".into();
        session.password = "password".into();
        let login = session.submit().unwrap();
        session.complete_login(
            login,
            Ok(Login {
                user: User {
                    id: "other".into(),
                    username: "bob".into(),
                },
                token: "new-token".into(),
                expires_at: 100,
            }),
            0,
        );
        conversation.complete_history(&mut session, &old_history, Err(AuthError::AlreadyInvalid));
        assert!(
            conversation
                .complete_channels(&mut session, &old_channels, Ok(channels()))
                .is_none()
        );
        assert_eq!(session.active.as_ref().unwrap().user.username, "bob");
        assert!(conversation.channels.is_none());
        assert!(conversation.history.is_empty());
    }

    #[test]
    fn reselecting_a_failed_channel_retries_without_losing_successful_history() {
        let mut session = logged_in();
        let mut conversation = Conversation::default();
        let request = conversation.start(&session).unwrap();
        let first = conversation
            .complete_channels(&mut session, &request, Ok(channels()))
            .unwrap();
        conversation.complete_history(&mut session, &first, Err(AuthError::Unavailable));
        let other = conversation.select(&session, "a").unwrap();
        conversation.complete_history(
            &mut session,
            &other,
            Ok(Page {
                items: vec![],
                next_cursor: None,
            }),
        );
        let retry = conversation
            .select(&session, "z")
            .expect("failed channel must retry");
        assert!(matches!(conversation.history.get("z"), Some(Load::Loading)));
        conversation.complete_history(
            &mut session,
            &retry,
            Ok(Page {
                items: vec![],
                next_cursor: None,
            }),
        );
        assert_eq!(conversation.history.get("z"), Some(&Load::Ready(vec![])));
        assert!(
            conversation.select(&session, "a").is_none(),
            "successful history is cached"
        );
    }

    #[test]
    fn late_create_cannot_navigate_after_logout_or_server_switch() {
        let mut session = logged_in();
        let mut conversation = Conversation::default();
        let list = conversation.start(&session).unwrap();
        conversation.complete_channels(&mut session, &list, Ok(vec![]));
        let old = conversation.create(&session, "New").unwrap();
        assert!(conversation.create(&session, "New").is_none());
        session.logout();
        conversation.clear();
        assert!(
            !conversation
                .complete_create(
                    &mut session,
                    &old,
                    Ok(Channel {
                        id: "3".into(),
                        name: "New".into()
                    }),
                    0
                )
                .0
        );
        assert!(conversation.channels.is_none());
        session.change_server("https://other.example".into());
        session.username = "other".into();
        session.password = "password".into();
        let login_request = session.submit().unwrap();
        session.complete_login(
            login_request,
            Ok(Login {
                user: User {
                    id: "other".into(),
                    username: "other".into(),
                },
                token: "new-token".into(),
                expires_at: 100,
            }),
            0,
        );
        let list = conversation.start(&session).unwrap();
        conversation.complete_channels(&mut session, &list, Ok(channels()));
        let selection = conversation.selected.clone();
        assert!(
            !conversation
                .complete_create(&mut session, &old, Err(AuthError::AlreadyInvalid), 0)
                .0
        );
        assert_eq!(conversation.selected, selection);
        assert!(session.active.is_some());
    }

    #[test]
    fn an_expired_session_cannot_accept_late_channel_creation() {
        let mut session = logged_in();
        let mut conversation = Conversation::default();
        let list = conversation.start(&session).unwrap();
        conversation.complete_channels(&mut session, &list, Ok(vec![]));
        let create = conversation.create(&session, "New").unwrap();
        let (confirmed, history) = conversation.complete_create(
            &mut session,
            &create,
            Ok(Channel {
                id: "3".into(),
                name: "New".into(),
            }),
            100,
        );
        assert!(!confirmed);
        assert!(history.is_none());
        assert!(session.active.is_none());
        assert!(conversation.channels.is_none());
        assert!(conversation.selected.is_none());
    }

    #[test]
    fn older_pages_merge_identity_in_server_order_retry_and_stop_at_exhaustion() {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let channels = view.start(&session).unwrap();
        let first = view
            .complete_channels(&mut session, &channels, Ok(super::tests::channels()))
            .unwrap();
        let message = |id: &str| Message {
            id: id.into(),
            channel_id: "z".into(),
            author_id: "u".into(),
            author_name: "Ada".into(),
            text: id.into(),
            created_at: "same".into(),
        };
        view.complete_history(
            &mut session,
            &first,
            Ok(Page {
                items: vec![message("5"), message("4"), message("3")],
                next_cursor: Some("opaque one".into()),
            }),
        );
        let older = view.request_older(&session).unwrap();
        assert_eq!(older.before.as_deref(), Some("opaque one"));
        assert!(view.request_older(&session).is_none());
        view.complete_history(&mut session, &older, Err(AuthError::Unavailable));
        assert_eq!(
            view.history.get("z").as_ref().unwrap(),
            &&Load::Ready(vec![message("5"), message("4"), message("3")])
        );
        assert!(matches!(view.older.get("z"), Some(Older::Failed(_))));
        assert!(view.request_older(&session).is_none());
        let retry = view.retry_older(&session).unwrap();
        assert_eq!(retry.before, older.before);
        assert_eq!(
            view.complete_history(
                &mut session,
                &older,
                Ok(Page {
                    items: vec![message("2")],
                    next_cursor: None
                })
            )
            .added,
            0
        );
        assert_eq!(
            view.complete_history(
                &mut session,
                &retry,
                Ok(Page {
                    items: vec![message("3"), message("2"), message("1"), message("2")],
                    next_cursor: Some("opaque two".into()),
                })
            )
            .added,
            2
        );
        let final_read = view.request_older(&session).unwrap();
        assert_eq!(final_read.before.as_deref(), Some("opaque two"));
        assert_eq!(
            view.complete_history(
                &mut session,
                &final_read,
                Ok(Page {
                    items: vec![message("1"), message("0")],
                    next_cursor: None,
                })
            )
            .added,
            1
        );
        assert_eq!(
            view.history.get("z"),
            Some(&Load::Ready(
                ["5", "4", "3", "2", "1", "0"].map(message).to_vec()
            ))
        );
        assert_eq!(view.older.get("z"), Some(&Older::Exhausted));
        assert!(view.request_older(&session).is_none());
    }

    #[test]
    fn switching_or_invalidating_session_discards_older_completions() {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let channels = view.start(&session).unwrap();
        let first = view
            .complete_channels(&mut session, &channels, Ok(super::tests::channels()))
            .unwrap();
        view.complete_history(
            &mut session,
            &first,
            Ok(Page {
                items: vec![],
                next_cursor: Some("cursor".into()),
            }),
        );
        let old = view.request_older(&session).unwrap();
        let other = view.select(&session, "a").unwrap();
        view.complete_history(&mut session, &old, Err(AuthError::AlreadyInvalid));
        assert!(session.active.is_some());
        assert_eq!(view.selected.as_deref(), Some("a"));
        view.complete_history(
            &mut session,
            &other,
            Ok(Page {
                items: vec![],
                next_cursor: Some("other cursor".into()),
            }),
        );
        let pending = view.request_older(&session).unwrap();
        session.logout();
        view.clear();
        view.complete_history(
            &mut session,
            &pending,
            Ok(Page {
                items: vec![],
                next_cursor: None,
            }),
        );
        assert!(view.history.is_empty());
        assert!(view.older.is_empty());
    }

    #[test]
    fn creating_a_channel_cancels_the_old_selected_page_and_allows_reselection() {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let channels = view.start(&session).unwrap();
        let first = view
            .complete_channels(&mut session, &channels, Ok(super::tests::channels()))
            .unwrap();
        view.complete_history(
            &mut session,
            &first,
            Ok(Page {
                items: vec![],
                next_cursor: Some("opaque server cursor".into()),
            }),
        );
        let pending = view.request_older(&session).unwrap();
        let create = view.create(&session, "New").unwrap();
        let (confirmed, next) = view.complete_create(
            &mut session,
            &create,
            Ok(Channel {
                id: "new".into(),
                name: "New".into(),
            }),
            0,
        );
        assert!(confirmed);
        assert_eq!(next.unwrap().channel_id.as_deref(), Some("new"));
        assert_eq!(view.older.get("z"), Some(&Older::Available));
        view.select(&session, "z");
        let retry = view
            .request_older(&session)
            .expect("canceled traversal is retryable");
        assert_eq!(retry.before.as_deref(), Some("opaque server cursor"));
        assert_eq!(
            view.complete_history(&mut session, &pending, Err(AuthError::AlreadyInvalid))
                .added,
            0
        );
        assert!(session.active.is_some());
    }

    #[test]
    fn selected_older_page_rejection_invalidates_session_and_clears_history() {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let channels = view.start(&session).unwrap();
        let first = view
            .complete_channels(&mut session, &channels, Ok(super::tests::channels()))
            .unwrap();
        view.complete_history(
            &mut session,
            &first,
            Ok(Page {
                items: vec![],
                next_cursor: Some("opaque".into()),
            }),
        );
        let older = view.request_older(&session).unwrap();
        view.complete_history(&mut session, &older, Err(AuthError::AlreadyInvalid));
        assert!(session.active.is_none());
        assert!(view.history.is_empty());
        assert!(view.older.is_empty());
        assert!(view.selected.is_none());
    }

    #[test]
    fn catchup_waits_for_overlap_across_opaque_pages_and_recovers_after_failure() {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let list = view.start(&session).unwrap();
        let initial = view
            .complete_channels(&mut session, &list, Ok(channels()))
            .unwrap();
        let message = |id: &str| Message {
            id: id.into(),
            channel_id: "z".into(),
            author_id: "u".into(),
            author_name: "Ada".into(),
            text: id.into(),
            created_at: "same".into(),
        };
        let page = |ids: &[&str], cursor: Option<&str>| Page {
            items: ids.iter().map(|id| message(id)).collect(),
            next_cursor: cursor.map(str::to_owned),
        };
        view.complete_history(
            &mut session,
            &initial,
            Ok(page(&["3", "2", "1"], Some("older"))),
        );
        let refresh = view.refresh_history(&session).unwrap();
        assert!(view.refresh_history(&session).is_none());
        assert!(view.request_older(&session).is_none());
        let pending = view
            .complete_history(
                &mut session,
                &refresh,
                Ok(page(&["9", "8"], Some("opaque /?+"))),
            )
            .next
            .unwrap();
        assert_eq!(pending.before.as_deref(), Some("opaque /?+"));
        assert_eq!(
            view.history.get("z"),
            Some(&Load::Ready(page(&["3", "2", "1"], None).items))
        );
        assert_eq!(view.refreshing.get("z"), Some(&Refresh::Running));
        view.complete_history(&mut session, &pending, Err(AuthError::Unavailable));
        assert!(matches!(
            view.refreshing.get("z"),
            Some(Refresh::Incomplete(_))
        ));
        assert_eq!(
            view.history.get("z"),
            Some(&Load::Ready(page(&["3", "2", "1"], None).items))
        );
        let retry = view.refresh_history(&session).unwrap();
        assert_eq!(
            view.complete_history(&mut session, &pending, Ok(page(&["7"], None)))
                .added,
            0
        );
        let middle = view
            .complete_history(
                &mut session,
                &retry,
                Ok(page(&["9", "8"], Some("opaque /?+"))),
            )
            .next
            .unwrap();
        let last = view
            .complete_history(
                &mut session,
                &middle,
                Ok(page(&["8", "7", "6"], Some("next"))),
            )
            .next
            .unwrap();
        assert_eq!(last.before.as_deref(), Some("next"));
        let outcome = view.complete_history(
            &mut session,
            &last,
            Ok(page(&["6", "3", "2"], Some("unused"))),
        );
        assert!(outcome.next.is_none());
        assert_eq!(outcome.added, 4);
        assert_eq!(
            view.history.get("z"),
            Some(&Load::Ready(
                page(&["9", "8", "7", "6", "3", "2", "1"], None).items
            ))
        );
        assert!(!view.refreshing.contains_key("z"));
        assert_eq!(view.older.get("z"), Some(&Older::Available));
    }

    #[test]
    fn catchup_from_an_empty_loaded_conversation_waits_until_exhaustion() {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let list = view.start(&session).unwrap();
        let initial = view
            .complete_channels(&mut session, &list, Ok(channels()))
            .unwrap();
        view.complete_history(
            &mut session,
            &initial,
            Ok(Page {
                items: vec![],
                next_cursor: None,
            }),
        );
        let message = |id: &str| Message {
            id: id.into(),
            channel_id: "z".into(),
            author_id: "u".into(),
            author_name: "Ada".into(),
            text: id.into(),
            created_at: "same".into(),
        };
        let refresh = view.refresh_history(&session).unwrap();
        let next = view.complete_history(
            &mut session,
            &refresh,
            Ok(Page {
                items: vec![message("4"), message("3")],
                next_cursor: Some("server cursor".into()),
            }),
        );
        assert_eq!(next.added, 0);
        assert_eq!(view.history.get("z"), Some(&Load::Ready(vec![])));
        let continuation = next.next.unwrap();
        view.complete_history(&mut session, &continuation, Err(AuthError::Unavailable));
        assert_eq!(view.history.get("z"), Some(&Load::Ready(vec![])));
        assert!(matches!(
            view.refreshing.get("z"),
            Some(Refresh::Incomplete(_))
        ));
        let retry = view.refresh_history(&session).unwrap();
        let continuation = view
            .complete_history(
                &mut session,
                &retry,
                Ok(Page {
                    items: vec![message("4"), message("3")],
                    next_cursor: Some("server cursor".into()),
                }),
            )
            .next
            .unwrap();
        let outcome = view.complete_history(
            &mut session,
            &continuation,
            Ok(Page {
                items: vec![message("3"), message("2"), message("1")],
                next_cursor: None,
            }),
        );
        assert_eq!(outcome.added, 4);
        assert!(outcome.next.is_none());
        assert_eq!(
            view.history.get("z"),
            Some(&Load::Ready(vec![
                message("4"),
                message("3"),
                message("2"),
                message("1"),
            ]))
        );
        assert!(!view.refreshing.contains_key("z"));
    }

    #[test]
    fn refresh_supersedes_an_older_read_without_losing_its_cursor() {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let list = view.start(&session).unwrap();
        let first = view
            .complete_channels(&mut session, &list, Ok(channels()))
            .unwrap();
        let message = Message {
            id: "1".into(),
            channel_id: "z".into(),
            author_id: "u".into(),
            author_name: "Ada".into(),
            text: "one".into(),
            created_at: "same".into(),
        };
        view.complete_history(
            &mut session,
            &first,
            Ok(Page {
                items: vec![message.clone()],
                next_cursor: Some("opaque older".into()),
            }),
        );
        let older = view.request_older(&session).unwrap();
        let refresh = view.refresh_history(&session).unwrap();
        assert_eq!(view.older.get("z"), Some(&Older::Available));
        assert_eq!(
            view.complete_history(&mut session, &older, Err(AuthError::AlreadyInvalid))
                .added,
            0
        );
        assert!(session.active.is_some());
        view.complete_history(
            &mut session,
            &refresh,
            Ok(Page {
                items: vec![message.clone()],
                next_cursor: Some("opaque newer".into()),
            }),
        );
        assert_eq!(view.history.get("z"), Some(&Load::Ready(vec![message])));
        assert_eq!(
            view.request_older(&session).unwrap().before.as_deref(),
            Some("opaque older")
        );
    }

    #[test]
    fn disconnected_exhaustion_keeps_contiguous_old_history_and_switch_cancels() {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let list = view.start(&session).unwrap();
        let first = view
            .complete_channels(&mut session, &list, Ok(channels()))
            .unwrap();
        let m = |id: &str| Message {
            id: id.into(),
            channel_id: "z".into(),
            author_id: "u".into(),
            author_name: "Ada".into(),
            text: id.into(),
            created_at: "same".into(),
        };
        view.complete_history(
            &mut session,
            &first,
            Ok(Page {
                items: vec![m("2"), m("1")],
                next_cursor: None,
            }),
        );
        let refresh = view.refresh_history(&session).unwrap();
        view.complete_history(
            &mut session,
            &refresh,
            Ok(Page {
                items: vec![m("9"), m("1")],
                next_cursor: None,
            }),
        );
        assert_eq!(
            view.history.get("z"),
            Some(&Load::Ready(vec![m("2"), m("1")]))
        );
        assert!(matches!(
            view.refreshing.get("z"),
            Some(Refresh::Incomplete(_))
        ));
        let pending = view.refresh_history(&session).unwrap();
        view.select(&session, "a");
        view.complete_history(&mut session, &pending, Err(AuthError::AlreadyInvalid));
        assert!(session.active.is_some());
        assert!(!view.refreshing.contains_key("z"));
    }

    #[test]
    fn discovery_preserves_selection_handles_empty_and_rejects_stale_results() {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let first = view.start(&session).unwrap();
        view.complete_channels(&mut session, &first, Ok(channels()));
        view.select(&session, "a");
        let refresh = view.refresh_channels(&session).unwrap();
        assert!(view.refresh_channels(&session).is_none());
        view.complete_channels(&mut session, &refresh, Err(AuthError::Unavailable));
        assert_eq!(view.selected.as_deref(), Some("a"));
        assert!(matches!(view.channels, Some(Load::Ready(_))));
        assert!(view.channel_error.is_some());
        let refresh = view.refresh_channels(&session).unwrap();
        view.complete_channels(
            &mut session,
            &refresh,
            Ok(vec![
                Channel {
                    id: "new".into(),
                    name: "New".into(),
                },
                channels()[1].clone(),
            ]),
        );
        assert_eq!(view.selected.as_deref(), Some("a"));
        let empty = view.refresh_channels(&session).unwrap();
        view.complete_channels(&mut session, &empty, Ok(vec![]));
        assert_eq!(view.selected, None);
        assert_eq!(view.channels, Some(Load::Ready(vec![])));
        view.complete_channels(&mut session, &refresh, Err(AuthError::AlreadyInvalid));
        assert!(session.active.is_some());
    }

    #[test]
    fn protected_rejection_clears_selection_and_cached_conversation() {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let list = view.start(&session).unwrap();
        let read = view
            .complete_channels(&mut session, &list, Ok(channels()))
            .unwrap();
        view.complete_history(&mut session, &read, Err(AuthError::AlreadyInvalid));
        assert!(session.active.is_none());
        assert!(view.selected.is_none());
        assert!(view.channels.is_none());
        assert!(view.history.is_empty());
    }
}
