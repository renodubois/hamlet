use crate::api::{ApiError, User};
use crate::api::{AuthenticatedClient, Authentication};

pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8081";

pub struct LoginRequest {
    pub generation: u64,
    pub server: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RestoreDecision {
    Restored,
    Delete,
    Retry,
    Stale,
}

pub enum RestoreResult {
    Verified {
        user: User,
        client: AuthenticatedClient,
    },
    Rejected,
    MissingCredential,
    Unavailable,
}

pub struct Revocation {
    pub generation: u64,
    pub client: AuthenticatedClient,
}

pub struct Session {
    pub server: String,
    pub user: User,
    client: AuthenticatedClient,
    pub expires_at: i64,
}

impl Session {
    pub(super) fn client(&self) -> AuthenticatedClient {
        self.client.clone()
    }
}

pub struct AppSession {
    pub server: String,
    pub pending: bool,
    restore_pending: bool,
    pub feedback: Option<String>,
    pub active: Option<Session>,
    generation: u64,
}

impl AppSession {
    fn active_client(&self) -> Option<AuthenticatedClient> {
        self.active.as_ref().map(Session::client)
    }

    pub fn new() -> Self {
        Self {
            server: DEFAULT_SERVER_URL.into(),
            pending: false,
            restore_pending: false,
            feedback: None,
            active: None,
            generation: 0,
        }
    }

    pub fn change_server(&mut self, server: String) {
        if self.server != server {
            self.generation = self.generation.wrapping_add(1);
            self.pending = false;
            self.restore_pending = false;
            self.active = None;
            self.server = server;
            self.feedback = None;
        }
    }

    pub fn submit(&mut self, username: &str, password: &str) -> Option<LoginRequest> {
        if !self.validate_submission() {
            return None;
        }
        if username.trim().is_empty() || password.is_empty() {
            self.feedback = Some("Enter a username and password.".into());
            return None;
        }
        Some(self.begin_submission())
    }

    pub fn submit_signup(&mut self, username: &str, password: &str) -> Option<LoginRequest> {
        if !self.validate_submission() {
            return None;
        }
        // Mirror the rewrite server's byte-based limits; the server remains authoritative.
        if !(3..=32).contains(&username.len())
            || !username
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
        {
            self.feedback =
                Some("Username must be 3–32 ASCII letters, digits, underscores or dots.".into());
            return None;
        }
        if !(8..=256).contains(&password.len()) {
            self.feedback = Some("Password must be 8–256 bytes long.".into());
            return None;
        }
        Some(self.begin_submission())
    }

    fn validate_submission(&mut self) -> bool {
        if self.pending || self.active.is_some() {
            return false;
        }
        if crate::api::validate_server(&self.server).is_err() {
            self.feedback =
                Some("Use an HTTPS server URL (HTTP is allowed only for loopback).".into());
            return false;
        }
        true
    }

    fn begin_submission(&mut self) -> LoginRequest {
        self.generation = self.generation.wrapping_add(1);
        self.pending = true;
        self.restore_pending = false;
        self.feedback = None;
        LoginRequest {
            generation: self.generation,
            server: self.server.clone(),
        }
    }

    pub fn begin_restore(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.pending = true;
        self.restore_pending = true;
        self.feedback = Some("Checking saved session…".into());
        self.generation
    }

    pub fn restore_pending(&self) -> bool {
        self.pending && self.restore_pending
    }

    pub fn restoring(&self, generation: u64, server: &str) -> bool {
        self.restore_pending() && self.generation == generation && self.server == server
    }

    pub fn finish_restore(
        &mut self,
        generation: u64,
        server: &str,
        expected: &User,
        expires_at: i64,
        result: RestoreResult,
        now: i64,
    ) -> RestoreDecision {
        if !self.restoring(generation, server) {
            return RestoreDecision::Stale;
        }
        self.pending = false;
        self.restore_pending = false;
        if expires_at <= now {
            self.feedback = Some("Saved session expired. Please log in again.".into());
            return RestoreDecision::Delete;
        }
        match result {
            RestoreResult::Verified { user, client }
                if user == *expected
                    && crate::api::validate_server(server)
                        .is_ok_and(|url| &url == client.server_url()) =>
            {
                self.active = Some(Session {
                    server: server.into(),
                    user,
                    client,
                    expires_at,
                });
                self.feedback = None;
                RestoreDecision::Restored
            }
            RestoreResult::Rejected | RestoreResult::MissingCredential => {
                self.feedback =
                    Some("Saved session rejected or missing. Please log in again.".into());
                RestoreDecision::Delete
            }
            RestoreResult::Verified { .. } => {
                self.feedback =
                    Some("Saved session identity did not match. Please log in again.".into());
                RestoreDecision::Delete
            }
            RestoreResult::Unavailable => {
                self.feedback = Some("Could not verify saved session; it has not been removed. Check connectivity and retry restoration or log in.".into());
                RestoreDecision::Retry
            }
        }
    }

    pub fn cancel_pending(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = false;
        self.restore_pending = false;
        self.feedback = None;
    }

    pub fn complete_signup(
        &mut self,
        request: LoginRequest,
        result: Result<Authentication, ApiError>,
        now: i64,
    ) -> bool {
        self.complete_auth(request, result, now, true)
    }

    pub fn complete_login(
        &mut self,
        request: LoginRequest,
        result: Result<Authentication, ApiError>,
        now: i64,
    ) -> bool {
        self.complete_auth(request, result, now, false)
    }

    fn complete_auth(
        &mut self,
        request: LoginRequest,
        result: Result<Authentication, ApiError>,
        now: i64,
        signup: bool,
    ) -> bool {
        if request.generation != self.generation || request.server != self.server || !self.pending {
            return false;
        }
        self.pending = false;
        self.restore_pending = false;
        match result {
            Ok(login) if login.expires_at > now
                && crate::api::validate_server(&request.server).is_ok_and(|url| &url == login.client.server_url()) => {
                self.active = Some(Session {
                    server: request.server,
                    user: login.user,
                    client: login.client,
                    expires_at: login.expires_at,
                });
                self.feedback = None;
            }
            Ok(_) | Err(ApiError::InvalidResponse) => {
                self.feedback = Some(if signup {
                    "Could not confirm signup from the server response; it may have succeeded. Check before resubmitting.".into()
                } else {
                    "The server returned an invalid login response.".into()
                })
            }
            Err(ApiError::Conflict) => {
                self.feedback = Some("Username already exists. Choose another username or log in.".into())
            }
            Err(ApiError::InvalidCredentials) => {
                self.feedback = Some("Incorrect username or password.".into())
            }
            Err(ApiError::InvalidInput) => {
                self.feedback = Some(format!("The server rejected the {} input. Check your username and password.", if signup { "signup" } else { "login" }))
            }
            Err(ApiError::Unavailable) => {
                self.feedback = Some(if signup {
                    "Could not confirm signup; it may have succeeded. Check the server before submitting again."
                } else {
                    "Could not reach the server. Check the address and try again."
                }.into())
            }
            Err(ApiError::AlreadyInvalid) => {
                self.feedback =
                    Some("The server rejected this session. Please log in again.".into())
            }
            Err(ApiError::NotFound | ApiError::ServerFailure) => {
                self.feedback = Some(if signup { "The server could not confirm signup; it may have succeeded. Check before retrying." } else { "The server could not complete login." }.into())
            }
        }
        true
    }

    pub fn logout(&mut self) -> Option<Revocation> {
        self.generation = self.generation.wrapping_add(1);
        self.pending = false;
        self.restore_pending = false;
        self.feedback = None;
        let client = self.active_client();
        self.active.take();
        client.map(|client| Revocation {
            generation: self.generation,
            client,
        })
    }

    pub fn revocation_result(&mut self, generation: u64, result: Result<(), ApiError>) {
        if generation == self.generation && self.active.is_none() {
            self.feedback = match result {
                Ok(()) => None,
                Err(ApiError::AlreadyInvalid) => Some("Logged out locally; the server reported the token was already invalid (401). No new revocation was confirmed.".into()),
                Err(_) => Some("Logged out locally; server revocation could not be confirmed. Your session may remain valid until it expires.".into()),
            };
        }
    }

    pub fn expire(&mut self, now: i64) {
        if self
            .active
            .as_ref()
            .is_some_and(|session| session.expires_at <= now)
        {
            self.invalidate();
        }
    }

    // Later protected features must capture this generation before starting work, and pass it
    // to this entry point on an authoritative 401; transport failures must not call it.
    pub fn session_generation(&self) -> Option<u64> {
        self.active.as_ref().map(|_| self.generation)
    }

    pub fn protected_rejected(&mut self, generation: u64) {
        if self.session_generation() == Some(generation) {
            self.invalidate();
        }
    }

    fn invalidate(&mut self) {
        self.logout();
        self.feedback = Some("Your session expired or was rejected. Please log in again.".into());
    }
}

#[cfg(test)]
#[path = "tests/binding.rs"]
mod binding_tests;

#[cfg(test)]
#[path = "tests/state.rs"]
mod tests;
