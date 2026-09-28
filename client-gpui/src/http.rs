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

impl AuthApi for HttpAuth {
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
            if response.status() == StatusCode::OK {
                let wire: WireLogin = response
                    .json()
                    .await
                    .map_err(|_| AuthError::InvalidResponse)?;
                if wire.access_token.is_empty()
                    || wire.user.id.is_empty()
                    || wire.user.username.is_empty()
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
            if response.status() == StatusCode::UNAUTHORIZED
                || response.status() == StatusCode::BAD_REQUEST
            {
                let status = response.status();
                let body: WireError = response
                    .json()
                    .await
                    .map_err(|_| AuthError::InvalidResponse)?;
                return match (status, body.error.code.as_str()) {
                    (StatusCode::UNAUTHORIZED, "unauthorized") => {
                        Err(AuthError::InvalidCredentials)
                    }
                    (StatusCode::BAD_REQUEST, "bad_request") => Err(AuthError::InvalidInput),
                    _ => Err(AuthError::InvalidResponse),
                };
            }
            Err(AuthError::Unavailable)
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
