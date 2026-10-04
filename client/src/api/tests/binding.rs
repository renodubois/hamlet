use super::test_support::{RequestAdapter, Response};
use super::tests::{response, server};
use super::*;
use reqwest::{Request, StatusCode};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

struct Controlled {
    requests: Mutex<Vec<Request>>,
    replies: Mutex<VecDeque<(StatusCode, &'static str)>>,
}

impl Controlled {
    fn new(replies: impl IntoIterator<Item = (StatusCode, &'static str)>) -> Arc<Self> {
        Arc::new(Self {
            requests: Mutex::new(vec![]),
            replies: Mutex::new(replies.into_iter().collect()),
        })
    }
}

impl RequestAdapter for Controlled {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        self.requests.lock().unwrap().push(request);
        let reply = self.replies.lock().unwrap().pop_front();
        Box::pin(async move {
            let (status, body) = reply.ok_or(ApiError::Unavailable)?;
            Ok(Response::controlled(status, body))
        })
    }
}

#[tokio::test]
async fn shared_wire_entities_still_pass_through_http_validation() {
    for body in [
        r#"{"id":"","channel_id":"c","author":{"id":"u","display_name":"Ada"},"text":"hi","created_at":"2026-01-01T00:00:00Z"}"#,
        r#"{"id":"m","channel_id":"wrong","author":{"id":"u","display_name":"Ada"},"text":"hi","created_at":"2026-01-01T00:00:00Z"}"#,
        r#"{"id":"m","channel_id":"c","author":{"id":"","display_name":"Ada"},"text":"hi","created_at":"2026-01-01T00:00:00Z"}"#,
        r#"{"id":"m","channel_id":"c","author":{"id":"u","display_name":""},"text":"hi","created_at":"2026-01-01T00:00:00Z"}"#,
        r#"{"id":"m","channel_id":"c","author":{"id":"u","display_name":"Ada"},"text":"hi","created_at":"invalid"}"#,
    ] {
        let adapter = Controlled::new([(StatusCode::CREATED, body)]);
        let client = HttpTransport::with_adapter(adapter)
            .server("https://example.com")
            .unwrap()
            .restore_candidate("synthetic".into())
            .unwrap();
        assert_eq!(
            client.send_message("c".into(), "hi".into()).await,
            Err(ApiError::InvalidResponse)
        );
    }
    for body in [
        r#"{"id":"","name":"general","type":"text"}"#,
        r#"{"id":"c","name":"","type":"text"}"#,
        r#"{"id":"c","name":"general","type":"voice"}"#,
    ] {
        let adapter = Controlled::new([(StatusCode::CREATED, body)]);
        let client = HttpTransport::with_adapter(adapter)
            .server("https://example.com")
            .unwrap()
            .restore_candidate("synthetic".into())
            .unwrap();
        assert_eq!(
            client.create_channel("general".into()).await,
            Err(ApiError::InvalidResponse)
        );
    }
    let adapter = Controlled::new([
        (
            StatusCode::CREATED,
            r#"{"id":"c","name":"general","type":"text","extra":true}"#,
        ),
        (
            StatusCode::CREATED,
            r#"{"id":"m","channel_id":"c","author":{"id":"u","display_name":"Ada","extra":true},"text":"世界\nhi","created_at":"2026-01-01T01:00:00+01:00","extra":true}"#,
        ),
    ]);
    let client = HttpTransport::with_adapter(adapter)
        .server("https://example.com")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    assert_eq!(
        client.create_channel("general".into()).await.unwrap().id,
        "c"
    );
    let message = client
        .send_message("c".into(), "世界\nhi".into())
        .await
        .unwrap();
    assert_eq!(message.text, "世界\nhi");
    assert_eq!(message.created_at, "2026-01-01T00:00:00+00:00");
}

#[tokio::test]
async fn bound_contexts_keep_origins_and_credentials_across_clones_and_new_logins() {
    let adapter = Controlled::new([
        (
            StatusCode::OK,
            r#"{"user":{"id":"1","username":"Ada"},"access_token":"alpha","expires_at":"2099-01-01T00:00:00Z"}"#,
        ),
        (
            StatusCode::CREATED,
            r#"{"user":{"id":"2","username":"Bob"},"access_token":"beta","expires_at":"2099-01-01T00:00:00Z"}"#,
        ),
        (
            StatusCode::OK,
            r#"{"user":{"id":"2","username":"Bob"},"access_token":"beta","expires_at":"2099-01-01T00:00:00Z"}"#,
        ),
        (StatusCode::OK, r#"{"id":"1","username":"Ada"}"#),
        (StatusCode::OK, r#"{"items":[]}"#),
        (StatusCode::OK, r#"{"items":[]}"#),
        (StatusCode::NO_CONTENT, ""),
        (StatusCode::OK, r#"{"id":"1","username":"Ada"}"#),
    ]);
    let transport = HttpTransport::with_adapter(adapter.clone());
    let a = transport.server("https://one.example:8443").unwrap();
    let b = transport.server("http://127.0.0.1:4321").unwrap();
    let old = a.login("Ada".into(), "old-password".into()).await.unwrap();
    let cloned = old.client.clone();
    let new = a.signup("Bob".into(), "new-password".into()).await.unwrap();
    let other = b
        .login("Bob".into(), "other-password".into())
        .await
        .unwrap();
    assert_eq!(cloned.current_user().await.unwrap(), old.user);
    new.client.channels().await.unwrap();
    other.client.channels().await.unwrap();
    old.client.logout().await.unwrap();
    // Revocation is a request, not mutable/global credential replacement. Old handles
    // remain bound; workflow generations (not this transport) decide whether to use them.
    assert_eq!(cloned.current_user().await.unwrap(), old.user);
    assert!(!format!("{old:?}").contains("alpha"));
    let requests = adapter.requests.lock().unwrap();
    let observed: Vec<_> = requests
        .iter()
        .map(|request| {
            (
                request.url().as_str(),
                request
                    .headers()
                    .get("authorization")
                    .map(|value| value.to_str().unwrap()),
            )
        })
        .collect();
    assert_eq!(
        observed,
        [
            ("https://one.example:8443/api/v1/auth/login", None),
            ("https://one.example:8443/api/v1/auth/signup", None),
            ("http://127.0.0.1:4321/api/v1/auth/login", None),
            ("https://one.example:8443/api/v1/me", Some("Bearer alpha")),
            (
                "https://one.example:8443/api/v1/channels",
                Some("Bearer beta")
            ),
            ("http://127.0.0.1:4321/api/v1/channels", Some("Bearer beta")),
            (
                "https://one.example:8443/api/v1/auth/logout",
                Some("Bearer alpha")
            ),
            ("https://one.example:8443/api/v1/me", Some("Bearer alpha")),
        ]
    );
}

#[actix_web::test]
async fn loopback_bound_operations_keep_headers_bodies_and_opaque_cursors() {
    use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
    use std::net::TcpListener;
    let first = TcpListener::bind("127.0.0.1:0").unwrap();
    let second = TcpListener::bind("127.0.0.1:0").unwrap();
    let host_a = first.local_addr().unwrap().to_string();
    let host_b = second.local_addr().unwrap().to_string();
    let requests = Arc::new(Mutex::new(vec![]));
    let replies = Arc::new(Mutex::new(VecDeque::from([
        (
            200,
            r#"{"user":{"id":"1","username":"Ada"},"access_token":"alpha","expires_at":"2099-01-01T00:00:00Z"}"#,
        ),
        (
            201,
            r#"{"user":{"id":"2","username":"Bob"},"access_token":"beta","expires_at":"2099-01-01T00:00:00Z"}"#,
        ),
        (
            200,
            r#"{"user":{"id":"2","username":"Bob"},"access_token":"beta","expires_at":"2099-01-01T00:00:00Z"}"#,
        ),
        (200, r#"{"id":"1","username":"Ada"}"#),
        (
            200,
            r#"{"items":[{"id":"c","name":"general","type":"text"}]}"#,
        ),
        (201, r#"{"id":"d","name":"new room","type":"text"}"#),
        (200, r#"{"items":[],"next_cursor":"opaque+/=&?"}"#),
        (200, r#"{"items":[],"next_cursor":null}"#),
        (
            201,
            r#"{"id":"m","channel_id":"c","author":{"id":"1","display_name":"Ada"},"text":"first\nsecond","created_at":"2026-01-01T00:00:00Z"}"#,
        ),
        (204, ""),
        (200, r#"{"items":[],"next_cursor":null}"#),
    ])));
    let http = HttpServer::new({
        let requests = requests.clone();
        move || {
            let requests = requests.clone();
            let replies = replies.clone();
            App::new().default_service(web::to(move |request: HttpRequest, body: web::Bytes| {
                let requests = requests.clone();
                let replies = replies.clone();
                async move {
                    let header = |name| {
                        request
                            .headers()
                            .get(name)
                            .map(|value| value.to_str().unwrap().to_owned())
                    };
                    requests.lock().unwrap().push((
                        request.method().to_string(),
                        header("host").unwrap(),
                        request.uri().to_string(),
                        header("authorization"),
                        body.to_vec(),
                    ));
                    let (status, body) = replies.lock().unwrap().pop_front().unwrap();
                    HttpResponse::build(actix_web::http::StatusCode::from_u16(status).unwrap())
                        .content_type("application/json")
                        .body(body)
                }
            }))
        }
    })
    .listen(first)
    .unwrap()
    .listen(second)
    .unwrap()
    .run();
    let handle = http.handle();
    actix_web::rt::spawn(http);
    let transport = HttpTransport::new();
    let a = transport.server(&format!("http://{host_a}")).unwrap();
    let b = transport.server(&format!("http://{host_b}")).unwrap();
    let old = a.login("Ada".into(), "old-password".into()).await.unwrap();
    let clone = old.client.clone();
    let other = b
        .signup("Bob".into(), "other-password".into())
        .await
        .unwrap();
    let newer = a.login("Bob".into(), "new-password".into()).await.unwrap();
    assert_eq!(clone.current_user().await.unwrap(), old.user);
    assert_eq!(other.client.channels().await.unwrap()[0].name, "general");
    assert_eq!(
        newer
            .client
            .create_channel("new room".into())
            .await
            .unwrap()
            .id,
        "d"
    );
    let page = old.client.history_page("c".into(), None).await.unwrap();
    other
        .client
        .history_page("c".into(), page.next_cursor)
        .await
        .unwrap();
    assert_eq!(
        clone
            .send_message("c".into(), "first\nsecond".into())
            .await
            .unwrap()
            .text,
        "first\nsecond"
    );
    old.client.logout().await.unwrap();
    assert!(
        clone
            .history_page("c".into(), None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    let captured = requests.lock().unwrap().clone();
    let observed: Vec<_> = captured
        .iter()
        .map(|(method, host, path, bearer, _)| {
            (
                method.as_str(),
                host.as_str(),
                path.as_str(),
                bearer.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        observed,
        [
            ("POST", host_a.as_str(), "/api/v1/auth/login", None),
            ("POST", host_b.as_str(), "/api/v1/auth/signup", None),
            ("POST", host_a.as_str(), "/api/v1/auth/login", None),
            ("GET", host_a.as_str(), "/api/v1/me", Some("Bearer alpha")),
            (
                "GET",
                host_b.as_str(),
                "/api/v1/channels",
                Some("Bearer beta")
            ),
            (
                "POST",
                host_a.as_str(),
                "/api/v1/channels",
                Some("Bearer beta")
            ),
            (
                "GET",
                host_a.as_str(),
                "/api/v1/channels/c/messages",
                Some("Bearer alpha")
            ),
            (
                "GET",
                host_b.as_str(),
                "/api/v1/channels/c/messages?before=opaque%2B%2F%3D%26%3F",
                Some("Bearer beta")
            ),
            (
                "POST",
                host_a.as_str(),
                "/api/v1/channels/c/messages",
                Some("Bearer alpha")
            ),
            (
                "POST",
                host_a.as_str(),
                "/api/v1/auth/logout",
                Some("Bearer alpha")
            ),
            (
                "GET",
                host_a.as_str(),
                "/api/v1/channels/c/messages",
                Some("Bearer alpha")
            ),
        ]
    );
    let body =
        |index: usize| serde_json::from_slice::<serde_json::Value>(&captured[index].4).unwrap();
    assert_eq!(
        body(0),
        serde_json::json!({"username":"Ada", "password":"old-password"})
    );
    assert_eq!(
        body(1),
        serde_json::json!({"username":"Bob", "password":"other-password"})
    );
    assert_eq!(
        body(5),
        serde_json::json!({"name":"new room", "type":"text"})
    );
    assert_eq!(body(8), serde_json::json!({"text":"first\nsecond"}));
    handle.stop(true).await;
}

#[tokio::test]
async fn controlled_adapter_cannot_bypass_binding_validation_or_invent_missing_outcomes() {
    let adapter = Controlled::new([]);
    let transport = HttpTransport::with_adapter(adapter.clone());
    for origin in [
        "http://example.com",
        "https://user:secret@example.com",
        "https://example.com/path",
        "https://example.com?query",
        "https://example.com#fragment",
    ] {
        assert!(matches!(
            transport.server(origin),
            Err(ApiError::InvalidResponse)
        ));
    }
    let server = transport.server("https://example.com").unwrap();
    assert!(matches!(
        server.restore_candidate(String::new()),
        Err(ApiError::InvalidResponse)
    ));
    let candidate = server.restore_candidate("synthetic-stored".into()).unwrap();
    for channel in ["", "..", ".", "../steal", "a/b", "a\\b"] {
        assert!(matches!(
            candidate.history_page(channel.into(), None).await,
            Err(ApiError::InvalidResponse)
        ));
        assert!(matches!(
            candidate.send_message(channel.into(), "text".into()).await,
            Err(ApiError::InvalidResponse)
        ));
    }
    assert!(adapter.requests.lock().unwrap().is_empty());
    // An omitted fixture response is an ordinary typed transport failure, never a default panic.
    assert_eq!(candidate.current_user().await, Err(ApiError::Unavailable));
    assert_eq!(
        candidate.send_message("c".into(), "text".into()).await,
        Err(ApiError::Unavailable)
    );
    let requests = adapter.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    for request in requests.iter() {
        assert_eq!(
            request.url().origin().ascii_serialization(),
            "https://example.com"
        );
        assert_eq!(
            request.headers()["authorization"],
            "Bearer synthetic-stored"
        );
    }
}

#[tokio::test]
async fn login_returns_public_identity_and_a_bound_client() {
    let (origin, login_request) = server(response(
        "200 OK",
        r#"{"user":{"id":"42","username":"Ada"},"access_token":"synthetic-old","expires_at":"2099-01-01T00:00:00Z"}"#,
    ));
    let server = HttpTransport::new().server(&origin).unwrap();
    let authenticated = server
        .login("Ada".into(), "synthetic-password".into())
        .await
        .unwrap();
    assert_eq!(authenticated.user.id, "42");
    assert_eq!(authenticated.user.username, "Ada");
    assert_eq!(authenticated.expires_at, 4070908800);
    assert_eq!(
        authenticated
            .client
            .server_url()
            .as_str()
            .trim_end_matches('/'),
        origin
    );
    let request = login_request.join().unwrap();
    assert!(request.starts_with("POST /api/v1/auth/login "));
    assert!(request.contains("\"password\":\"synthetic-password\""));
    assert!(!request.to_ascii_lowercase().contains("authorization:"));
}
