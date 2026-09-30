use super::ApiError;
use super::*;
use reqwest::{Client, StatusCode};
use std::time::Duration;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

// TCP fixtures exercise bound clients (not private decoding helpers).
pub(super) fn server(reply: impl Into<String>) -> (String, thread::JoinHandle<String>) {
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
pub(super) fn response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
#[actix_web::test]
async fn send_message_uses_real_server_validation_and_decodes_created_identity() {
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
    let auth = HttpTransport::new().server(&url).unwrap();
    let login = auth
        .signup("Ada".into(), "long password".into())
        .await
        .unwrap();
    let channel = login.client.channels().await.unwrap()[0].id.clone();
    let sent = login
        .client
        .send_message(channel.clone(), "first\nsecond".into())
        .await
        .unwrap();
    assert_eq!(sent.channel_id, channel);
    assert_eq!(sent.author_id, login.user.id);
    assert_eq!(sent.author_name, "Ada");
    assert_eq!(sent.text, "first\nsecond");
    assert!(!sent.id.is_empty());
    assert!(sent.created_at.contains('T'));
    assert_eq!(
        login
            .client
            .history_page(channel.clone(), None)
            .await
            .unwrap()
            .items,
        vec![sent]
    );
    for text in ["   ".to_owned(), "x".repeat(4001)] {
        assert!(matches!(
            login.client.send_message(channel.clone(), text).await,
            Err(ApiError::InvalidInput)
        ));
    }
    assert!(matches!(
        login
            .client
            .send_message("999999999999999".into(), "hi".into())
            .await,
        Err(ApiError::NotFound)
    ));
    assert!(matches!(
        auth.restore_candidate("bad".into())
            .unwrap()
            .send_message(channel, "hi".into())
            .await,
        Err(ApiError::AlreadyInvalid)
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
    let auth = HttpTransport::from_client(
        Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap(),
    )
    .server(&url)
    .unwrap()
    .restore_candidate("token".into())
    .unwrap();
    assert!(matches!(
        auth.send_message("123".into(), "do not retry".into()).await,
        Err(ApiError::Unavailable)
    ));
    let captured = captured.join().unwrap();
    assert!(captured.starts_with("POST /api/v1/channels/123/messages "));
    assert!(captured.contains("do not retry"));
    let (url, request) = server(response("201 Created", "{bad json"));
    assert!(matches!(
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .restore_candidate("token".into())
            .unwrap()
            .send_message("123".into(), "hi".into())
            .await,
        Err(ApiError::InvalidResponse)
    ));
    request.join().unwrap();
}

#[tokio::test]
async fn login_decodes_server_contract_and_unauthorized() {
    let body = r#"{"user":{"id":"42","username":"ada"},"access_token":"abc","expires_at":"2099-01-01T00:00:00Z"}"#;
    let reply = response("200 OK", body);
    let (url, captured) = server(reply);
    let login = HttpTransport::new()
        .server(&url)
        .unwrap()
        .login("ada".into(), "pass".into())
        .await
        .unwrap();
    assert_eq!(login.user.username, "ada");
    assert_eq!(login.client.credential_for_session(), "abc");
    let request = captured.join().unwrap();
    assert!(request.starts_with("POST /api/v1/auth/login "));
    assert!(request.contains("\"password\":\"pass\""));
    let reply = response(
        "401 Unauthorized",
        r#"{"error":{"code":"unauthorized","message":"Unauthorized"}}"#,
    );
    let (url, captured) = server(reply);
    assert!(matches!(
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .login("ada".into(), "bad".into())
            .await,
        Err(ApiError::InvalidCredentials)
    ));
    captured.join().unwrap();
}
#[tokio::test]
async fn current_user_requires_bearer_and_rejects_redirect_or_malformed_identity() {
    let (url, captured) = server(response("200 OK", r#"{"id":"42","username":"Ada"}"#));
    assert_eq!(
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .restore_candidate("secret".into())
            .unwrap()
            .current_user()
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
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .restore_candidate("bad".into())
            .unwrap()
            .current_user()
            .await,
        Err(ApiError::AlreadyInvalid)
    ));
    captured.join().unwrap();
    let (url, captured) = server(response("200 OK", r#"{"id":"","username":"Ada"}"#));
    assert!(matches!(
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .restore_candidate("secret".into())
            .unwrap()
            .current_user()
            .await,
        Err(ApiError::InvalidResponse)
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
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .restore_candidate("secret".into())
            .unwrap()
            .current_user()
            .await,
        Err(ApiError::Unavailable)
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
    let auth = HttpTransport::from_client(
        Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap(),
    );
    assert!(matches!(
        auth.server(&url)
            .unwrap()
            .restore_candidate("still-valid-until-verified".into())
            .unwrap()
            .current_user()
            .await,
        Err(ApiError::Unavailable)
    ));
    waiting.join().unwrap();
}

#[actix_web::test]
async fn current_user_against_server_routes_before_and_after_revocation() {
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
    let api = HttpTransport::new().server(&url).unwrap();
    let login = api
        .signup("Ada".into(), "long password".into())
        .await
        .unwrap();
    assert_eq!(login.client.current_user().await.unwrap(), login.user);
    assert!(matches!(
        api.restore_candidate("bad".into())
            .unwrap()
            .current_user()
            .await,
        Err(ApiError::AlreadyInvalid)
    ));
    login.client.logout().await.unwrap();
    assert!(matches!(
        login.client.current_user().await,
        Err(ApiError::AlreadyInvalid)
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
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .login("ada".into(), "pass".into())
            .await,
        Err(ApiError::InvalidResponse)
    ));
    captured.join().unwrap();
}

#[tokio::test]
async fn logout_sends_bearer_and_redirects_are_not_followed() {
    let (url, captured) =
        server("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    HttpTransport::new()
        .server(&url)
        .unwrap()
        .restore_candidate("secret".into())
        .unwrap()
        .logout()
        .await
        .unwrap();
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
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .restore_candidate("secret".into())
            .unwrap()
            .logout()
            .await,
        Err(ApiError::Unavailable)
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
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .login("ada".into(), "pass".into())
            .await,
        Err(ApiError::Unavailable)
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
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .login("ada".into(), "pass".into())
            .await,
        Err(ApiError::InvalidInput)
    ));
    captured.join().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let waiting = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_millis(150));
        drop(stream);
    });
    let auth = HttpTransport::from_client(
        Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(30))
            .build()
            .unwrap(),
    );
    let started = std::time::Instant::now();
    assert!(matches!(
        auth.server(&url)
            .unwrap()
            .restore_candidate("secret".into())
            .unwrap()
            .logout()
            .await,
        Err(ApiError::Unavailable)
    ));
    assert!(started.elapsed() < Duration::from_millis(130));
    waiting.join().unwrap();
}
// Exercise server routes over real HTTP using bound clients.
#[actix_web::test]
async fn server_routes_signup_login_logout_and_errors() {
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

    let auth = HttpTransport::new().server(&url).unwrap();
    let login = auth
        .login("aLiCe".into(), "long password".into())
        .await
        .unwrap();
    assert_eq!(login.user.username, "Alice");
    assert_eq!(login.user.id, bootstrap["user"]["id"].as_str().unwrap());
    assert_ne!(login.client.credential_for_session(), bootstrap_token);
    assert!(login.expires_at > chrono::Utc::now().timestamp());

    let channel = client
        .post(format!("{url}/api/v1/channels"))
        .bearer_auth(login.client.credential_for_session())
        .json(&serde_json::json!({"name":"alpha", "type":"text"}))
        .send()
        .await
        .unwrap();
    assert_eq!(channel.status(), StatusCode::CREATED);
    let channel: serde_json::Value = channel.json().await.unwrap();
    let channel_id = channel["id"].as_str().unwrap();
    let list = login.client.channels().await.unwrap();
    assert_eq!(
        list.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["alpha", "general"]
    );
    assert_eq!(list[0].id, channel_id);
    assert!(matches!(
        auth.restore_candidate("bad".into())
            .unwrap()
            .channels()
            .await,
        Err(ApiError::AlreadyInvalid)
    ));
    assert!(matches!(
        auth.restore_candidate("bad".into())
            .unwrap()
            .history_page(channel_id.into(), None)
            .await,
        Err(ApiError::AlreadyInvalid)
    ));
    let empty = login
        .client
        .history_page(channel_id.into(), None)
        .await
        .unwrap()
        .items;
    assert!(empty.is_empty());
    for text in ["first\nline", "second"] {
        let posted = client
            .post(format!("{url}/api/v1/channels/{channel_id}/messages"))
            .bearer_auth(login.client.credential_for_session())
            .json(&serde_json::json!({"text":text}))
            .send()
            .await
            .unwrap();
        assert_eq!(posted.status(), StatusCode::CREATED);
    }
    let newest = login
        .client
        .history_page(channel_id.into(), None)
        .await
        .unwrap()
        .items;
    assert_eq!(newest.len(), 2);
    assert_eq!(newest[0].text, "second");
    assert_eq!(newest[1].text, "first\nline");
    assert_eq!(newest[0].author_id, login.user.id);
    assert_eq!(newest[0].author_name, "Alice");
    assert_eq!(newest[0].channel_id, channel_id);
    assert_ne!(newest[0].id, newest[1].id);
    assert!(newest[0].created_at.contains('T'));
    assert!(matches!(
        login
            .client
            .history_page("999999999999999".into(), None)
            .await,
        Err(ApiError::NotFound)
    ));
    assert!(matches!(
        login.client.history_page("../bad".into(), None).await,
        Err(ApiError::InvalidResponse)
    ));
    assert!(matches!(
        auth.login("Alice".into(), "bad password".into()).await,
        Err(ApiError::InvalidCredentials)
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
        HttpTransport::new().server("http://example.com"),
        Err(ApiError::InvalidResponse)
    ));

    // The second token stays live; this distinguishes a protected bearer logout from a
    // misleading 204 on a public route or an invalid-token 401 interpreted as success.
    auth.restore_candidate(bootstrap_token.into())
        .unwrap()
        .logout()
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
            .bearer_auth(login.client.credential_for_session())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(matches!(
        auth.restore_candidate(bootstrap_token.into())
            .unwrap()
            .logout()
            .await,
        Err(ApiError::AlreadyInvalid)
    ));
    server_handle.stop(true).await;
}

#[actix_web::test]
async fn signup_against_server_routes_returns_usable_session_and_rejections() {
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
    let auth = HttpTransport::new().server(&url).unwrap();
    let created = auth
        .signup("Alice_1".into(), "long password".into())
        .await
        .unwrap();
    assert_eq!(created.user.username, "Alice_1");
    assert!(!created.user.id.is_empty());
    assert!(created.expires_at > chrono::Utc::now().timestamp());
    let user = created.client.current_user().await.unwrap();
    assert_eq!(user.id, created.user.id);
    assert!(matches!(
        auth.signup("aLiCe_1".into(), "another password".into())
            .await,
        Err(ApiError::Conflict)
    ));
    assert!(matches!(
        auth.signup("bad!".into(), "long password".into()).await,
        Err(ApiError::InvalidInput)
    ));
    assert!(matches!(
        auth.signup("NewUser".into(), "short".into()).await,
        Err(ApiError::InvalidInput)
    ));
    handle.stop(true).await;
}

#[actix_web::test]
async fn create_channel_against_server_routes() {
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
    let auth = HttpTransport::new().server(&url).unwrap();
    let login = auth
        .signup("Alice".into(), "long password".into())
        .await
        .unwrap();
    let channel = login
        .client
        .create_channel("  New Room  ".into())
        .await
        .unwrap();
    assert_eq!(channel.name, "New Room");
    assert!(!channel.id.is_empty());
    assert!(login.client.channels().await.unwrap().contains(&channel));
    assert!(matches!(
        login.client.create_channel("new room".into()).await,
        Err(ApiError::Conflict)
    ));
    assert!(matches!(
        login.client.create_channel("bad!".into()).await,
        Err(ApiError::InvalidInput)
    ));
    assert!(matches!(
        auth.restore_candidate("bad".into())
            .unwrap()
            .create_channel("Other".into())
            .await,
        Err(ApiError::AlreadyInvalid)
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
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .restore_candidate("secret".into())
            .unwrap()
            .create_channel("  Trimmed  ".into())
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
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .restore_candidate("secret".into())
            .unwrap()
            .create_channel("A".into())
            .await,
        Err(ApiError::InvalidResponse)
    ));
    captured.join().unwrap();
    let (url, captured) = server(response("409 Conflict", r#"{"error":{"code":"conflict"}}"#));
    assert!(matches!(
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .restore_candidate("secret".into())
            .unwrap()
            .create_channel("A".into())
            .await,
        Err(ApiError::Conflict)
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
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .signup("Alice".into(), "long password".into())
            .await,
        Err(ApiError::Unavailable)
    ));
    let request = captured.join().unwrap();
    assert!(request.starts_with("POST /api/v1/auth/signup "));
    assert!(destination.accept().is_err());
    assert!(matches!(
        HttpTransport::new().server("http://example.com"),
        Err(ApiError::InvalidResponse)
    ));
    let (url, captured) = server(response(
        "200 OK",
        r#"{"user":{"id":"42","username":"Alice"},"access_token":"abc","expires_at":"2099-01-01T00:00:00Z"}"#,
    ));
    assert!(matches!(
        HttpTransport::new()
            .server(&url)
            .unwrap()
            .signup("Alice".into(), "long password".into())
            .await,
        Err(ApiError::Unavailable)
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
