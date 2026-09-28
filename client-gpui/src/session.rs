use std::sync::Arc;

pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8081";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub username: String,
}

#[derive(Clone, Debug)]
pub struct Login {
    pub user: User,
    pub token: String,
    pub expires_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthError {
    InvalidCredentials,
    InvalidInput,
    Unavailable,
    InvalidResponse,
    AlreadyInvalid,
}

pub type ApiFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

// This is the view's behavior boundary. Only the HTTP adapter knows bearer headers or JSON.
pub trait AuthApi: Send + Sync {
    fn login(
        &self,
        server: String,
        username: String,
        password: String,
    ) -> ApiFuture<Result<Login, AuthError>>;
    fn logout(&self, server: String, token: String) -> ApiFuture<Result<(), AuthError>>;
}

pub struct LoginRequest {
    pub generation: u64,
    pub server: String,
    pub username: String,
    pub password: String,
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

pub struct AppSession {
    pub server: String,
    pub username: String,
    pub password: String,
    pub pending: bool,
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
            self.active = None;
            self.password.clear();
            self.server = server;
            self.feedback = None;
        }
    }

    pub fn submit(&mut self) -> Option<LoginRequest> {
        if self.pending || self.active.is_some() {
            return None;
        }
        if crate::http::validate_server(&self.server).is_err() {
            self.feedback =
                Some("Use an HTTPS server URL (HTTP is allowed only for loopback).".into());
            return None;
        }
        if self.username.trim().is_empty() || self.password.is_empty() {
            self.feedback = Some("Enter a username and password.".into());
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.pending = true;
        self.feedback = None;
        Some(LoginRequest {
            generation: self.generation,
            server: self.server.clone(),
            username: self.username.clone(),
            password: self.password.clone(),
        })
    }

    pub fn complete_login(
        &mut self,
        request: LoginRequest,
        result: Result<Login, AuthError>,
        now: i64,
    ) -> bool {
        if request.generation != self.generation || request.server != self.server || !self.pending {
            return false;
        }
        self.pending = false;
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
                self.feedback = Some("The server returned an invalid login response.".into())
            }
            Err(AuthError::InvalidCredentials) => {
                self.feedback = Some("Incorrect username or password.".into())
            }
            Err(AuthError::InvalidInput) => {
                self.feedback = Some(
                    "The server rejected the login input. Check your username and password.".into(),
                )
            }
            Err(AuthError::Unavailable) => {
                self.feedback =
                    Some("Could not reach the server. Check the address and try again.".into())
            }
            Err(AuthError::AlreadyInvalid) => {
                self.feedback =
                    Some("The server rejected this session. Please log in again.".into())
            }
        }
        true
    }

    pub fn logout(&mut self) -> Option<Revocation> {
        self.generation = self.generation.wrapping_add(1);
        self.pending = false;
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

    #[allow(dead_code)] // Entry point for future protected work; no protected resource exists in this slice.
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
        fn login(&self, _: String, _: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
            Box::pin(async { Err(AuthError::Unavailable) })
        }
        fn logout(&self, _: String, _: String) -> ApiFuture<Result<(), AuthError>> {
            Box::pin(async { Ok(()) })
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
