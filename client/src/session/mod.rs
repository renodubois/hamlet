//! Application-lifetime session coordination, independent of either rendered screen.
//! The host delivers opaque updates and observes lifecycle changes; it never dispatches auth.
//! Saved-login workflows share this owner, never the lifetime of a rendered screen.

mod saved_login;
mod state;
use crate::storage::{Config, Persistence};
pub(crate) use saved_login::StorageRetry;
use saved_login::{SavedLogin, SavedUpdate};

use crate::api::HttpTransport;
use crate::conversation::{ConversationHandle, SessionEnd};
use crate::runtime::{Execution, Work};
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Lifecycle {
    Authenticated,
    Invalidated,
    ServerChanged,
}

/// Opaque executor delivery. Only the coordinator interprets results/identities.
pub(crate) struct SessionUpdate(Update);
enum Update {
    Authentication(
        LoginRequest,
        Result<crate::api::Authentication, ApiError>,
        bool,
    ),
    Expiry(u64),
    Revocation(u64, Result<(), ApiError>),
    Saved(SavedUpdate),
}

pub(crate) struct SessionCoordinator {
    state: AppSession,
    api: HttpTransport,
    execution: Execution,
    updates: async_channel::Receiver<SessionUpdate>,
    deliver: async_channel::Sender<SessionUpdate>,
    expiry: Option<Work>,
    lifecycle: Option<Lifecycle>,
    saved: SavedLogin,
    conversation: Option<ConversationHandle>,
}

impl SessionCoordinator {
    pub fn new(
        api: HttpTransport,
        execution: Execution,
        config: Config,
        persistence: Option<Persistence>,
    ) -> Self {
        let (deliver, updates) = async_channel::unbounded();
        let mut state = AppSession::new();
        state.change_server(
            config
                .server
                .clone()
                .unwrap_or_else(|| DEFAULT_SERVER_URL.into()),
        );
        let saved = SavedLogin::new(config, persistence, &state.server);
        let mut coordinator = Self {
            state,
            api,
            execution,
            updates,
            deliver,
            expiry: None,
            lifecycle: None,
            saved,
            conversation: None,
        };
        coordinator.start_saved_login();
        coordinator
    }

    pub fn updates(&self) -> async_channel::Receiver<SessionUpdate> {
        self.updates.clone()
    }
    pub fn server(&self) -> &str {
        &self.state.server
    }
    pub fn pending(&self) -> bool {
        self.state.pending
    }
    pub fn feedback(&self) -> Option<&str> {
        self.state.feedback.as_deref()
    }
    pub fn active(&self) -> Option<&Session> {
        self.state.active.as_ref()
    }
    pub fn session_generation(&self) -> Option<u64> {
        self.state.session_generation()
    }
    pub fn conversation(&self) -> Option<ConversationHandle> {
        self.conversation.clone()
    }
    pub fn conversation_ended(&mut self, end: SessionEnd) {
        match end {
            SessionEnd::Rejected(generation) => self.protected_rejected(generation),
            SessionEnd::Expired(generation) if self.session_generation() == Some(generation) => {
                self.expire(self.execution.unix_seconds());
            }
            SessionEnd::Expired(_) => {}
        }
    }
    fn close_conversation(&mut self) {
        if let Some(conversation) = self.conversation.take() {
            conversation.close();
        }
    }
    pub fn take_lifecycle(&mut self) -> Option<Lifecycle> {
        self.lifecycle.take()
    }

    pub fn submit(&mut self, username: String, password: String, signup: bool) {
        let request = if signup {
            self.state.submit_signup(&username, &password)
        } else {
            self.state.submit(&username, &password)
        };
        let Some(request) = request else {
            return;
        };
        let server = self.api.server(&request.server);
        let deliver = self.deliver.clone();
        self.execution.spawn(async move {
            let result = match server {
                Ok(server) if signup => server.signup(username, password).await,
                Ok(server) => server.login(username, password).await,
                Err(error) => Err(error),
            };
            let _ = deliver
                .send(SessionUpdate(Update::Authentication(
                    request, result, signup,
                )))
                .await;
        });
    }

    pub fn apply(&mut self, update: SessionUpdate) {
        match update.0 {
            Update::Saved(update) => self.apply_saved(update),
            Update::Authentication(request, result, signup) => {
                let accepted = if signup {
                    self.state
                        .complete_signup(request, result, self.execution.unix_seconds())
                } else {
                    self.state
                        .complete_login(request, result, self.execution.unix_seconds())
                };
                if accepted && self.active().is_some() {
                    self.activated(true);
                }
            }
            Update::Expiry(generation) => {
                if self.session_generation() == Some(generation) {
                    self.expire(self.execution.unix_seconds());
                }
            }
            Update::Revocation(generation, result) => {
                self.state.revocation_result(generation, result)
            }
        }
    }

    fn activated(&mut self, save: bool) {
        self.cancel_expiry();
        let session = self.active().unwrap();
        let generation = self.session_generation().unwrap();
        let delay = Duration::from_secs(
            session
                .expires_at
                .saturating_sub(self.execution.unix_seconds())
                .max(0) as u64,
        );
        let deliver = self.deliver.clone();
        let timer = self.execution.sleep(delay);
        // Owned cancellable timer; dispatch and generation gating are session policy.
        let (work, _) = self.execution.start(async move {
            timer.await;
            let _ = deliver
                .send(SessionUpdate(Update::Expiry(generation)))
                .await;
        });
        self.expiry = Some(work);
        self.close_conversation();
        let session = self.active().unwrap();
        self.conversation = Some(ConversationHandle::new(
            generation,
            session.expires_at,
            session.client(),
            self.execution.clone(),
        ));
        self.lifecycle = Some(Lifecycle::Authenticated);
        if save {
            self.save_login();
        }
    }

    fn cancel_expiry(&mut self) {
        if let Some(work) = self.expiry.take() {
            work.abort();
        }
    }
    fn invalidated(&mut self) {
        self.close_conversation();
        self.cancel_expiry();
        self.lifecycle = Some(Lifecycle::Invalidated);
        self.invalidate_storage();
    }
    pub fn change_server(&mut self, server: String) {
        if self.server() != server {
            self.close_conversation();
            self.state.change_server(server);
            self.cancel_expiry();
            self.lifecycle = Some(Lifecycle::ServerChanged);
            self.invalidate_storage();
        }
    }
    pub fn cancel_pending(&mut self) {
        if self.active().is_none() {
            self.state.cancel_pending();
        }
    }
    pub fn server_changed_feedback(&mut self) {
        self.state.feedback = Some("Server changed. Enter your password again.".into());
    }
    pub fn logout(&mut self) {
        let revocation = self.state.logout();
        self.invalidated();
        if let Some(revocation) = revocation {
            let deliver = self.deliver.clone();
            self.execution.spawn(async move {
                let result = revocation.client.logout().await;
                let _ = deliver
                    .send(SessionUpdate(Update::Revocation(
                        revocation.generation,
                        result,
                    )))
                    .await;
            });
        }
    }
    pub fn expire(&mut self, now: i64) {
        let was_active = self.active().is_some();
        self.state.expire(now);
        if was_active && self.active().is_none() {
            self.invalidated();
        }
    }
    pub fn protected_rejected(&mut self, generation: u64) {
        if self.session_generation() == Some(generation) {
            self.state.protected_rejected(generation);
            self.invalidated();
        }
    }

    pub fn restore_pending(&self) -> bool {
        self.state.restore_pending()
    }
}

impl Drop for SessionCoordinator {
    fn drop(&mut self) {
        self.close_conversation();
        self.cancel_expiry();
    }
}

use crate::api::ApiError;
use state::{AppSession, LoginRequest, RestoreDecision, RestoreResult};
pub(crate) use state::{DEFAULT_SERVER_URL, Session};

impl Session {
    /// Credential material goes directly from the accepted context to the ordered worker.
    fn save(
        &self,
        store: &crate::storage::Persistence,
    ) -> async_channel::Receiver<crate::storage::Outcome> {
        store.save(
            crate::storage::Selection {
                server: self.server.clone(),
                user: self.user.clone(),
                expires_at: self.expires_at,
            },
            self.client().credential_for_session().to_owned(),
        )
    }
}

impl SessionCoordinator {
    /// The candidate remains inside this future until current-user verification completes.
    /// Identity, expiry and generation still gate activation in the saved-login workflow.
    async fn verify_saved(server: crate::api::ServerClient, token: String) -> RestoreResult {
        let Ok(client) = server.restore_candidate(token) else {
            return RestoreResult::MissingCredential;
        };
        match client.current_user().await {
            Ok(user) => RestoreResult::Verified { user, client },
            Err(ApiError::AlreadyInvalid) => RestoreResult::Rejected,
            _ => RestoreResult::Unavailable,
        }
    }
}

#[cfg(test)]
#[path = "tests/coordinator.rs"]
mod coordinator_tests;
#[cfg(test)]
#[path = "tests/route.rs"]
mod route_tests;
#[cfg(test)]
#[path = "tests/saved_login.rs"]
mod saved_login_tests;
