//! Temporary forwarding facade; retire after BOTH authentication and protected callers migrate.
//! No endpoints, HTTP dispatch or decoding belong here. Existing fixture implementations remain
//! until their owning coordinator migrates; new tests use bound clients and request adapters.
#[cfg(test)]
use super::User;
use super::{ApiError as AuthError, ApiFuture, Channel, HttpTransport, Message, Page};

#[cfg(test)]
#[derive(Clone)]
pub struct Login {
    pub user: User,
    pub token: String,
    pub expires_at: i64,
}

#[cfg(test)]
impl Login {
    // Mechanical pure-state fixture conversion; authentication workflow tests use bound calls.
    pub(crate) fn bind(self, server: &str) -> super::Authentication {
        super::Authentication {
            client: HttpTransport::new()
                .server(server)
                .unwrap()
                .restore_candidate(self.token)
                .unwrap(),
            user: self.user,
            expires_at: self.expires_at,
        }
    }
}

#[cfg(test)]
impl std::fmt::Debug for Login {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Login")
            .field("user", &self.user)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
fn legacy_login(auth: super::Authentication) -> Login {
    // Only mixed legacy protected-operation tests still consume this representation.
    // Production authentication/session/storage use the accepted bound context.
    Login {
        user: auth.user,
        token: auth.client.0.token.clone(),
        expires_at: auth.expires_at,
    }
}

#[derive(Clone)]
pub struct HttpAuth {
    transport: HttpTransport,
}

impl HttpAuth {
    pub fn new() -> Self {
        Self::with_transport(HttpTransport::new())
    }

    pub fn with_transport(transport: HttpTransport) -> Self {
        Self { transport }
    }

    #[cfg(test)]
    pub(super) fn with_client(client: reqwest::Client) -> Self {
        Self {
            transport: HttpTransport::from_client(client),
        }
    }
}

impl AuthApi for HttpAuth {
    fn server(self: std::sync::Arc<Self>, origin: &str) -> Result<super::ServerClient, AuthError> {
        self.transport.server(origin)
    }

    #[cfg(test)]
    fn login(
        &self,
        server: String,
        username: String,
        password: String,
    ) -> ApiFuture<Result<Login, AuthError>> {
        let transport = self.transport.clone();
        Box::pin(async move {
            transport
                .server(&server)?
                .login(username, password)
                .await
                .map(legacy_login)
        })
    }
    #[cfg(test)]
    fn signup(
        &self,
        server: String,
        username: String,
        password: String,
    ) -> ApiFuture<Result<Login, AuthError>> {
        let transport = self.transport.clone();
        Box::pin(async move {
            transport
                .server(&server)?
                .signup(username, password)
                .await
                .map(legacy_login)
        })
    }
    #[cfg(test)]
    fn current_user(&self, server: String, token: String) -> ApiFuture<Result<User, AuthError>> {
        let transport = self.transport.clone();
        Box::pin(async move {
            transport
                .server(&server)?
                .restore_candidate(token)?
                .current_user()
                .await
        })
    }
    #[cfg(test)]
    fn logout(&self, server: String, token: String) -> ApiFuture<Result<(), AuthError>> {
        let transport = self.transport.clone();
        Box::pin(async move {
            transport
                .server(&server)?
                .restore_candidate(token)?
                .logout()
                .await
        })
    }
    fn channels(
        &self,
        server: String,
        token: String,
    ) -> ApiFuture<Result<Vec<Channel>, AuthError>> {
        let transport = self.transport.clone();
        Box::pin(async move {
            transport
                .server(&server)?
                .restore_candidate(token)?
                .channels()
                .await
        })
    }
    fn create_channel(
        &self,
        server: String,
        token: String,
        name: String,
    ) -> ApiFuture<Result<Channel, AuthError>> {
        let transport = self.transport.clone();
        Box::pin(async move {
            transport
                .server(&server)?
                .restore_candidate(token)?
                .create_channel(name)
                .await
        })
    }
    fn send_message(
        &self,
        server: String,
        token: String,
        channel_id: String,
        text: String,
    ) -> ApiFuture<Result<Message, AuthError>> {
        let transport = self.transport.clone();
        Box::pin(async move {
            transport
                .server(&server)?
                .restore_candidate(token)?
                .send_message(channel_id, text)
                .await
        })
    }
    fn history(
        &self,
        server: String,
        token: String,
        channel_id: String,
    ) -> ApiFuture<Result<Vec<Message>, AuthError>> {
        let transport = self.transport.clone();
        Box::pin(async move {
            transport
                .server(&server)?
                .restore_candidate(token)?
                .history(channel_id)
                .await
        })
    }
    fn history_page(
        &self,
        server: String,
        token: String,
        channel_id: String,
        before: Option<String>,
    ) -> ApiFuture<Result<Page, AuthError>> {
        let transport = self.transport.clone();
        Box::pin(async move {
            transport
                .server(&server)?
                .restore_candidate(token)?
                .history_page(channel_id, before)
                .await
        })
    }
}

// Temporary caller compatibility only; production forwards to bound clients above.
pub trait AuthApi: Send + Sync + 'static {
    #[cfg(not(test))]
    fn server(self: std::sync::Arc<Self>, origin: &str) -> Result<super::ServerClient, AuthError>;

    // Temporary adapter for protected-operation fixtures pending #52. Bound authentication
    // still exercises request construction/decoding and the production session workflow.
    #[cfg(test)]
    fn server(self: std::sync::Arc<Self>, origin: &str) -> Result<super::ServerClient, AuthError> {
        HttpTransport::with_adapter(std::sync::Arc::new(
            super::legacy_fixture::AuthenticationAdapter(self),
        ))
        .server(origin)
    }

    #[cfg(test)]
    fn login(
        &self,
        server: String,
        username: String,
        password: String,
    ) -> ApiFuture<Result<Login, AuthError>>;
    #[cfg(test)]
    fn signup(
        &self,
        server: String,
        username: String,
        password: String,
    ) -> ApiFuture<Result<Login, AuthError>>;
    #[cfg(test)]
    fn logout(&self, server: String, token: String) -> ApiFuture<Result<(), AuthError>>;
    #[cfg(test)]
    fn current_user(&self, _server: String, _token: String) -> ApiFuture<Result<User, AuthError>> {
        Box::pin(async { Err(AuthError::Unavailable) })
    }
    fn channels(&self, server: String, token: String)
    -> ApiFuture<Result<Vec<Channel>, AuthError>>;
    fn create_channel(
        &self,
        server: String,
        token: String,
        name: String,
    ) -> ApiFuture<Result<Channel, AuthError>>;
    fn send_message(
        &self,
        _server: String,
        _token: String,
        _channel_id: String,
        _text: String,
    ) -> ApiFuture<Result<Message, AuthError>> {
        Box::pin(async { Err(AuthError::Unavailable) })
    }
    fn history(
        &self,
        server: String,
        token: String,
        channel_id: String,
    ) -> ApiFuture<Result<Vec<Message>, AuthError>>;
    fn history_page(
        &self,
        server: String,
        token: String,
        channel_id: String,
        before: Option<String>,
    ) -> ApiFuture<Result<Page, AuthError>> {
        let _ = before;
        let history = self.history(server, token, channel_id);
        Box::pin(async move {
            history.await.map(|items| Page {
                items,
                next_cursor: None,
            })
        })
    }
}
