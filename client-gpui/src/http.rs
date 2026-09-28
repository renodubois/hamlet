use crate::conversation::{Channel, Message};
use crate::session::{ApiFuture, AuthApi, AuthError, Login, User};
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

pub fn validate_server(value: &str) -> Result<Url, AuthError> {
    let url = Url::parse(value).map_err(|_| AuthError::InvalidResponse)?;
    let host = url.host_str().ok_or(AuthError::InvalidResponse)?;
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
        return Err(AuthError::InvalidResponse);
    }
    Ok(url)
}

#[derive(Clone)]
pub struct HttpAuth {
    client: Client,
}

impl HttpAuth {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .no_proxy()
                .resolve_to_addrs(
                    "localhost",
                    &[
                        SocketAddr::from(([127, 0, 0, 1], 0)),
                        SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 0)),
                    ],
                )
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(8))
                .build()
                .expect("HTTP client"),
        }
    }
}

#[derive(Deserialize)]
struct WireUser {
    id: String,
    username: String,
}
#[derive(Deserialize)]
struct WireLogin {
    user: WireUser,
    access_token: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Deserialize)]
struct WireError {
    error: WireErrorInfo,
}
#[derive(Deserialize)]
struct WireErrorInfo {
    code: String,
}

#[derive(Deserialize)]
struct WireChannels {
    items: Vec<WireChannel>,
}
#[derive(Deserialize)]
struct WireChannel {
    id: String,
    name: String,
    #[serde(rename = "type")]
    kind: String,
}
#[derive(Deserialize)]
struct WireHistory {
    items: Vec<WireMessage>,
    next_cursor: Option<String>,
}
#[derive(Deserialize)]
struct WireMessage {
    id: String,
    channel_id: String,
    author: WireAuthor,
    text: String,
    created_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Deserialize)]
struct WireAuthor {
    id: String,
    display_name: String,
}

async fn protected_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, AuthError> {
    match response.status() {
        StatusCode::OK => response
            .json()
            .await
            .map_err(|_| AuthError::InvalidResponse),
        StatusCode::UNAUTHORIZED => Err(AuthError::AlreadyInvalid),
        StatusCode::BAD_REQUEST => Err(AuthError::InvalidInput),
        StatusCode::NOT_FOUND => Err(AuthError::NotFound),
        StatusCode::INTERNAL_SERVER_ERROR => Err(AuthError::ServerFailure),
        _ => Err(AuthError::Unavailable),
    }
}

async fn decode_created_channel(response: reqwest::Response) -> Result<Channel, AuthError> {
    match response.status() {
        StatusCode::CREATED => {
            let wire: WireChannel = response
                .json()
                .await
                .map_err(|_| AuthError::InvalidResponse)?;
            if wire.id.is_empty() || wire.name.is_empty() || wire.kind != "text" {
                return Err(AuthError::InvalidResponse);
            }
            Ok(Channel {
                id: wire.id,
                name: wire.name,
            })
        }
        StatusCode::UNAUTHORIZED => Err(AuthError::AlreadyInvalid),
        StatusCode::BAD_REQUEST | StatusCode::CONFLICT => {
            let status = response.status();
            let body: WireError = response
                .json()
                .await
                .map_err(|_| AuthError::InvalidResponse)?;
            match (status, body.error.code.as_str()) {
                (StatusCode::BAD_REQUEST, "bad_request") => Err(AuthError::InvalidInput),
                (StatusCode::CONFLICT, "conflict") => Err(AuthError::Conflict),
                _ => Err(AuthError::InvalidResponse),
            }
        }
        StatusCode::INTERNAL_SERVER_ERROR => Err(AuthError::ServerFailure),
        _ => Err(AuthError::Unavailable),
    }
}

async fn decode_auth(response: reqwest::Response, success: StatusCode) -> Result<Login, AuthError> {
    if response.status() == success {
        let wire: WireLogin = response
            .json()
            .await
            .map_err(|_| AuthError::InvalidResponse)?;
        if wire.access_token.is_empty() || wire.user.id.is_empty() || wire.user.username.is_empty()
        {
            return Err(AuthError::InvalidResponse);
        }
        return Ok(Login {
            user: User {
                id: wire.user.id,
                username: wire.user.username,
            },
            token: wire.access_token,
            expires_at: wire.expires_at.timestamp(),
        });
    }
    let status = response.status();
    if status == StatusCode::UNAUTHORIZED
        || status == StatusCode::BAD_REQUEST
        || status == StatusCode::CONFLICT
    {
        let body: WireError = response
            .json()
            .await
            .map_err(|_| AuthError::InvalidResponse)?;
        return match (status, body.error.code.as_str()) {
            (StatusCode::UNAUTHORIZED, "unauthorized") => Err(AuthError::InvalidCredentials),
            (StatusCode::BAD_REQUEST, "bad_request") => Err(AuthError::InvalidInput),
            (StatusCode::CONFLICT, "conflict") if success == StatusCode::CREATED => {
                Err(AuthError::Conflict)
            }
            _ => Err(AuthError::InvalidResponse),
        };
    }
    Err(AuthError::Unavailable)
}

impl AuthApi for HttpAuth {
    fn signup(
        &self,
        server: String,
        username: String,
        password: String,
    ) -> ApiFuture<Result<Login, AuthError>> {
        let client = self.client.clone();
        Box::pin(async move {
            let url = validate_server(&server)?
                .join("api/v1/auth/signup")
                .map_err(|_| AuthError::InvalidResponse)?;
            let response = client
                .post(url)
                .json(&serde_json::json!({"username": username, "password": password}))
                .send()
                .await
                .map_err(|_| AuthError::Unavailable)?;
            decode_auth(response, StatusCode::CREATED).await
        })
    }

    fn login(
        &self,
        server: String,
        username: String,
        password: String,
    ) -> ApiFuture<Result<Login, AuthError>> {
        let client = self.client.clone();
        Box::pin(async move {
            let url = validate_server(&server)?;
            let response = client
                .post(
                    url.join("api/v1/auth/login")
                        .map_err(|_| AuthError::InvalidResponse)?,
                )
                .json(&serde_json::json!({"username": username, "password": password}))
                .send()
                .await
                .map_err(|_| AuthError::Unavailable)?;
            decode_auth(response, StatusCode::OK).await
        })
    }

    fn channels(
        &self,
        server: String,
        token: String,
    ) -> ApiFuture<Result<Vec<Channel>, AuthError>> {
        let client = self.client.clone();
        Box::pin(async move {
            let url = validate_server(&server)?
                .join("api/v1/channels")
                .map_err(|_| AuthError::InvalidResponse)?;
            let wire: WireChannels = protected_response(
                client
                    .get(url)
                    .bearer_auth(token)
                    .send()
                    .await
                    .map_err(|_| AuthError::Unavailable)?,
            )
            .await?;
            wire.items
                .into_iter()
                .map(|item| {
                    if item.id.is_empty() || item.name.is_empty() || item.kind != "text" {
                        return Err(AuthError::InvalidResponse);
                    }
                    Ok(Channel {
                        id: item.id,
                        name: item.name,
                    })
                })
                .collect()
        })
    }

    fn create_channel(
        &self,
        server: String,
        token: String,
        name: String,
    ) -> ApiFuture<Result<Channel, AuthError>> {
        let client = self.client.clone();
        Box::pin(async move {
            let url = validate_server(&server)?
                .join("api/v1/channels")
                .map_err(|_| AuthError::InvalidResponse)?;
            decode_created_channel(
                client
                    .post(url)
                    .bearer_auth(token)
                    .json(&serde_json::json!({"name": name, "type": "text"}))
                    .send()
                    .await
                    .map_err(|_| AuthError::Unavailable)?,
            )
            .await
        })
    }

    fn history(
        &self,
        server: String,
        token: String,
        channel_id: String,
    ) -> ApiFuture<Result<Vec<Message>, AuthError>> {
        let client = self.client.clone();
        Box::pin(async move {
            // The server supplies channel IDs. Encode them as one path segment rather than
            // assuming their format or allowing an ID to escape the history route.
            if channel_id.is_empty()
                || channel_id.contains(['/', '\\'])
                || channel_id == "."
                || channel_id == ".."
            {
                return Err(AuthError::InvalidResponse);
            }
            let mut url = validate_server(&server)?;
            url.path_segments_mut()
                .map_err(|_| AuthError::InvalidResponse)?
                .extend(["api", "v1", "channels", &channel_id, "messages"]);
            let wire: WireHistory = protected_response(
                client
                    .get(url)
                    .bearer_auth(token)
                    .send()
                    .await
                    .map_err(|_| AuthError::Unavailable)?,
            )
            .await?;
            let _ = wire.next_cursor; // Older-page traversal is a later slice.
            wire.items
                .into_iter()
                .map(|item| {
                    if item.id.is_empty()
                        || item.channel_id != channel_id
                        || item.author.id.is_empty()
                        || item.author.display_name.is_empty()
                    {
                        return Err(AuthError::InvalidResponse);
                    }
                    Ok(Message {
                        id: item.id,
                        channel_id: item.channel_id,
                        author_id: item.author.id,
                        author_name: item.author.display_name,
                        text: item.text,
                        created_at: item.created_at.to_rfc3339(),
                    })
                })
                .collect()
        })
    }

    fn logout(&self, server: String, token: String) -> ApiFuture<Result<(), AuthError>> {
        let client = self.client.clone();
        Box::pin(async move {
            let url = validate_server(&server)?;
            let response = client
                .post(
                    url.join("api/v1/auth/logout")
                        .map_err(|_| AuthError::InvalidResponse)?,
                )
                .bearer_auth(token)
                .send()
                .await
                .map_err(|_| AuthError::Unavailable)?;
            match response.status() {
                StatusCode::NO_CONTENT => Ok(()),
                StatusCode::UNAUTHORIZED => Err(AuthError::AlreadyInvalid),
                _ => Err(AuthError::Unavailable),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    // TCP fixtures exercise the public AuthApi (not private decoding helpers).
    fn server(reply: impl Into<String>) -> (String, thread::JoinHandle<String>) {
        let reply = reply.into();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let count = stream.read(&mut buf).unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..count]);
                if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|length| length.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            stream.write_all(reply.as_bytes()).unwrap();
            String::from_utf8(request).unwrap()
        });
        (url, handle)
    }
    fn response(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }
    #[tokio::test]
    async fn login_decodes_rewrite_contract_and_unauthorized() {
        let body = r#"{"user":{"id":"42","username":"ada"},"access_token":"abc","expires_at":"2099-01-01T00:00:00Z"}"#;
        let reply = response("200 OK", body);
        let (url, captured) = server(reply);
        let login = HttpAuth::new()
            .login(url, "ada".into(), "pass".into())
            .await
            .unwrap();
        assert_eq!(login.user.username, "ada");
        assert_eq!(login.token, "abc");
        let request = captured.join().unwrap();
        assert!(request.starts_with("POST /api/v1/auth/login "));
        assert!(request.contains("\"password\":\"pass\""));
        let reply = response(
            "401 Unauthorized",
            r#"{"error":{"code":"unauthorized","message":"Unauthorized"}}"#,
        );
        let (url, captured) = server(reply);
        assert!(matches!(
            HttpAuth::new().login(url, "ada".into(), "bad".into()).await,
            Err(AuthError::InvalidCredentials)
        ));
        captured.join().unwrap();
    }
    #[tokio::test]
    async fn unexpected_conflict_on_login_is_not_a_signup_error() {
        let (url, captured) = server(response(
            "409 Conflict",
            r#"{"error":{"code":"conflict","message":"Already exists"}}"#,
        ));
        assert!(matches!(
            HttpAuth::new()
                .login(url, "ada".into(), "pass".into())
                .await,
            Err(AuthError::InvalidResponse)
        ));
        captured.join().unwrap();
    }

    #[tokio::test]
    async fn logout_sends_bearer_and_redirects_are_not_followed() {
        let (url, captured) =
            server("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        HttpAuth::new().logout(url, "secret".into()).await.unwrap();
        let request = captured.join().unwrap();
        assert!(request.starts_with("POST /api/v1/auth/logout "));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer secret")
        );
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        destination.set_nonblocking(true).unwrap();
        let reply = format!(
            "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            destination.local_addr().unwrap()
        );
        let (url, captured) = server(reply);
        assert!(matches!(
            HttpAuth::new().logout(url, "secret".into()).await,
            Err(AuthError::Unavailable)
        ));
        captured.join().unwrap();
        assert!(
            destination.accept().is_err(),
            "redirect sent bearer token to another server"
        );
    }
    #[tokio::test]
    async fn rejects_login_redirect_without_contacting_destination() {
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        destination.set_nonblocking(true).unwrap();
        let reply = format!(
            "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            destination.local_addr().unwrap()
        );
        let (url, captured) = server(reply);
        assert!(matches!(
            HttpAuth::new()
                .login(url, "ada".into(), "pass".into())
                .await,
            Err(AuthError::Unavailable)
        ));
        captured.join().unwrap();
        assert!(
            destination.accept().is_err(),
            "redirect sent credentials to another server"
        );
    }
    #[tokio::test]
    async fn bounded_revocation_and_error_decoding() {
        let reply = response(
            "400 Bad Request",
            r#"{"error":{"code":"bad_request","message":"Invalid request"}}"#,
        );
        let (url, captured) = server(reply);
        assert!(matches!(
            HttpAuth::new()
                .login(url, "ada".into(), "pass".into())
                .await,
            Err(AuthError::InvalidInput)
        ));
        captured.join().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let waiting = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(150));
            drop(stream);
        });
        let auth = HttpAuth {
            client: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_millis(30))
                .build()
                .unwrap(),
        };
        let started = std::time::Instant::now();
        assert!(matches!(
            auth.logout(url, "secret".into()).await,
            Err(AuthError::Unavailable)
        ));
        assert!(started.elapsed() < Duration::from_millis(130));
        waiting.join().unwrap();
    }
    // Exercise the unchanged server-rewrite routes over real HTTP using the public AuthApi.
    #[actix_web::test]
    async fn rewrite_routes_signup_login_logout_and_errors() {
        use actix_web::{App, HttpServer, web};
        use hamlet::{connect_to_database, routes};

        let dir = tempfile::tempdir().unwrap();
        let db_url = format!("sqlite://{}?mode=rwc", dir.path().join("auth.db").display());
        let state = connect_to_database(&db_url).await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(state.clone()))
                .configure(routes)
        })
        .listen(listener)
        .unwrap()
        .run();
        let server_handle = server.handle();
        actix_web::rt::spawn(server);

        let client = Client::new();
        let signup = client
            .post(format!("{url}/api/v1/auth/signup"))
            .json(&serde_json::json!({"username": "Alice", "password": "long password"}))
            .send()
            .await
            .unwrap();
        assert_eq!(signup.status(), StatusCode::CREATED);
        let bootstrap: serde_json::Value = signup.json().await.unwrap();
        let bootstrap_token = bootstrap["access_token"].as_str().unwrap();
        assert!(!bootstrap_token.is_empty());
        let me = client
            .get(format!("{url}/api/v1/me"))
            .bearer_auth(bootstrap_token)
            .send()
            .await
            .unwrap();
        assert_eq!(me.status(), StatusCode::OK);

        let auth: &dyn AuthApi = &HttpAuth::new();
        let login = auth
            .login(url.clone(), "aLiCe".into(), "long password".into())
            .await
            .unwrap();
        assert_eq!(login.user.username, "Alice");
        assert_eq!(login.user.id, bootstrap["user"]["id"].as_str().unwrap());
        assert_ne!(login.token, bootstrap_token);
        assert!(login.expires_at > chrono::Utc::now().timestamp());

        let channel = client
            .post(format!("{url}/api/v1/channels"))
            .bearer_auth(&login.token)
            .json(&serde_json::json!({"name":"alpha", "type":"text"}))
            .send()
            .await
            .unwrap();
        assert_eq!(channel.status(), StatusCode::CREATED);
        let channel: serde_json::Value = channel.json().await.unwrap();
        let channel_id = channel["id"].as_str().unwrap();
        let list = auth
            .channels(url.clone(), login.token.clone())
            .await
            .unwrap();
        assert_eq!(
            list.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["alpha", "general"]
        );
        assert_eq!(list[0].id, channel_id);
        assert!(matches!(
            auth.channels(url.clone(), "bad".into()).await,
            Err(AuthError::AlreadyInvalid)
        ));
        assert!(matches!(
            auth.history(url.clone(), "bad".into(), channel_id.into())
                .await,
            Err(AuthError::AlreadyInvalid)
        ));
        let empty = auth
            .history(url.clone(), login.token.clone(), channel_id.into())
            .await
            .unwrap();
        assert!(empty.is_empty());
        for text in ["first\nline", "second"] {
            let posted = client
                .post(format!("{url}/api/v1/channels/{channel_id}/messages"))
                .bearer_auth(&login.token)
                .json(&serde_json::json!({"text":text}))
                .send()
                .await
                .unwrap();
            assert_eq!(posted.status(), StatusCode::CREATED);
        }
        let newest = auth
            .history(url.clone(), login.token.clone(), channel_id.into())
            .await
            .unwrap();
        assert_eq!(newest.len(), 2);
        assert_eq!(newest[0].text, "second");
        assert_eq!(newest[1].text, "first\nline");
        assert_eq!(newest[0].author_id, login.user.id);
        assert_eq!(newest[0].author_name, "Alice");
        assert_eq!(newest[0].channel_id, channel_id);
        assert_ne!(newest[0].id, newest[1].id);
        assert!(newest[0].created_at.contains('T'));
        assert!(matches!(
            auth.history(url.clone(), login.token.clone(), "999999999999999".into())
                .await,
            Err(AuthError::NotFound)
        ));
        assert!(matches!(
            auth.history(url.clone(), login.token.clone(), "../bad".into())
                .await,
            Err(AuthError::InvalidResponse)
        ));
        assert!(matches!(
            auth.login(url.clone(), "Alice".into(), "bad password".into())
                .await,
            Err(AuthError::InvalidCredentials)
        ));

        // The public client always serializes a well-formed credentials object. Send broken JSON
        // separately to verify the real route's 400 envelope, not a homemade fixture's response.
        let malformed = client
            .post(format!("{url}/api/v1/auth/login"))
            .header("content-type", "application/json")
            .body("{not json")
            .send()
            .await
            .unwrap();
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
        let error: serde_json::Value = malformed.json().await.unwrap();
        assert_eq!(error["error"]["code"], "bad_request");
        assert!(matches!(
            auth.login("http://example.com".into(), "Alice".into(), "pass".into())
                .await,
            Err(AuthError::InvalidResponse)
        ));

        // The second token stays live; this distinguishes a protected bearer logout from a
        // misleading 204 on a public route or an invalid-token 401 interpreted as success.
        auth.logout(url.clone(), bootstrap_token.into())
            .await
            .unwrap();
        assert_eq!(
            client
                .get(format!("{url}/api/v1/me"))
                .bearer_auth(bootstrap_token)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            client
                .get(format!("{url}/api/v1/me"))
                .bearer_auth(&login.token)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert!(matches!(
            auth.logout(url, bootstrap_token.into()).await,
            Err(AuthError::AlreadyInvalid)
        ));
        server_handle.stop(true).await;
    }

    #[actix_web::test]
    async fn signup_against_rewrite_routes_returns_usable_session_and_rejections() {
        use actix_web::{App, HttpServer, web};
        use hamlet::{connect_to_database, routes};
        let dir = tempfile::tempdir().unwrap();
        let db = connect_to_database(&format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("signup.db").display()
        ))
        .await
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(db.clone()))
                .configure(routes)
        })
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);
        let auth: &dyn AuthApi = &HttpAuth::new();
        let created = auth
            .signup(url.clone(), "Alice_1".into(), "long password".into())
            .await
            .unwrap();
        assert_eq!(created.user.username, "Alice_1");
        assert!(!created.user.id.is_empty());
        assert!(created.expires_at > chrono::Utc::now().timestamp());
        let me = Client::new()
            .get(format!("{url}/api/v1/me"))
            .bearer_auth(&created.token)
            .send()
            .await
            .unwrap();
        assert_eq!(me.status(), StatusCode::OK);
        let user: serde_json::Value = me.json().await.unwrap();
        assert_eq!(user["id"], created.user.id);
        assert!(matches!(
            auth.signup(url.clone(), "aLiCe_1".into(), "another password".into())
                .await,
            Err(AuthError::Conflict)
        ));
        assert!(matches!(
            auth.signup(url.clone(), "bad!".into(), "long password".into())
                .await,
            Err(AuthError::InvalidInput)
        ));
        assert!(matches!(
            auth.signup(url.clone(), "NewUser".into(), "short".into())
                .await,
            Err(AuthError::InvalidInput)
        ));
        handle.stop(true).await;
    }

    #[actix_web::test]
    async fn create_channel_against_unchanged_rewrite_routes() {
        use actix_web::{App, HttpServer, web};
        use hamlet::{connect_to_database, routes};
        let dir = tempfile::tempdir().unwrap();
        let db = connect_to_database(&format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("channels.db").display()
        ))
        .await
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(db.clone()))
                .configure(routes)
        })
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);
        let auth: &dyn AuthApi = &HttpAuth::new();
        let login = auth
            .signup(url.clone(), "Alice".into(), "long password".into())
            .await
            .unwrap();
        let channel = auth
            .create_channel(url.clone(), login.token.clone(), "  New Room  ".into())
            .await
            .unwrap();
        assert_eq!(channel.name, "New Room");
        assert!(!channel.id.is_empty());
        assert!(
            auth.channels(url.clone(), login.token.clone())
                .await
                .unwrap()
                .contains(&channel)
        );
        assert!(matches!(
            auth.create_channel(url.clone(), login.token.clone(), "new room".into())
                .await,
            Err(AuthError::Conflict)
        ));
        assert!(matches!(
            auth.create_channel(url.clone(), login.token.clone(), "bad!".into())
                .await,
            Err(AuthError::InvalidInput)
        ));
        assert!(matches!(
            auth.create_channel(url.clone(), "bad".into(), "Other".into())
                .await,
            Err(AuthError::AlreadyInvalid)
        ));
        handle.stop(true).await;
    }

    #[tokio::test]
    async fn create_decodes_response_and_sends_text_type_without_redirect() {
        let (url, captured) = server(response(
            "201 Created",
            r#"{"id":"123","name":"Trimmed","type":"text"}"#,
        ));
        assert_eq!(
            HttpAuth::new()
                .create_channel(url, "secret".into(), "  Trimmed  ".into())
                .await
                .unwrap(),
            Channel {
                id: "123".into(),
                name: "Trimmed".into()
            }
        );
        let request = captured.join().unwrap();
        assert!(request.starts_with("POST /api/v1/channels "));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer secret")
        );
        assert!(request.contains("\"type\":\"text\""));
        let (url, captured) = server(response(
            "201 Created",
            r#"{"id":"123","name":"A","type":"voice"}"#,
        ));
        assert!(matches!(
            HttpAuth::new()
                .create_channel(url, "secret".into(), "A".into())
                .await,
            Err(AuthError::InvalidResponse)
        ));
        captured.join().unwrap();
        let (url, captured) = server(response("409 Conflict", r#"{"error":{"code":"conflict"}}"#));
        assert!(matches!(
            HttpAuth::new()
                .create_channel(url, "secret".into(), "A".into())
                .await,
            Err(AuthError::Conflict)
        ));
        captured.join().unwrap();
    }

    #[tokio::test]
    async fn signup_redirect_does_not_forward_password() {
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        destination.set_nonblocking(true).unwrap();
        let reply = format!(
            "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            destination.local_addr().unwrap()
        );
        let (url, captured) = server(reply);
        assert!(matches!(
            HttpAuth::new()
                .signup(url, "Alice".into(), "long password".into())
                .await,
            Err(AuthError::Unavailable)
        ));
        let request = captured.join().unwrap();
        assert!(request.starts_with("POST /api/v1/auth/signup "));
        assert!(destination.accept().is_err());
        assert!(matches!(
            HttpAuth::new()
                .signup(
                    "http://example.com".into(),
                    "Alice".into(),
                    "long password".into()
                )
                .await,
            Err(AuthError::InvalidResponse)
        ));
        let (url, captured) = server(response(
            "200 OK",
            r#"{"user":{"id":"42","username":"Alice"},"access_token":"abc","expires_at":"2099-01-01T00:00:00Z"}"#,
        ));
        assert!(matches!(
            HttpAuth::new()
                .signup(url, "Alice".into(), "long password".into())
                .await,
            Err(AuthError::Unavailable)
        ));
        captured.join().unwrap();
    }

    #[test]
    fn server_urls_disallow_remote_http_credentials_and_paths() {
        for url in [
            "http://example.com",
            "http://192.168.1.2",
            "https://user:pass@example.com",
            "https://example.com/other",
            "https://example.com?x=1",
            "file:///tmp/secret",
        ] {
            assert!(validate_server(url).is_err(), "{url}");
        }
        for url in [
            "http://localhost:8081",
            "http://127.0.0.1:8081",
            "http://[::1]:8081",
            "https://example.com",
        ] {
            assert!(validate_server(url).is_ok(), "{url}");
        }
    }
}
