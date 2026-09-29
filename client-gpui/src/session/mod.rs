//! Application-lifetime session coordination, independent of either rendered screen.
//! The host delivers opaque updates and observes lifecycle changes; it never dispatches auth.
//! Saved-login workflow decisions remain behind temporary hooks until #55.

mod state;

use crate::api::HttpTransport;
use crate::runtime::{Execution, Work};
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Lifecycle {
    Authenticated { save: bool },
    Invalidated,
    ServerChanged,
}

/// Opaque executor delivery. Only the coordinator interprets results/identities.
pub(crate) struct SessionUpdate(Update);
enum Update {
    Authentication(
        LoginRequest,
        Result<crate::api::Authentication, AuthError>,
        bool,
    ),
    Expiry(u64),
    Revocation(u64, Result<(), AuthError>),
}

pub(crate) struct SessionCoordinator {
    state: AppSession,
    api: HttpTransport,
    execution: Execution,
    updates: async_channel::Receiver<SessionUpdate>,
    deliver: async_channel::Sender<SessionUpdate>,
    expiry: Option<Work>,
    lifecycle: Option<Lifecycle>,
}

impl SessionCoordinator {
    pub fn new(api: HttpTransport, execution: Execution, server: String) -> Self {
        let (deliver, updates) = async_channel::unbounded();
        let mut state = AppSession::new();
        state.change_server(server);
        Self {
            state,
            api,
            execution,
            updates,
            deliver,
            expiry: None,
            lifecycle: None,
        }
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
    pub fn client_for(&self, generation: u64) -> Option<crate::api::AuthenticatedClient> {
        self.state.client_for(generation)
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
        self.lifecycle = Some(Lifecycle::Authenticated { save });
    }

    fn cancel_expiry(&mut self) {
        if let Some(work) = self.expiry.take() {
            work.abort();
        }
    }
    fn invalidated(&mut self) {
        self.cancel_expiry();
        self.lifecycle = Some(Lifecycle::Invalidated);
    }
    pub fn change_server(&mut self, server: String) {
        if self.server() != server {
            self.state.change_server(server);
            self.cancel_expiry();
            self.lifecycle = Some(Lifecycle::ServerChanged);
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

    // Temporary saved-login lifecycle hooks. No active-context owner exists in the shell.
    pub fn restore_pending(&self) -> bool {
        self.state.restore_pending()
    }
    pub fn begin_restore(&mut self) -> Option<u64> {
        if self.active().is_some() || self.pending() {
            return None;
        }
        Some(self.state.begin_restore())
    }
    pub fn restore_server(&self, server: &str) -> Result<crate::api::ServerClient, AuthError> {
        self.api.server(server)
    }
    pub fn finish_restore(
        &mut self,
        generation: u64,
        selection: &crate::storage::Selection,
        result: RestoreResult,
    ) -> RestoreDecision {
        let decision = self.state.finish_restore(
            generation,
            &selection.server,
            &selection.user,
            selection.expires_at,
            result,
            self.execution.unix_seconds(),
        );
        if decision == RestoreDecision::Restored {
            self.activated(false);
        }
        decision
    }
}

impl Drop for SessionCoordinator {
    fn drop(&mut self) {
        self.cancel_expiry();
    }
}

#[cfg(not(test))]
use state::AppSession;
use state::LoginRequest;
pub(crate) use state::{AuthError, DEFAULT_SERVER_URL, RestoreDecision, RestoreResult, Session};
// Existing pure-transition/adapter fixtures remain available without exposing a
// second mutable session owner to production callers.
#[cfg(test)]
pub(crate) use state::{AppSession, User};

/// Narrow bridge for the still-pure conversation transitions. No authentication,
/// credential or mutable coordinator state is exposed to conversation behavior.
pub(crate) trait SessionAccess {
    fn session_generation(&self) -> Option<u64>;
    fn expire(&mut self, now: i64);
    fn protected_rejected(&mut self, generation: u64);
}

impl SessionAccess for SessionCoordinator {
    fn session_generation(&self) -> Option<u64> {
        self.session_generation()
    }
    fn expire(&mut self, now: i64) {
        self.expire(now);
    }
    fn protected_rejected(&mut self, generation: u64) {
        self.protected_rejected(generation);
    }
}

#[cfg(test)]
impl SessionAccess for AppSession {
    fn session_generation(&self) -> Option<u64> {
        self.session_generation()
    }
    fn expire(&mut self, now: i64) {
        self.expire(now);
    }
    fn protected_rejected(&mut self, generation: u64) {
        self.protected_rejected(generation);
    }
}

impl Session {
    /// Credential material goes directly from the accepted context to the ordered worker.
    pub fn save(
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

impl AppSession {
    /// Only accepted contexts are available to protected activity.
    pub fn active_client(&self) -> Option<crate::api::AuthenticatedClient> {
        self.active.as_ref().map(Session::client)
    }

    /// Capture only the accepted context that originated protected work.
    pub fn client_for(&self, generation: u64) -> Option<crate::api::AuthenticatedClient> {
        (self.session_generation() == Some(generation))
            .then(|| self.active_client())
            .flatten()
    }
}

impl SessionCoordinator {
    /// The candidate remains inside this future until current-user verification completes.
    /// Identity, expiry and generation still gate activation in `finish_restore`.
    pub async fn verify_saved(server: crate::api::ServerClient, token: String) -> RestoreResult {
        let Ok(client) = server.restore_candidate(token) else {
            return RestoreResult::MissingCredential;
        };
        match client.current_user().await {
            Ok(user) => RestoreResult::Verified { user, client },
            Err(AuthError::AlreadyInvalid) => RestoreResult::Rejected,
            _ => RestoreResult::Unavailable,
        }
    }
}

// Pure-state integration fixtures keep their existing verification seam during migration.
#[cfg(test)]
impl AppSession {
    pub async fn verify_saved(server: crate::api::ServerClient, token: String) -> RestoreResult {
        SessionCoordinator::verify_saved(server, token).await
    }
}

#[cfg(test)]
mod binding_tests;
#[cfg(test)]
mod coordinator_tests;
