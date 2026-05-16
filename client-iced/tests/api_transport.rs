#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use hamlet_client_iced::api::{ApiClient, ApiError, HttpApi};
use hamlet_client_iced::protocol::ChannelKind;
use serde_json::{Value, json};

#[test]
fn login_stores_session_cookie_and_reuses_it_for_me() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![
        ResponseSpec::json(200, user_json("alice")).with_header(
            "Set-Cookie",
            "session=abc123; Path=/; HttpOnly; SameSite=Lax",
        ),
        ResponseSpec::json(200, user_json("alice")),
    ])?;
    let api = HttpApi::new("http://unused.example.test")?;

    api.set_base_url(server.base_url())?;
    let session = api.login("alice".to_string(), "secret".to_string())?;
    let me = api.get_me()?;

    assert_eq!(session.user.username, "alice");
    assert_eq!(session.session_token.as_deref(), Some("abc123"));
    assert_eq!(me, session.user);

    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/login");
    assert_json_field(&requests[0].body, "username", "alice")?;
    assert_json_field(&requests[0].body, "password", "secret")?;
    assert_eq!(requests[1].method, "GET");
    assert_eq!(requests[1].path, "/me");
    assert!(
        requests[1]
            .header("cookie")
            .unwrap()
            .contains("session=abc123")
    );

    Ok(())
}

#[test]
fn register_sends_server_dto_to_configured_base_url() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::json(200, user_json("new-user"))])?;
    let api = HttpApi::new(server.base_url())?;

    let session = api.register(
        "new-user".to_string(),
        "secret".to_string(),
        Some("new@example.test".to_string()),
    )?;

    assert_eq!(session.user.username, "new-user");

    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/register");
    assert_json_field(&requests[0].body, "username", "new-user")?;
    assert_json_field(&requests[0].body, "password", "secret")?;
    assert_json_field(&requests[0].body, "email", "new@example.test")?;

    Ok(())
}

#[test]
fn auth_error_responses_map_to_distinct_api_errors() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (
            401,
            "invalid_credentials",
            "invalid credentials",
            ApiError::InvalidCredentials,
        ),
        (
            409,
            "username_taken",
            "username already taken",
            ApiError::UsernameTaken,
        ),
        (
            400,
            "invalid_request",
            "invalid request",
            ApiError::InvalidRequest("invalid request".to_string()),
        ),
    ];

    for (status, kind, message, expected) in cases {
        let server = TestServer::start(vec![ResponseSpec::json(
            status,
            json!({ "error": { "kind": kind, "message": message } }),
        )])?;
        let api = HttpApi::new(server.base_url())?;

        let error = api
            .login("alice".to_string(), "wrong".to_string())
            .expect_err("login should fail");

        assert_eq!(error, expected);
    }

    Ok(())
}

#[test]
fn list_channels_decodes_text_and_voice_dtos() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::json(
        200,
        json!([
            { "id": 10, "name": "general", "position": 0, "type": "text" },
            { "id": 11, "name": "voice", "position": 1, "type": "voice" }
        ]),
    )])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("channels-token".to_string()))?;
    let channels = api.list_channels()?;

    assert_eq!(channels.len(), 2);
    assert_eq!(channels[0].kind, ChannelKind::Text);
    assert_eq!(channels[1].kind, ChannelKind::Voice);
    let requests = server.requests();
    assert_eq!(requests[0].path, "/channels");
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=channels-token")
    );

    Ok(())
}

#[test]
fn list_voice_participants_decodes_participant_dtos() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::json(
        200,
        json!([
            { "user_id": 42, "channel_id": 11, "username": "alice", "avatar_url": null },
            { "user_id": 43, "channel_id": 11, "username": "bob", "avatar_url": "/avatars/bob.png" }
        ]),
    )])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("voice-token".to_string()))?;
    let participants = api.list_voice_participants(11)?;

    assert_eq!(participants.len(), 2);
    assert_eq!(participants[0].username, "alice");
    assert_eq!(
        participants[1].avatar_url.as_deref(),
        Some("/avatars/bob.png")
    );
    let requests = server.requests();
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, "/voice/participants/11");
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=voice-token")
    );

    Ok(())
}

#[test]
fn get_voice_token_posts_to_channel_and_decodes_livekit_room()
-> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::json(
        200,
        json!({ "url": "ws://localhost:7880", "token": "jwt", "room": "channel-11" }),
    )])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("voice-token".to_string()))?;
    let token = api.get_voice_token(11)?;

    assert_eq!(token.url, "ws://localhost:7880");
    assert_eq!(token.token, "jwt");
    assert_eq!(token.room, "channel-11");
    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/voice/token/11");
    assert!(requests[0].body.is_empty());
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=voice-token")
    );

    Ok(())
}

#[test]
fn post_voice_speaking_sends_authenticated_state() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::empty(204)])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("voice-token".to_string()))?;
    api.post_voice_speaking(11, true)?;

    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/voice/speaking");
    let body: Value = serde_json::from_str(&requests[0].body)?;
    assert_eq!(body["channel_id"], 11);
    assert_eq!(body["speaking"], true);
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=voice-token")
    );

    Ok(())
}

#[test]
fn create_channel_posts_name_and_kind_to_server() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![
        ResponseSpec::json(
            200,
            json!({ "id": 12, "name": "native-text", "position": 2, "type": "text" }),
        ),
        ResponseSpec::json(
            200,
            json!({ "id": 13, "name": "native-voice", "position": 3, "type": "voice" }),
        ),
    ])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("create-token".to_string()))?;
    let text_channel = api.create_channel("native-text".to_string(), ChannelKind::Text)?;
    let voice_channel = api.create_channel("native-voice".to_string(), ChannelKind::Voice)?;

    assert_eq!(text_channel.kind, ChannelKind::Text);
    assert_eq!(voice_channel.kind, ChannelKind::Voice);
    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/channel");
    assert_json_field(&requests[0].body, "name", "native-text")?;
    assert_json_field(&requests[0].body, "type", "text")?;
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=create-token")
    );
    assert_eq!(requests[1].method, "POST");
    assert_eq!(requests[1].path, "/channel");
    assert_json_field(&requests[1].body, "name", "native-voice")?;
    assert_json_field(&requests[1].body, "type", "voice")?;

    Ok(())
}

#[test]
fn reorder_channels_puts_full_id_order_and_decodes_server_order()
-> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::json(
        200,
        json!([
            { "id": 11, "name": "voice", "position": 0, "type": "voice" },
            { "id": 10, "name": "general", "position": 1, "type": "text" }
        ]),
    )])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("reorder-token".to_string()))?;
    let channels = api.reorder_channels(vec![11, 10])?;

    assert_eq!(channels.len(), 2);
    assert_eq!(channels[0].id, 11);
    assert_eq!(channels[0].position, 0);
    let requests = server.requests();
    assert_eq!(requests[0].method, "PUT");
    assert_eq!(requests[0].path, "/channels/order");
    assert_eq!(
        serde_json::from_str::<Value>(&requests[0].body)?["ids"],
        json!([11, 10])
    );
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=reorder-token")
    );

    Ok(())
}

#[test]
fn get_messages_decodes_history_for_channel() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::json(
        200,
        json!([
            {
                "id": 99,
                "user_id": 42,
                "channel_id": 10,
                "text": "hello",
                "username": "alice",
                "display_name": null,
                "avatar_url": null,
                "suppress_embeds": false,
                "embeds": [{
                    "id": 501,
                    "message_id": 99,
                    "url": "https://example.test/article",
                    "title": "Example article",
                    "description": "A native preview",
                    "image_url": "https://cdn.example.test/preview.jpg",
                    "site_name": "Example",
                    "embed_type": "link",
                    "iframe_url": null,
                    "iframe_width": null,
                    "iframe_height": null
                }]
            }
        ]),
    )])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("messages-token".to_string()))?;
    let messages = api.get_messages(10)?;

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text, "hello");
    assert_eq!(messages[0].embeds.len(), 1);
    assert_eq!(messages[0].embeds[0].site_name.as_deref(), Some("Example"));
    assert_eq!(
        messages[0].embeds[0].image_url.as_deref(),
        Some("https://cdn.example.test/preview.jpg")
    );
    let requests = server.requests();
    assert_eq!(requests[0].path, "/messages/10");
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=messages-token")
    );

    Ok(())
}

#[test]
fn send_message_posts_text_to_selected_channel() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::json(
        200,
        json!({
            "id": 123,
            "user_id": 42,
            "channel_id": 10,
            "text": "hello from native",
            "username": "alice",
            "display_name": null,
            "avatar_url": null,
            "suppress_embeds": false,
            "embeds": []
        }),
    )])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("send-token".to_string()))?;
    let message = api.send_message(10, "hello from native".to_string())?;

    assert_eq!(message.text, "hello from native");
    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/message/10");
    assert_json_field(&requests[0].body, "text", "hello from native")?;
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=send-token")
    );

    Ok(())
}

#[test]
fn post_typing_pings_channel_without_body() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::empty(204)])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("typing-token".to_string()))?;
    api.post_typing(10)?;

    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/typing/10");
    assert!(requests[0].body.is_empty());
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=typing-token")
    );

    Ok(())
}

#[test]
fn edit_message_puts_text_to_message_id() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::json(
        200,
        json!({
            "id": 123,
            "user_id": 42,
            "channel_id": 10,
            "text": "fixed typo",
            "username": "alice",
            "display_name": null,
            "avatar_url": null,
            "suppress_embeds": false,
            "embeds": []
        }),
    )])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("edit-token".to_string()))?;
    let message = api.edit_message(123, "fixed typo".to_string())?;

    assert_eq!(message.text, "fixed typo");
    let requests = server.requests();
    assert_eq!(requests[0].method, "PUT");
    assert_eq!(requests[0].path, "/message/123");
    assert_json_field(&requests[0].body, "text", "fixed typo")?;
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=edit-token")
    );

    Ok(())
}

#[test]
fn suppress_message_embeds_posts_request_and_decodes_update()
-> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::json(
        200,
        json!({
            "id": 123,
            "channel_id": 10,
            "suppress_embeds": true,
            "embeds": [{
                "id": 501,
                "message_id": 123,
                "url": "https://example.test/article",
                "title": "Example article",
                "description": "A native preview",
                "image_url": "https://cdn.example.test/preview.jpg",
                "site_name": "Example",
                "embed_type": "link",
                "iframe_url": null,
                "iframe_width": null,
                "iframe_height": null
            }]
        }),
    )])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("suppress-token".to_string()))?;
    let update = api.suppress_message_embeds(123, true)?;

    assert_eq!(update.id, 123);
    assert!(update.suppress_embeds);
    assert_eq!(update.embeds.len(), 1);
    assert_eq!(
        update.embeds[0].image_url.as_deref(),
        Some("https://cdn.example.test/preview.jpg")
    );
    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/message/123/suppress_embeds");
    let body: Value = serde_json::from_str(&requests[0].body)?;
    assert_eq!(body["suppress"], true);
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=suppress-token")
    );

    Ok(())
}

#[test]
fn delete_message_sends_delete_to_message_id() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::empty(204)])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("delete-token".to_string()))?;
    api.delete_message(123)?;

    let requests = server.requests();
    assert_eq!(requests[0].method, "DELETE");
    assert_eq!(requests[0].path, "/message/123");
    assert!(requests[0].body.is_empty());
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=delete-token")
    );

    Ok(())
}

#[test]
fn update_profile_puts_display_name_and_clear_to_me() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![
        ResponseSpec::json(200, user_json_with_display("alice", Some("Alice"))),
        ResponseSpec::json(200, user_json_with_display("alice", None)),
    ])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("profile-token".to_string()))?;
    let updated = api.update_profile(Some("Alice".to_string()))?;
    let cleared = api.update_profile(None)?;

    assert_eq!(updated.display_name.as_deref(), Some("Alice"));
    assert_eq!(cleared.display_name, None);

    let requests = server.requests();
    assert_eq!(requests[0].method, "PUT");
    assert_eq!(requests[0].path, "/me");
    assert_json_field(&requests[0].body, "display_name", "Alice")?;
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=profile-token")
    );
    assert_eq!(requests[1].method, "PUT");
    assert_eq!(requests[1].path, "/me");
    let clear_body: Value = serde_json::from_str(&requests[1].body)?;
    assert!(clear_body["display_name"].is_null());

    Ok(())
}

#[test]
fn upload_avatar_posts_multipart_file() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::json(
        200,
        user_json_with_avatar("alice", Some("/uploads/avatars/42.webp?v=1")),
    )])?;
    let api = HttpApi::new(server.base_url())?;
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(b"avatar-bytes")?;

    api.set_session_token(Some("avatar-token".to_string()))?;
    let user = api.upload_avatar(file.path().to_path_buf())?;

    assert_eq!(
        user.avatar_url.as_deref(),
        Some("/uploads/avatars/42.webp?v=1")
    );
    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/me/avatar");
    assert!(
        requests[0]
            .header("content-type")
            .unwrap_or_default()
            .starts_with("multipart/form-data; boundary=")
    );
    assert!(requests[0].body.contains("name=\"file\""));
    assert!(requests[0].body.contains("avatar-bytes"));
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=avatar-token")
    );

    Ok(())
}

#[test]
fn delete_avatar_sends_delete_to_me_avatar() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::json(
        200,
        user_json_with_avatar("alice", None),
    )])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("avatar-delete-token".to_string()))?;
    let user = api.delete_avatar()?;

    assert_eq!(user.avatar_url, None);
    let requests = server.requests();
    assert_eq!(requests[0].method, "DELETE");
    assert_eq!(requests[0].path, "/me/avatar");
    assert!(requests[0].body.is_empty());
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=avatar-delete-token")
    );

    Ok(())
}

#[test]
fn stored_session_token_is_sent_to_me() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::json(200, user_json("alice"))])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("stored-token".to_string()))?;
    let user = api.get_me()?;

    assert_eq!(user.username, "alice");
    let requests = server.requests();
    assert_eq!(requests[0].path, "/me");
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=stored-token")
    );

    Ok(())
}

#[test]
fn bare_unauthorized_response_maps_to_unauthorized() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::empty(401)])?;
    let api = HttpApi::new(server.base_url())?;

    let error = api.get_me().expect_err("me should fail");

    assert_eq!(error, ApiError::Unauthorized);

    Ok(())
}

#[test]
fn logout_sends_cookie_and_clears_local_cookie() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(vec![ResponseSpec::empty(200), ResponseSpec::empty(401)])?;
    let api = HttpApi::new(server.base_url())?;

    api.set_session_token(Some("logout-token".to_string()))?;
    api.logout()?;
    let error = api.get_me().expect_err("me should be unauthorized");

    assert_eq!(error, ApiError::Unauthorized);
    let requests = server.requests();
    assert_eq!(requests[0].path, "/logout");
    assert!(
        requests[0]
            .header("cookie")
            .unwrap()
            .contains("session=logout-token")
    );
    assert!(requests[1].header("cookie").is_none());

    Ok(())
}

#[test]
fn unreachable_server_maps_to_connectivity_error() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let server_url = format!("http://{}", listener.local_addr()?);
    drop(listener);
    let api = HttpApi::new(server_url.clone())?;

    let error = api
        .login("alice".to_string(), "secret".to_string())
        .expect_err("login should fail");

    assert!(matches!(
        error,
        ApiError::Unreachable { server_url: url, .. } if url == server_url
    ));

    Ok(())
}

#[derive(Debug, Clone)]
struct RecordedRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl RecordedRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone)]
struct ResponseSpec {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl ResponseSpec {
    fn empty(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: String::new(),
        }
    }

    fn json(status: u16, value: Value) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: value.to_string(),
        }
    }

    fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

struct TestServer {
    base_url: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    handle: Option<JoinHandle<()>>,
}

impl TestServer {
    fn start(responses: Vec<ResponseSpec>) -> Result<Self, Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let base_url = format!("http://{}", listener.local_addr()?);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_requests = Arc::clone(&requests);
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().expect("test server accepts request");
                let request = read_request(&mut stream);
                thread_requests.lock().unwrap().push(request);
                write_response(&mut stream, &response);
            }
        });

        Ok(Self {
            base_url,
            requests,
            handle: Some(handle),
        })
    }

    fn base_url(&self) -> String {
        self.base_url.clone()
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        if let Some(handle) = &self.handle {
            while !handle.is_finished() {
                thread::yield_now();
            }
        }

        self.requests.lock().unwrap().clone()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().expect("test server thread should finish");
        }
    }
}

fn read_request(stream: &mut TcpStream) -> RecordedRequest {
    let mut reader = BufReader::new(stream.try_clone().expect("clone test stream"));
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .expect("read request line");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut headers = Vec::new();

    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read header line");
        let trimmed = line.trim_end_matches(['\r', '\n']);

        if trimmed.is_empty() {
            break;
        }

        if let Some((name, value)) = trimmed.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }

    let content_length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body_bytes = vec![0; content_length];

    if content_length > 0 {
        reader
            .read_exact(&mut body_bytes)
            .expect("read request body");
    }

    RecordedRequest {
        method,
        path,
        headers,
        body: String::from_utf8(body_bytes).expect("request body is UTF-8"),
    }
}

fn write_response(stream: &mut TcpStream, response: &ResponseSpec) {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        409 => "Conflict",
        _ => "Error",
    };
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        reason,
        response.body.len()
    );

    for (name, value) in &response.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }

    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .expect("write response head");
    stream
        .write_all(response.body.as_bytes())
        .expect("write response body");
}

fn user_json(username: &str) -> Value {
    user_json_with_display(username, None)
}

fn user_json_with_display(username: &str, display_name: Option<&str>) -> Value {
    json!({
        "id": 42,
        "username": username,
        "display_name": display_name,
        "email": null,
        "email_verified": false,
        "avatar_url": null
    })
}

fn user_json_with_avatar(username: &str, avatar_url: Option<&str>) -> Value {
    json!({
        "id": 42,
        "username": username,
        "display_name": null,
        "email": null,
        "email_verified": false,
        "avatar_url": avatar_url
    })
}

fn assert_json_field(
    body: &str,
    field: &str,
    expected: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let body: Value = serde_json::from_str(body)?;

    assert_eq!(body[field], expected);

    Ok(())
}
