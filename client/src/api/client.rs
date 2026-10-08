//! Validates server origins and binds HTTP transports to immutable server/credential contexts.

use super::{ApiError, ApiFuture, User};
use reqwest::{Client, Request, RequestBuilder, StatusCode, Url};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

pub fn validate_server(value: &str) -> Result<Url, ApiError> {
    let url = Url::parse(value).map_err(|_| ApiError::InvalidResponse)?;
    let host = url.host_str().ok_or(ApiError::InvalidResponse)?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    if (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(ApiError::InvalidResponse);
    }
    Ok(url)
}

/// Reusable connection pool and transport policy, shared across bound contexts.
#[derive(Clone)]
pub struct HttpTransport {
    pub(super) client: Client,
    adapter: Arc<dyn RequestAdapter>,
    pub(super) stream_client: Client,
    pub(super) stream_adapter: Arc<dyn super::events::StreamAdapter>,
}

impl HttpTransport {
    pub fn new() -> Self {
        Self::from_client(
            Self::client_builder()
                .timeout(Duration::from_secs(8))
                .build()
                .expect("HTTP client"),
        )
    }

    fn client_builder() -> reqwest::ClientBuilder {
        Client::builder()
            .no_proxy()
            .resolve_to_addrs(
                "localhost",
                &[
                    SocketAddr::from(([127, 0, 0, 1], 0)),
                    SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 0)),
                ],
            )
            .redirect(reqwest::redirect::Policy::none())
    }

    pub(super) fn from_client(client: Client) -> Self {
        // A distinct pool: clearing Request::timeout_mut() falls back to the
        // client's total timeout. Streaming needs a client with no total deadline.
        let stream_client = Self::client_builder().build().expect("stream HTTP client");
        Self {
            adapter: Arc::new(client.clone()),
            client,
            stream_adapter: Arc::new(stream_client.clone()),
            stream_client,
        }
    }

    /// Controlled requests still pass through the same URL, header and body construction.
    #[cfg(test)]
    pub(crate) fn with_adapter(adapter: Arc<dyn RequestAdapter>) -> Self {
        Self {
            adapter,
            ..Self::new()
        }
    }

    #[cfg(test)]
    pub(super) fn with_stream_adapter(adapter: Arc<dyn super::events::StreamAdapter>) -> Self {
        Self {
            stream_adapter: adapter,
            ..Self::new()
        }
    }

    #[cfg(test)]
    pub(crate) fn with_adapters(
        adapter: Arc<dyn RequestAdapter>,
        stream_adapter: Arc<dyn super::events::StreamAdapter>,
    ) -> Self {
        Self {
            adapter,
            stream_adapter,
            ..Self::new()
        }
    }

    pub fn server(&self, origin: &str) -> Result<ServerClient, ApiError> {
        Ok(ServerClient(Arc::new(ServerContext {
            url: validate_server(origin)?,
            transport: self.clone(),
        })))
    }

    pub(super) fn send(&self, request: RequestBuilder) -> ApiFuture<Result<Response, ApiError>> {
        let adapter = self.adapter.clone();
        Box::pin(async move {
            let request = request.build().map_err(|_| ApiError::Unavailable)?;
            adapter.execute(request).await
        })
    }
}

/// Private transport seam, after binding; never an alternate endpoint implementation.
pub(crate) trait RequestAdapter: Send + Sync {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>>;
}

impl RequestAdapter for Client {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            Client::execute(&client, request)
                .await
                .map(Response::Http)
                .map_err(|_| ApiError::Unavailable)
        })
    }
}

// Body decoding stays lazy: logout/status-only errors must not wait for a response body.
// Real response streams retain reqwest's total eight-second request/body deadline.
pub(crate) enum Response {
    Http(reqwest::Response),
    #[cfg(test)]
    Controlled {
        status: StatusCode,
        body: String,
    },
}

impl Response {
    pub(super) fn status(&self) -> StatusCode {
        match self {
            Self::Http(response) => response.status(),
            #[cfg(test)]
            Self::Controlled { status, .. } => *status,
        }
    }

    pub(super) async fn json<T: serde::de::DeserializeOwned>(self) -> Result<T, ApiError> {
        match self {
            Self::Http(response) => response.json().await.map_err(|_| ApiError::InvalidResponse),
            #[cfg(test)]
            Self::Controlled { body, .. } => {
                serde_json::from_str(&body).map_err(|_| ApiError::InvalidResponse)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn controlled(status: StatusCode, body: impl Into<String>) -> Self {
        Self::Controlled {
            status,
            body: body.into(),
        }
    }
}

#[derive(Clone)]
pub struct ServerClient(pub(super) Arc<ServerContext>);

pub(super) struct ServerContext {
    pub url: Url,
    pub transport: HttpTransport,
}

/// Immutable server/credential context. A clone never consults a replaceable token.
#[derive(Clone)]
pub struct AuthenticatedClient(pub(super) Arc<AuthenticatedContext>);

pub(super) struct AuthenticatedContext {
    pub server: ServerClient,
    pub token: String,
}

/// No credential is exported in successful bound authentication results.
#[derive(Clone, Debug)]
pub struct Authentication {
    pub client: AuthenticatedClient,
    pub user: User,
    pub expires_at: i64,
}

impl ServerClient {
    pub fn server_url(&self) -> &Url {
        &self.0.url
    }

    /// A private candidate for saved-login verification, not an accepted session.
    /// Session coordination must verify current user, identity and expiry before publication.
    pub fn restore_candidate(&self, token: String) -> Result<AuthenticatedClient, ApiError> {
        if token.is_empty() {
            return Err(ApiError::InvalidResponse);
        }
        Ok(AuthenticatedClient(Arc::new(AuthenticatedContext {
            server: self.clone(),
            token,
        })))
    }

    pub(super) fn endpoint(&self, path: &str) -> Url {
        self.0.url.join(path).expect("static API path")
    }
}

impl AuthenticatedClient {
    /// Restricted session/storage seam, not a view or chat interface.
    /// Session persistence copies directly into the ordered secure-store operation.
    pub(crate) fn credential_for_session(&self) -> &str {
        &self.0.token
    }

    pub fn server_url(&self) -> &Url {
        self.0.server.server_url()
    }

    pub(super) fn request(&self, method: reqwest::Method, url: Url) -> RequestBuilder {
        self.0
            .server
            .0
            .transport
            .client
            .request(method, url)
            .bearer_auth(&self.0.token)
    }
}

impl std::fmt::Debug for AuthenticatedClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthenticatedClient")
            .field("server", &self.server_url())
            .finish_non_exhaustive()
    }
}
