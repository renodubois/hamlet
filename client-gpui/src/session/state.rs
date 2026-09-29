#[cfg(test)]
pub use crate::api::ApiFuture;
#[cfg(test)]
use crate::api::{Channel, Message};
// Temporary import compatibility; API is the authoritative owner.
pub use crate::api::{
    ApiError as AuthError, User,
    legacy::{AuthApi, Login},
};
use std::sync::Arc;

pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8081";

pub struct LoginRequest {
    pub generation: u64,
    pub server: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RestoreDecision {
    Restored,
    Delete,
    Retry,
    Stale,
}

pub enum RestoreResult {
    Verified { user: User, token: String },
    Rejected,
    MissingCredential,
    Unavailable,
}

pub struct Revocation {
    pub generation: u64,
    pub server: String,
    pub token: String,
}

pub struct Session {
    pub server: String,
    pub user: User,
    token: String,
    pub expires_at: i64,
}

impl Session {
    pub fn token(&self) -> &str {
        &self.token
    }
}

pub struct AppSession {
    pub server: String,
    pub username: String,
    pub password: String,
    pub pending: bool,
    restore_pending: bool,
    pub feedback: Option<String>,
    pub active: Option<Session>,
    generation: u64,
    pub api: Arc<dyn AuthApi>,
}

impl AppSession {
    pub fn new(api: Arc<dyn AuthApi>) -> Self {
        Self {
            server: DEFAULT_SERVER_URL.into(),
            username: String::new(),
            password: String::new(),
            pending: false,
            restore_pending: false,
            feedback: None,
            active: None,
            generation: 0,
            api,
        }
    }

    pub fn change_server(&mut self, server: String) {
        if self.server != server {
            self.generation = self.generation.wrapping_add(1);
            self.pending = false;
            self.restore_pending = false;
            self.active = None;
            self.password.clear();
            self.server = server;
            self.feedback = None;
        }
    }

    pub fn submit(&mut self) -> Option<LoginRequest> {
        if !self.validate_submission() {
            return None;
        }
        if self.username.trim().is_empty() || self.password.is_empty() {
            self.feedback = Some("Enter a username and password.".into());
            return None;
        }
        Some(self.begin_submission())
    }

    pub fn submit_signup(&mut self) -> Option<LoginRequest> {
        if !self.validate_submission() {
            return None;
        }
        // Mirror the rewrite server's byte-based limits; the server remains authoritative.
        if !(3..=32).contains(&self.username.len())
            || !self
                .username
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
        {
            self.feedback =
                Some("Username must be 3–32 ASCII letters, digits, underscores or dots.".into());
            return None;
        }
        if !(8..=256).contains(&self.password.len()) {
            self.feedback = Some("Password must be 8–256 bytes long.".into());
            return None;
        }
        Some(self.begin_submission())
    }

    fn validate_submission(&mut self) -> bool {
        if self.pending || self.active.is_some() {
            return false;
        }
        if crate::http::validate_server(&self.server).is_err() {
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
            username: self.username.clone(),
            password: self.password.clone(),
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
            RestoreResult::Verified { user, token } if user == *expected && !token.is_empty() => {
                self.username = user.username.clone();
                self.active = Some(Session {
                    server: server.into(),
                    user,
                    token,
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
        result: Result<Login, AuthError>,
        now: i64,
    ) -> bool {
        self.complete_auth(request, result, now, true)
    }

    pub fn complete_login(
        &mut self,
        request: LoginRequest,
        result: Result<Login, AuthError>,
        now: i64,
    ) -> bool {
        self.complete_auth(request, result, now, false)
    }

    fn complete_auth(
        &mut self,
        request: LoginRequest,
        result: Result<Login, AuthError>,
        now: i64,
        signup: bool,
    ) -> bool {
        if request.generation != self.generation || request.server != self.server || !self.pending {
            return false;
        }
        self.pending = false;
        self.restore_pending = false;
        match result {
            Ok(login) if login.expires_at > now && !login.token.is_empty() => {
                self.password.clear();
                self.active = Some(Session {
                    server: request.server,
                    user: login.user,
                    token: login.token,
                    expires_at: login.expires_at,
                });
                self.feedback = None;
            }
            Ok(_) | Err(AuthError::InvalidResponse) => {
                self.feedback = Some(if signup {
                    "Could not confirm signup from the server response; it may have succeeded. Check before resubmitting.".into()
                } else {
                    "The server returned an invalid login response.".into()
                })
            }
            Err(AuthError::Conflict) => {
                self.feedback = Some("Username already exists. Choose another username or log in.".into())
            }
            Err(AuthError::InvalidCredentials) => {
                self.feedback = Some("Incorrect username or password.".into())
            }
            Err(AuthError::InvalidInput) => {
                self.feedback = Some(format!("The server rejected the {} input. Check your username and password.", if signup { "signup" } else { "login" }))
            }
            Err(AuthError::Unavailable) => {
                self.feedback = Some(if signup {
                    "Could not confirm signup; it may have succeeded. Check the server before submitting again."
                } else {
                    "Could not reach the server. Check the address and try again."
                }.into())
            }
            Err(AuthError::AlreadyInvalid) => {
                self.feedback =
                    Some("The server rejected this session. Please log in again.".into())
            }
            Err(AuthError::NotFound | AuthError::ServerFailure) => {
                self.feedback = Some(if signup { "The server could not confirm signup; it may have succeeded. Check before retrying." } else { "The server could not complete login." }.into())
            }
        }
        true
    }

    pub fn logout(&mut self) -> Option<Revocation> {
        self.generation = self.generation.wrapping_add(1);
        self.pending = false;
        self.restore_pending = false;
        self.password.clear();
        self.feedback = None;
        self.active.take().map(|session| Revocation {
            generation: self.generation,
            server: session.server,
            token: session.token,
        })
    }

    pub fn revocation_result(&mut self, generation: u64, result: Result<(), AuthError>) {
        if generation == self.generation && self.active.is_none() {
            self.feedback = match result {
                Ok(()) => None,
                Err(AuthError::AlreadyInvalid) => Some("Logged out locally; the server reported the token was already invalid (401). No new revocation was confirmed.".into()),
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
mod tests {
    use super::*;
    struct Controlled;
    impl AuthApi for Controlled {
        fn signup(&self, _: String, _: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
            Box::pin(async { Err(AuthError::Unavailable) })
        }
        fn login(&self, _: String, _: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
            Box::pin(async { Err(AuthError::Unavailable) })
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
    fn app() -> AppSession {
        AppSession::new(Arc::new(Controlled))
    }
    fn login(name: &str) -> Login {
        Login {
            user: User {
                id: "42".into(),
                username: name.into(),
            },
            token: "secret".into(),
            expires_at: 100,
        }
    }
    #[test]
    fn login_feedback_and_recoverable_inputs() {
        let mut app = app();
        assert!(app.submit().is_none());
        app.username = "alice".into();
        app.password = "wrong".into();
        let request = app.submit().unwrap();
        assert!(app.submit().is_none());
        app.complete_login(request, Err(AuthError::InvalidCredentials), 0);
        assert_eq!(
            app.feedback.as_deref(),
            Some("Incorrect username or password.")
        );
        assert_eq!(app.password, "wrong");
        let request = app.submit().unwrap();
        app.complete_login(request, Ok(login("alice")), 0);
        assert_eq!(app.active.as_ref().unwrap().user.username, "alice");
        assert!(app.password.is_empty());
    }
    #[test]
    fn signup_enters_session_and_preserves_inputs_on_rejection() {
        let mut app = app();
        app.username = "bad!".into();
        app.password = "short".into();
        assert!(app.submit_signup().is_none());
        assert!(app.feedback.as_deref().unwrap().contains("3–32"));
        app.username = "Alice_1".into();
        app.password = "long password".into();
        let request = app.submit_signup().unwrap();
        assert!(app.submit_signup().is_none());
        assert!(app.complete_signup(request, Err(AuthError::Conflict), 0));
        assert!(app.feedback.as_deref().unwrap().contains("already exists"));
        assert_eq!(app.password, "long password");
        let request = app.submit_signup().unwrap();
        assert!(app.complete_signup(request, Err(AuthError::Unavailable), 0));
        assert!(
            app.feedback
                .as_deref()
                .unwrap()
                .contains("may have succeeded")
        );
        let request = app.submit_signup().unwrap();
        assert!(app.complete_signup(request, Ok(login("Alice_1")), 0));
        assert_eq!(app.active.as_ref().unwrap().user.username, "Alice_1");
        assert!(app.password.is_empty());
    }
    #[test]
    fn stale_signup_cannot_replace_new_session_or_feedback() {
        let mut app = app();
        app.username = "Alice".into();
        app.password = "password".into();
        let old = app.submit_signup().unwrap();
        app.cancel_pending();
        let newer = app.submit().unwrap();
        app.complete_login(newer, Ok(login("new")), 0);
        assert!(!app.complete_signup(old, Err(AuthError::Conflict), 0));
        assert_eq!(app.active.as_ref().unwrap().user.username, "new");
        assert!(app.feedback.is_none());
    }
    #[test]
    fn stale_outcomes_cannot_replace_or_invalidate_newer_session() {
        let mut app = app();
        app.username = "a".into();
        app.password = "p".into();
        let old = app.submit().unwrap();
        app.logout();
        app.password = "p".into();
        let newer = app.submit().unwrap();
        app.complete_login(newer, Ok(login("new")), 0);
        let current = app.session_generation().unwrap();
        app.complete_login(old, Err(AuthError::InvalidCredentials), 0);
        app.protected_rejected(current.wrapping_sub(1));
        assert_eq!(app.active.as_ref().unwrap().user.username, "new");
        app.protected_rejected(current);
        assert!(app.active.is_none());
        assert!(app.feedback.as_deref().unwrap().contains("session expired"));
    }
    #[test]
    fn logout_clears_immediately_and_only_current_revocation_warns() {
        let mut app = app();
        app.username = "a".into();
        app.password = "p".into();
        let request = app.submit().unwrap();
        app.complete_login(request, Ok(login("a")), 0);
        let revocation = app.logout().unwrap();
        assert!(app.active.is_none());
        assert!(app.password.is_empty());
        app.revocation_result(revocation.generation, Err(AuthError::Unavailable));
        assert!(
            app.feedback
                .as_deref()
                .unwrap()
                .contains("could not be confirmed")
        );
        app.revocation_result(revocation.generation, Err(AuthError::AlreadyInvalid));
        assert!(
            app.feedback
                .as_deref()
                .unwrap()
                .contains("already invalid (401)")
        );
        app.password = "p".into();
        let request = app.submit().unwrap();
        app.revocation_result(revocation.generation, Err(AuthError::Unavailable));
        assert!(
            app.feedback.is_none(),
            "old revocation cannot alter a newer login"
        );
        app.complete_login(request, Ok(login("b")), 0);
        app.revocation_result(revocation.generation, Err(AuthError::Unavailable));
        assert!(app.feedback.is_none());
        app.expire(100);
        assert!(app.active.is_none());
    }
    #[test]
    fn saved_session_requires_verification_and_distinguishes_temporary_failure_and_expiry() {
        let mut app = app();
        let server = app.server.clone();
        let first = app.begin_restore();
        assert!(app.pending);
        assert!(app.active.is_none());
        assert_eq!(
            app.finish_restore(
                first,
                &server,
                &login("Ada").user,
                100,
                RestoreResult::Unavailable,
                0
            ),
            RestoreDecision::Retry
        );
        assert!(
            app.feedback
                .as_deref()
                .unwrap()
                .contains("not been removed")
        );
        let second = app.begin_restore();
        assert_eq!(
            app.finish_restore(
                second,
                &server,
                &login("Ada").user,
                100,
                RestoreResult::Verified {
                    user: login("Ada").user,
                    token: "secret".into()
                },
                0
            ),
            RestoreDecision::Restored
        );
        assert_eq!(app.active.as_ref().unwrap().user.username, "Ada");
        app.expire(100);
        assert!(app.active.is_none());
        let third = app.begin_restore();
        assert_eq!(
            app.finish_restore(
                third,
                &server,
                &login("Ada").user,
                100,
                RestoreResult::Unavailable,
                100
            ),
            RestoreDecision::Delete
        );
        assert!(app.feedback.as_deref().unwrap().contains("expired"));
        let fourth = app.begin_restore();
        assert_eq!(
            app.finish_restore(
                fourth,
                &server,
                &login("Ada").user,
                100,
                RestoreResult::Rejected,
                0
            ),
            RestoreDecision::Delete
        );
        assert!(app.feedback.as_deref().unwrap().contains("rejected"));
        let fifth = app.begin_restore();
        assert_eq!(
            app.finish_restore(
                fifth,
                &server,
                &login("Other").user,
                100,
                RestoreResult::Verified {
                    user: login("Ada").user,
                    token: "secret".into()
                },
                0
            ),
            RestoreDecision::Delete
        );
        assert!(app.feedback.as_deref().unwrap().contains("identity"));
    }
    #[test]
    fn late_restoration_after_logout_or_server_change_cannot_reopen_session() {
        let mut app = app();
        let server = app.server.clone();
        let pending = app.begin_restore();
        app.logout();
        assert_eq!(
            app.finish_restore(
                pending,
                &server,
                &login("Ada").user,
                100,
                RestoreResult::Unavailable,
                0
            ),
            RestoreDecision::Stale
        );
        let pending = app.begin_restore();
        app.change_server("https://elsewhere.example".into());
        assert_eq!(
            app.finish_restore(
                pending,
                &server,
                &login("Ada").user,
                100,
                RestoreResult::Unavailable,
                0
            ),
            RestoreDecision::Stale
        );
        assert!(app.active.is_none());
    }
    #[test]
    fn changing_servers_invalidates_pending_and_active() {
        let mut app = app();
        app.username = "a".into();
        app.password = "p".into();
        let request = app.submit().unwrap();
        app.change_server("https://example.org".into());
        app.complete_login(request, Ok(login("a")), 0);
        assert!(app.active.is_none());
        assert!(!app.pending);
    }
}
