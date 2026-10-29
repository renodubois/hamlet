use super::response::{BAD_REQUEST, CONFLICT, Coded, expect, expect_json};
use super::{ApiError, ApiFuture, AuthenticatedClient, Authentication, ServerClient, User};
use hamlet_protocol::{AuthResponse, Credentials};
use reqwest::{Method, StatusCode};

const INVALID_CREDENTIALS: Coded = (
    StatusCode::UNAUTHORIZED,
    "unauthorized",
    ApiError::InvalidCredentials,
);

impl ServerClient {
    pub fn login(
        &self,
        username: String,
        password: String,
    ) -> ApiFuture<Result<Authentication, ApiError>> {
        self.authenticate(
            &["api", "v1", "auth", "login"],
            StatusCode::OK,
            &[INVALID_CREDENTIALS, BAD_REQUEST],
            Credentials { username, password },
        )
    }

    pub fn signup(
        &self,
        username: String,
        password: String,
    ) -> ApiFuture<Result<Authentication, ApiError>> {
        self.authenticate(
            &["api", "v1", "auth", "signup"],
            StatusCode::CREATED,
            &[INVALID_CREDENTIALS, BAD_REQUEST, CONFLICT],
            Credentials { username, password },
        )
    }

    fn authenticate(
        &self,
        path: &[&str],
        success: StatusCode,
        coded: &'static [Coded],
        credentials: Credentials,
    ) -> ApiFuture<Result<Authentication, ApiError>> {
        let server = self.clone();
        let url = self.endpoint(path);
        Box::pin(async move {
            let request = server.transport().client.post(url?).json(&credentials);
            let response = server.transport().send(request).await?;
            let wire: AuthResponse = expect_json(response, success, coded).await?;
            if wire.access_token.is_empty()
                || wire.user.id.is_empty()
                || wire.user.username.is_empty()
            {
                return Err(ApiError::InvalidResponse);
            }
            Ok(Authentication {
                client: server.restore_candidate(wire.access_token)?,
                user: wire.user,
                expires_at: wire.expires_at.timestamp(),
            })
        })
    }
}

impl AuthenticatedClient {
    pub fn current_user(&self) -> ApiFuture<Result<User, ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            let request = client.request(Method::GET, &["api", "v1", "me"])?;
            let wire: User = expect_json(client.send(request).await?, StatusCode::OK, &[]).await?;
            if wire.id.is_empty() || wire.username.is_empty() {
                return Err(ApiError::InvalidResponse);
            }
            Ok(wire)
        })
    }

    pub fn logout(&self) -> ApiFuture<Result<(), ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            let request = client.request(Method::POST, &["api", "v1", "auth", "logout"])?;
            expect(client.send(request).await?, StatusCode::NO_CONTENT, &[]).await?;
            Ok(())
        })
    }
}
