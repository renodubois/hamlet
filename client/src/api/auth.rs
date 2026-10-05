use super::{ApiError, ApiFuture, AuthenticatedClient, Authentication, ServerClient, User};
use super::{client::Response, error::ErrorResponse};
use hamlet_protocol::{AuthResponse, Credentials};
use reqwest::StatusCode;

pub(super) async fn protected_response<T: serde::de::DeserializeOwned>(
    response: Response,
) -> Result<T, ApiError> {
    match response.status() {
        StatusCode::OK => response.json().await.map_err(|_| ApiError::InvalidResponse),
        StatusCode::UNAUTHORIZED => Err(ApiError::AlreadyInvalid),
        StatusCode::BAD_REQUEST => Err(ApiError::InvalidInput),
        StatusCode::NOT_FOUND => Err(ApiError::NotFound),
        StatusCode::INTERNAL_SERVER_ERROR => Err(ApiError::ServerFailure),
        _ => Err(ApiError::Unavailable),
    }
}

async fn decode_auth(response: Response, success: StatusCode) -> Result<AuthResponse, ApiError> {
    if response.status() == success {
        let wire: AuthResponse = response
            .json()
            .await
            .map_err(|_| ApiError::InvalidResponse)?;
        if wire.access_token.is_empty() || wire.user.id.is_empty() || wire.user.username.is_empty()
        {
            return Err(ApiError::InvalidResponse);
        }
        return Ok(wire);
    }
    let status = response.status();
    if status == StatusCode::UNAUTHORIZED
        || status == StatusCode::BAD_REQUEST
        || status == StatusCode::CONFLICT
    {
        let body: ErrorResponse = response
            .json()
            .await
            .map_err(|_| ApiError::InvalidResponse)?;
        return match (status, body.error.code.as_str()) {
            (StatusCode::UNAUTHORIZED, "unauthorized") => Err(ApiError::InvalidCredentials),
            (StatusCode::BAD_REQUEST, "bad_request") => Err(ApiError::InvalidInput),
            (StatusCode::CONFLICT, "conflict") if success == StatusCode::CREATED => {
                Err(ApiError::Conflict)
            }
            _ => Err(ApiError::InvalidResponse),
        };
    }
    Err(ApiError::Unavailable)
}

impl ServerClient {
    pub fn login(
        &self,
        username: String,
        password: String,
    ) -> ApiFuture<Result<Authentication, ApiError>> {
        self.authenticate("api/v1/auth/login", StatusCode::OK, username, password)
    }

    pub fn signup(
        &self,
        username: String,
        password: String,
    ) -> ApiFuture<Result<Authentication, ApiError>> {
        self.authenticate(
            "api/v1/auth/signup",
            StatusCode::CREATED,
            username,
            password,
        )
    }

    fn authenticate(
        &self,
        path: &str,
        success: StatusCode,
        username: String,
        password: String,
    ) -> ApiFuture<Result<Authentication, ApiError>> {
        let server = self.clone();
        let request = self
            .0
            .transport
            .client
            .post(self.endpoint(path))
            .json(&Credentials { username, password });
        Box::pin(async move {
            let wire = decode_auth(server.0.transport.send(request).await?, success).await?;
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
            let request =
                client.request(reqwest::Method::GET, client.0.server.endpoint("api/v1/me"));
            let wire: User =
                protected_response(client.0.server.0.transport.send(request).await?).await?;
            if wire.id.is_empty() || wire.username.is_empty() {
                return Err(ApiError::InvalidResponse);
            }
            Ok(wire)
        })
    }

    pub fn logout(&self) -> ApiFuture<Result<(), ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            let request = client.request(
                reqwest::Method::POST,
                client.0.server.endpoint("api/v1/auth/logout"),
            );
            match client.0.server.0.transport.send(request).await?.status() {
                StatusCode::NO_CONTENT => Ok(()),
                StatusCode::UNAUTHORIZED => Err(ApiError::AlreadyInvalid),
                _ => Err(ApiError::Unavailable),
            }
        })
    }
}
