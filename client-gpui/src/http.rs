use crate::conversation::{Channel, Message, Page};
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

async fn decode_created_message(
    response: reqwest::Response,
    channel_id: &str,
) -> Result<Message, AuthError> {
    match response.status() {
        StatusCode::CREATED => {
            let wire: WireMessage = response
                .json()
                .await
                .map_err(|_| AuthError::InvalidResponse)?;
            decode_message(wire, channel_id)
        }
        StatusCode::UNAUTHORIZED => Err(AuthError::AlreadyInvalid),
        StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND => {
            let status = response.status();
            let body: WireError = response
                .json()
                .await
                .map_err(|_| AuthError::InvalidResponse)?;
            match (status, body.error.code.as_str()) {
                (StatusCode::BAD_REQUEST, "bad_request") => Err(AuthError::InvalidInput),
                (StatusCode::NOT_FOUND, "not_found") => Err(AuthError::NotFound),
                _ => Err(AuthError::InvalidResponse),
            }
        }
        StatusCode::INTERNAL_SERVER_ERROR => Err(AuthError::ServerFailure),
        _ => Err(AuthError::Unavailable),
    }
}

fn decode_message(wire: WireMessage, channel_id: &str) -> Result<Message, AuthError> {
    if wire.id.is_empty()
        || wire.channel_id != channel_id
        || wire.author.id.is_empty()
        || wire.author.display_name.is_empty()
    {
        return Err(AuthError::InvalidResponse);
    }
    Ok(Message {
        id: wire.id,
        channel_id: wire.channel_id,
        author_id: wire.author.id,
        author_name: wire.author.display_name,
        text: wire.text,
        created_at: wire.created_at.to_rfc3339(),
    })
}

fn message_url(server: &str, channel_id: &str) -> Result<Url, AuthError> {
    if channel_id.is_empty()
        || channel_id.contains(['/', '\\'])
        || channel_id == "."
        || channel_id == ".."
    {
        return Err(AuthError::InvalidResponse);
    }
    let mut url = validate_server(server)?;
    url.path_segments_mut()
        .map_err(|_| AuthError::InvalidResponse)?
        .extend(["api", "v1", "channels", channel_id, "messages"]);
    Ok(url)
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

    fn current_user(&self, server: String, token: String) -> ApiFuture<Result<User, AuthError>> {
        let client = self.client.clone();
        Box::pin(async move {
            let url = validate_server(&server)?
                .join("api/v1/me")
                .map_err(|_| AuthError::InvalidResponse)?;
            let wire: WireUser = protected_response(
                client
                    .get(url)
                    .bearer_auth(token)
                    .send()
                    .await
                    .map_err(|_| AuthError::Unavailable)?,
            )
            .await?;
            if wire.id.is_empty() || wire.username.is_empty() {
                return Err(AuthError::InvalidResponse);
            }
            Ok(User {
                id: wire.id,
                username: wire.username,
            })
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

    fn send_message(
        &self,
        server: String,
        token: String,
        channel_id: String,
        text: String,
    ) -> ApiFuture<Result<Message, AuthError>> {
        let client = self.client.clone();
        Box::pin(async move {
            let url = message_url(&server, &channel_id)?;
            // One POST only. In particular, a timeout, lost response, malformed success or
            // server failure must never turn into an automatic replay of a non-idempotent write.
            decode_created_message(
                client
                    .post(url)
                    .bearer_auth(token)
                    .json(&serde_json::json!({"text": text}))
                    .send()
                    .await
                    .map_err(|_| AuthError::Unavailable)?,
                &channel_id,
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
        let page = self.history_page(server, token, channel_id, None);
        Box::pin(async move { page.await.map(|page| page.items) })
    }

    fn history_page(
        &self,
        server: String,
        token: String,
        channel_id: String,
        before: Option<String>,
    ) -> ApiFuture<Result<Page, AuthError>> {
        let client = self.client.clone();
        Box::pin(async move {
            let mut url = message_url(&server, &channel_id)?;
            if let Some(cursor) = before.as_ref() {
                // Query serialization treats the server-issued cursor as opaque, even if it
                // contains reserved URL characters. Never decode or synthesize one.
                url.query_pairs_mut().append_pair("before", cursor);
            }
            let wire: WireHistory = protected_response(
                client
                    .get(url)
                    .bearer_auth(token)
                    .send()
                    .await
                    .map_err(|_| AuthError::Unavailable)?,
            )
            .await?;
            let items = wire
                .items
                .into_iter()
                .map(|item| decode_message(item, &channel_id))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Page {
                items,
                next_cursor: wire.next_cursor,
            })
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
    #[actix_web::test]
    async fn send_message_uses_real_rewrite_validation_and_decodes_created_identity() {
        use actix_web::{App, HttpServer, web};
        use hamlet::{connect_to_database, routes};
        let dir = tempfile::tempdir().unwrap();
        let db = connect_to_database(&format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("messages.db").display()
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
            .signup(url.clone(), "Ada".into(), "long password".into())
            .await
            .unwrap();
        let channel = auth
            .channels(url.clone(), login.token.clone())
            .await
            .unwrap()[0]
            .id
            .clone();
        let sent = auth
            .send_message(
                url.clone(),
                login.token.clone(),
                channel.clone(),
                "first\nsecond".into(),
            )
            .await
            .unwrap();
        assert_eq!(sent.channel_id, channel);
        assert_eq!(sent.author_id, login.user.id);
        assert_eq!(sent.author_name, "Ada");
        assert_eq!(sent.text, "first\nsecond");
        assert!(!sent.id.is_empty());
        assert!(sent.created_at.contains('T'));
        assert_eq!(
            auth.history(url.clone(), login.token.clone(), channel.clone())
                .await
                .unwrap(),
            vec![sent]
        );
        for text in ["   ".to_owned(), "x".repeat(4001)] {
            assert!(matches!(
                auth.send_message(url.clone(), login.token.clone(), channel.clone(), text)
                    .await,
                Err(AuthError::InvalidInput)
            ));
        }
        assert!(matches!(
            auth.send_message(
                url.clone(),
                login.token.clone(),
                "999999999999999".into(),
                "hi".into()
            )
            .await,
            Err(AuthError::NotFound)
        ));
        assert!(matches!(
            auth.send_message(url.clone(), "bad".into(), channel, "hi".into())
                .await,
            Err(AuthError::AlreadyInvalid)
        ));
        handle.stop(true).await;
    }

    #[tokio::test]
    async fn ambiguous_write_timeout_does_not_replay_and_malformed_success_is_uncertain() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let captured = thread::spawn(move || {
            let start = std::time::Instant::now();
            let mut first = None;
            while start.elapsed() < Duration::from_secs(2) && first.is_none() {
                if let Ok((stream, _)) = listener.accept() {
                    first = Some(stream);
                } else {
                    thread::sleep(Duration::from_millis(2));
                }
            }
            let mut stream = first.expect("one write reached server");
            stream
                .set_read_timeout(Some(Duration::from_millis(100)))
                .unwrap();
            let mut request = [0u8; 4096];
            let count = stream.read(&mut request).unwrap();
            thread::sleep(Duration::from_millis(120));
            assert!(
                listener.accept().is_err(),
                "client retried an ambiguous POST"
            );
            String::from_utf8_lossy(&request[..count]).to_string()
        });
        let auth = HttpAuth {
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_millis(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
        };
        assert!(matches!(
            auth.send_message(url, "token".into(), "123".into(), "do not retry".into())
                .await,
            Err(AuthError::Unavailable)
        ));
        let captured = captured.join().unwrap();
        assert!(captured.starts_with("POST /api/v1/channels/123/messages "));
        assert!(captured.contains("do not retry"));
        let (url, request) = server(response("201 Created", "{bad json"));
        assert!(matches!(
            HttpAuth::new()
                .send_message(url, "token".into(), "123".into(), "hi".into())
                .await,
            Err(AuthError::InvalidResponse)
        ));
        request.join().unwrap();
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
    async fn current_user_requires_bearer_and_rejects_redirect_or_malformed_identity() {
        let (url, captured) = server(response("200 OK", r#"{"id":"42","username":"Ada"}"#));
        assert_eq!(
            HttpAuth::new()
                .current_user(url, "secret".into())
                .await
                .unwrap(),
            User {
                id: "42".into(),
                username: "Ada".into()
            }
        );
        let request = captured.join().unwrap();
        assert!(request.starts_with("GET /api/v1/me "));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer secret")
        );
        let (url, captured) = server(response("401 Unauthorized", "{}"));
        assert!(matches!(
            HttpAuth::new().current_user(url, "bad".into()).await,
            Err(AuthError::AlreadyInvalid)
        ));
        captured.join().unwrap();
        let (url, captured) = server(response("200 OK", r#"{"id":"","username":"Ada"}"#));
        assert!(matches!(
            HttpAuth::new().current_user(url, "secret".into()).await,
            Err(AuthError::InvalidResponse)
        ));
        captured.join().unwrap();
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        destination.set_nonblocking(true).unwrap();
        let reply = format!(
            "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            destination.local_addr().unwrap()
        );
        let (url, captured) = server(reply);
        assert!(matches!(
            HttpAuth::new().current_user(url, "secret".into()).await,
            Err(AuthError::Unavailable)
        ));
        captured.join().unwrap();
        assert!(destination.accept().is_err());
    }

    #[tokio::test]
    async fn current_user_timeout_is_not_authoritative_invalidity() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let waiting = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(120));
            drop(stream);
        });
        let auth = HttpAuth {
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_millis(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
        };
        assert!(matches!(
            auth.current_user(url, "still-valid-until-verified".into())
                .await,
            Err(AuthError::Unavailable)
        ));
        waiting.join().unwrap();
    }

    #[actix_web::test]
    async fn current_user_against_rewrite_routes_before_and_after_revocation() {
        use actix_web::{App, HttpServer, web};
        use hamlet::{connect_to_database, routes};
        let dir = tempfile::tempdir().unwrap();
        let db = connect_to_database(&format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("me.db").display()
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
        let api: &dyn AuthApi = &HttpAuth::new();
        let login = api
            .signup(url.clone(), "Ada".into(), "long password".into())
            .await
            .unwrap();
        assert_eq!(
            api.current_user(url.clone(), login.token.clone())
                .await
                .unwrap(),
            login.user
        );
        assert!(matches!(
            api.current_user(url.clone(), "bad".into()).await,
            Err(AuthError::AlreadyInvalid)
        ));
        api.logout(url.clone(), login.token.clone()).await.unwrap();
        assert!(matches!(
            api.current_user(url, login.token).await,
            Err(AuthError::AlreadyInvalid)
        ));
        handle.stop(true).await;
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
    async fn signup_uses_shared_persistent_session_and_verified_restore_against_rewrite_routes() {
        use crate::persistence::{Outcome, Persistence, Selection, tests::Controlled};
        use crate::session::{AppSession, RestoreDecision, RestoreResult};
        use actix_web::{App, HttpServer, web};
        use std::sync::{Arc, Condvar, Mutex};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let db = hamlet::connect_to_database(&format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("signup-restore.db").display()
        ))
        .await
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(db.clone()))
                .configure(hamlet::routes)
        })
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);
        let api: Arc<dyn AuthApi> = Arc::new(HttpAuth::new());
        let mut session = AppSession::new(api.clone());
        session.change_server(url.clone());
        session.username = "Alice".into();
        session.password = "long password".into();
        let request = session.submit_signup().unwrap();
        let created = api
            .signup(
                request.server.clone(),
                request.username.clone(),
                request.password.clone(),
            )
            .await
            .unwrap();
        assert!(session.complete_signup(request, Ok(created), chrono::Utc::now().timestamp()));
        let active = session.active.as_ref().unwrap();
        assert!(session.password.is_empty());
        let selected = Selection {
            server: active.server.clone(),
            user: active.user.clone(),
            expires_at: active.expires_at,
        };
        let token = active.token().to_owned();
        let shared = Arc::new((
            Mutex::new((vec![], false, false, false, false, false)),
            Condvar::new(),
        ));
        let store = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
        assert_eq!(
            store.remember(url.clone()).recv().await.unwrap(),
            Outcome::Remembered
        );
        assert_eq!(
            store
                .save(selected.clone(), token.clone())
                .recv()
                .await
                .unwrap(),
            Outcome::Saved
        );
        let metadata = std::fs::read_to_string(&path).unwrap();
        assert!(!metadata.contains(&token) && !metadata.contains("long password"));
        // This is the same restoration decision path used by the view after reading the store.
        let mut restarted = AppSession::new(api.clone());
        restarted.change_server(url.clone());
        let generation = restarted.begin_restore();
        let stored = match store.read(selected.clone()).recv().await.unwrap() {
            Outcome::Token(Some(token)) => token,
            _ => panic!("saved credential missing"),
        };
        let user = api.current_user(url.clone(), stored.clone()).await.unwrap();
        assert_eq!(
            restarted.finish_restore(
                generation,
                &url,
                &selected.user,
                selected.expires_at,
                RestoreResult::Verified {
                    user,
                    token: stored
                },
                chrono::Utc::now().timestamp()
            ),
            RestoreDecision::Restored
        );
        assert_eq!(restarted.active.as_ref().unwrap().user, selected.user);
        let revocation = restarted.logout().unwrap();
        assert!(restarted.active.is_none());
        assert_eq!(
            store.delete(selected.clone()).recv().await.unwrap(),
            Outcome::Deleted
        );
        api.logout(revocation.server, revocation.token)
            .await
            .unwrap();
        assert_eq!(store.read(selected).recv().await.unwrap(), Outcome::Stale);
        assert!(matches!(
            api.current_user(url, token).await,
            Err(AuthError::AlreadyInvalid)
        ));
        assert!(shared.0.lock().unwrap().0.is_empty());
        handle.stop(true).await;
    }

    #[actix_web::test]
    async fn second_user_activity_is_found_by_focused_polling_against_unchanged_routes() {
        use crate::polling::{Polling, Resource};
        use actix_web::{App, HttpServer, web};
        let dir = tempfile::tempdir().unwrap();
        let db = hamlet::connect_to_database(&format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("polling.db").display()
        ))
        .await
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(db.clone()))
                .configure(hamlet::routes)
        })
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);
        let api: &dyn AuthApi = &HttpAuth::new();
        let alice = api
            .signup(url.clone(), "Alice".into(), "long password".into())
            .await
            .unwrap();
        let bob = api
            .signup(url.clone(), "Bob".into(), "long password".into())
            .await
            .unwrap();
        let channel = api
            .channels(url.clone(), alice.token.clone())
            .await
            .unwrap()[0]
            .id
            .clone();
        let mut session = crate::session::AppSession::new(std::sync::Arc::new(HttpAuth::new()));
        session.change_server(url.clone());
        session.username = "Alice".into();
        session.password = "long password".into();
        let login = session.submit().unwrap();
        session.complete_login(login, Ok(alice), chrono::Utc::now().timestamp());
        let mut conversation = crate::conversation::Conversation::default();
        let list = conversation.start(&session).unwrap();
        let channels = api.channels(url.clone(), list.token.clone()).await.unwrap();
        let initial = conversation
            .complete_channels(&mut session, &list, Ok(channels))
            .unwrap();
        conversation.complete_history(
            &mut session,
            &initial,
            Ok(api
                .history_page(url.clone(), initial.token.clone(), channel.clone(), None)
                .await
                .unwrap()),
        );
        let mut poll = Polling::default();
        assert!(poll.focus(true, Duration::ZERO));
        poll.started(Resource::History);
        poll.started(Resource::Channels);
        poll.completed(Resource::History, true, Duration::ZERO);
        poll.completed(Resource::Channels, true, Duration::ZERO);
        let posted = api
            .send_message(
                url.clone(),
                bob.token.clone(),
                channel.clone(),
                "Bob published while Alice was reading".into(),
            )
            .await
            .unwrap();
        let new_channel = api
            .create_channel(url.clone(), bob.token.clone(), "Bob room".into())
            .await
            .unwrap();
        assert!(poll.due(Resource::History, Duration::from_secs(3)));
        let refresh = conversation.refresh_history(&session).unwrap();
        let mut request = refresh;
        loop {
            let page = api
                .history_page(
                    url.clone(),
                    request.token.clone(),
                    channel.clone(),
                    request.before.clone(),
                )
                .await
                .unwrap();
            let outcome = conversation.complete_history(&mut session, &request, Ok(page));
            if let Some(next) = outcome.next {
                request = next;
            } else {
                break;
            }
        }
        assert!(
            matches!(conversation.history.get(&channel), Some(crate::conversation::Load::Ready(messages)) if messages.iter().any(|m| m.id == posted.id && m.author_name == "Bob"))
        );
        assert!(poll.due(Resource::Channels, Duration::from_secs(15)));
        let listing = conversation.refresh_channels(&session).unwrap();
        let channels = api.channels(url, listing.token.clone()).await.unwrap();
        conversation.complete_channels(&mut session, &listing, Ok(channels));
        assert!(
            matches!(conversation.channels, Some(crate::conversation::Load::Ready(ref channels)) if channels.contains(&new_channel))
        );
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

    #[actix_web::test]
    async fn rewrite_history_traverses_multiple_pages_with_timestamp_ties() {
        use actix_web::{App, HttpServer, web};
        use hamlet::{connect_to_database, routes};
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        let dir = tempfile::tempdir().unwrap();
        let db = connect_to_database(&format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("pages.db").display()
        ))
        .await
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new({
            let db = db.clone();
            move || {
                App::new()
                    .app_data(web::Data::new(db.clone()))
                    .configure(routes)
            }
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
            .channels(url.clone(), login.token.clone())
            .await
            .unwrap()[0]
            .id
            .clone();
        let client = Client::new();
        for ix in 0..53 {
            assert_eq!(
                client
                    .post(format!("{url}/api/v1/channels/{channel}/messages"))
                    .bearer_auth(&login.token)
                    .json(&serde_json::json!({"text": format!("line {ix}")}))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CREATED
            );
        }
        db.db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "UPDATE messages SET created_at = '2026-01-01T00:00:00.000000000Z'",
            ))
            .await
            .unwrap();
        let first = auth
            .history_page(url.clone(), login.token.clone(), channel.clone(), None)
            .await
            .unwrap();
        assert_eq!(first.items.len(), 50);
        let cursor = first
            .next_cursor
            .clone()
            .expect("a server-issued next cursor");
        let last = auth
            .history_page(
                url.clone(),
                login.token.clone(),
                channel.clone(),
                Some(cursor),
            )
            .await
            .unwrap();
        assert_eq!(last.items.len(), 3);
        assert_eq!(last.next_cursor, None);
        let ids: Vec<_> = first
            .items
            .iter()
            .chain(&last.items)
            .map(|m| m.id.parse::<i64>().unwrap())
            .collect();
        assert!(
            ids.windows(2).all(|pair| pair[0] > pair[1]),
            "server tie order must survive decoding"
        );
        assert_eq!(
            ids.iter().collect::<std::collections::HashSet<_>>().len(),
            53
        );
        assert!(
            first
                .items
                .iter()
                .chain(&last.items)
                .all(|m| m.created_at.starts_with("2026-01-01"))
        );
        // Catch up through the real route, not just adapter pagination: a burst spans
        // multiple pages before it can reach the previously loaded 53-message segment.
        let mut session = crate::session::AppSession::new(std::sync::Arc::new(HttpAuth::new()));
        session.change_server(url.clone());
        session.username = "Alice".into();
        session.password = "long password".into();
        let login_request = session.submit().unwrap();
        session.complete_login(login_request, Ok(login), chrono::Utc::now().timestamp());
        let mut conversation = crate::conversation::Conversation::default();
        let channels = conversation.start(&session).unwrap();
        let list = session
            .api
            .channels(url.clone(), session.active.as_ref().unwrap().token().into())
            .await
            .unwrap();
        let initial = conversation
            .complete_channels(&mut session, &channels, Ok(list))
            .unwrap();
        let read = |request: &crate::conversation::ReadRequest,
                    session: &crate::session::AppSession| {
            session.api.history_page(
                request.server.clone(),
                request.token.clone(),
                request.channel_id.clone().unwrap(),
                request.before.clone(),
            )
        };
        let first_page = read(&initial, &session).await.unwrap();
        conversation.complete_history(&mut session, &initial, Ok(first_page));
        for ix in 53..158 {
            assert_eq!(
                client
                    .post(format!("{url}/api/v1/channels/{channel}/messages"))
                    .bearer_auth(session.active.as_ref().unwrap().token())
                    .json(&serde_json::json!({"text": format!("line {ix}")}))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CREATED
            );
        }
        db.db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "UPDATE messages SET created_at = '2027-01-01T00:00:00.000000000Z' WHERE created_at != '2026-01-01T00:00:00.000000000Z'",
            ))
            .await
            .unwrap();
        let mut request = conversation.refresh_history(&session).unwrap();
        let mut traversed = 0;
        loop {
            let page = read(&request, &session).await.unwrap();
            let outcome = conversation.complete_history(&mut session, &request, Ok(page));
            traversed += 1;
            if let Some(next) = outcome.next {
                request = next;
            } else {
                break;
            }
        }
        assert!(
            traversed >= 3,
            "catch-up must traverse beyond two new pages"
        );
        let crate::conversation::Load::Ready(messages) =
            conversation.history.get(&channel).unwrap()
        else {
            panic!("history missing");
        };
        assert_eq!(messages.len(), 155); // 105 new plus the 50 initially loaded; older 3 remain paged
        assert!(
            messages
                .windows(2)
                .all(|pair| pair[0].created_at > pair[1].created_at
                    || (pair[0].created_at == pair[1].created_at
                        && pair[0].id.parse::<i64>().unwrap()
                            > pair[1].id.parse::<i64>().unwrap())),
            "server timestamp and ID tie order must survive catch-up"
        );
        assert!(!conversation.refreshing.contains_key(&channel));
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
