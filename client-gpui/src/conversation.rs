use crate::session::{AppSession, AuthError};
use std::collections::HashMap;

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
pub enum Load<T> {
    Loading,
    Ready(T),
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct ReadRequest {
    pub generation: u64,
    pub server: String,
    pub token: String,
    pub channel_id: Option<String>,
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
    pub create_pending: bool,
    pub create_feedback: Option<String>,
    create_serial: u64,
}

impl Conversation {
    pub fn clear(&mut self) {
        // Keep serials distinct even when the same session remains active after a reset.
        let next = self.create_serial.wrapping_add(1);
        *self = Self::default();
        self.create_serial = next;
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
        let active = session.active.as_ref()?;
        if self.channels.is_some() {
            return None;
        }
        self.channels = Some(Load::Loading);
        Some(ReadRequest {
            generation: session.session_generation()?,
            server: active.server.clone(),
            token: active.token().to_owned(),
            channel_id: None,
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
            || !matches!(self.channels, Some(Load::Loading))
        {
            return None;
        }
        match result {
            Ok(channels) => {
                self.selected = channels.first().map(|channel| channel.id.clone());
                self.channels = Some(Load::Ready(channels));
                self.selected
                    .clone()
                    .and_then(|id| self.select(session, &id))
            }
            Err(AuthError::AlreadyInvalid) => {
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

    pub fn select(&mut self, session: &AppSession, id: &str) -> Option<ReadRequest> {
        let Some(Load::Ready(channels)) = &self.channels else {
            return None;
        };
        if !channels.iter().any(|channel| channel.id == id) {
            return None;
        }
        self.selected = Some(id.into());
        if matches!(self.history.get(id), Some(Load::Loading | Load::Ready(_))) {
            return None;
        }
        let active = session.active.as_ref()?;
        self.history.insert(id.into(), Load::Loading);
        Some(ReadRequest {
            generation: session.session_generation()?,
            server: active.server.clone(),
            token: active.token().to_owned(),
            channel_id: Some(id.into()),
        })
    }

    pub fn complete_history(
        &mut self,
        session: &mut AppSession,
        request: &ReadRequest,
        result: Result<Vec<Message>, AuthError>,
    ) {
        let Some(id) = &request.channel_id else {
            return;
        };
        if session.session_generation() != Some(request.generation)
            || !matches!(self.history.get(id), Some(Load::Loading))
        {
            return;
        }
        match result {
            Ok(messages) => {
                // The server returns newest first, with ID as the tie breaker. Never sort here.
                self.history.insert(id.clone(), Load::Ready(messages));
            }
            Err(AuthError::AlreadyInvalid) => {
                session.protected_rejected(request.generation);
                self.clear();
            }
            Err(error) => {
                self.history
                    .insert(id.clone(), Load::Failed(error.description().into()));
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
        view.complete_history(&mut session, &request, Ok(messages.clone()));
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
        assert!(matches!(view.history.get("z"), Some(Load::Failed(_))));
        view.complete_history(&mut session, &current, Ok(vec![]));
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
        conversation.complete_history(&mut session, &other, Ok(vec![]));
        let retry = conversation
            .select(&session, "z")
            .expect("failed channel must retry");
        assert!(matches!(conversation.history.get("z"), Some(Load::Loading)));
        conversation.complete_history(&mut session, &retry, Ok(vec![]));
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
