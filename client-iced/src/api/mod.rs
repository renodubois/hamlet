use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use reqwest::StatusCode;
use reqwest::blocking::{Client, Response, multipart};
use reqwest::cookie::Jar;
use reqwest::header::SET_COOKIE;
use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;
use url::Url;

use crate::auth::AuthSession;
use crate::protocol::{
    Channel, ChannelKind, CreateChannelRequest, ErrorEnvelope, Id, LoginRequest, Message,
    MessageEmbedsUpdatedEvent, RegisterRequest, ReorderChannelsRequest, SendMessageRequest,
    SuppressEmbedsRequest, UpdateProfileRequest, User, VoiceParticipant, VoiceSpeakingRequest,
    VoiceToken,
};
use crate::storage::{DEFAULT_SERVER_URL, Preferences};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApiError {
    #[error("invalid server URL: {0}")]
    InvalidServerUrl(String),
    #[error("could not initialize HTTP transport: {0}")]
    TransportSetup(String),
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("username is already taken")]
    UsernameTaken,
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("could not reach {server_url}: {message}")]
    Unreachable { server_url: String, message: String },
    #[error("server returned {status}: {message}")]
    Server {
        status: u16,
        kind: Option<String>,
        message: String,
    },
    #[error("could not decode server response: {0}")]
    Decode(String),
    #[error("fake API failure: {0}")]
    Fake(String),
}

impl ApiError {
    pub fn user_message(&self) -> String {
        match self {
            Self::InvalidServerUrl(message) => {
                format!("Enter a valid Hamlet server URL before continuing. {message}")
            }
            Self::TransportSetup(_) => {
                "The native HTTP transport could not be initialized.".to_string()
            }
            Self::InvalidCredentials => "Invalid username or password.".to_string(),
            Self::UsernameTaken => "That username is already taken.".to_string(),
            Self::InvalidRequest(_) => {
                "Invalid input. Username and password are required.".to_string()
            }
            Self::Unauthorized => {
                "Your session is not authorized. Please log in again.".to_string()
            }
            Self::Unreachable { server_url, .. } => {
                format!("Could not reach the Hamlet server at {server_url}.")
            }
            Self::Server {
                status, message, ..
            } => {
                format!("Server returned {status}: {message}")
            }
            Self::Decode(_) => "The server response was not in the expected format.".to_string(),
            Self::Fake(message) => message.clone(),
        }
    }
}

pub trait ApiClient {
    fn base_url(&self) -> String;
    fn set_base_url(&self, server_url: String) -> Result<(), ApiError>;
    fn set_session_token(&self, session_token: Option<String>) -> Result<(), ApiError>;
    fn login(&self, username: String, password: String) -> Result<AuthSession, ApiError>;
    fn register(
        &self,
        username: String,
        password: String,
        email: Option<String>,
    ) -> Result<AuthSession, ApiError>;
    fn get_me(&self) -> Result<User, ApiError>;
    fn logout(&self) -> Result<(), ApiError>;
    fn update_profile(&self, display_name: Option<String>) -> Result<User, ApiError>;
    fn upload_avatar(&self, path: PathBuf) -> Result<User, ApiError>;
    fn delete_avatar(&self) -> Result<User, ApiError>;
    fn list_channels(&self) -> Result<Vec<Channel>, ApiError>;
    fn create_channel(&self, name: String, kind: ChannelKind) -> Result<Channel, ApiError>;
    fn reorder_channels(&self, ids: Vec<Id>) -> Result<Vec<Channel>, ApiError>;
    fn get_messages(&self, channel_id: Id) -> Result<Vec<Message>, ApiError>;
    fn send_message(&self, channel_id: Id, text: String) -> Result<Message, ApiError>;
    fn post_typing(&self, channel_id: Id) -> Result<(), ApiError>;
    fn edit_message(&self, message_id: Id, text: String) -> Result<Message, ApiError>;
    fn delete_message(&self, message_id: Id) -> Result<(), ApiError>;
    fn suppress_message_embeds(
        &self,
        message_id: Id,
        suppress: bool,
    ) -> Result<MessageEmbedsUpdatedEvent, ApiError>;
    fn list_voice_participants(&self, channel_id: Id) -> Result<Vec<VoiceParticipant>, ApiError>;
    fn get_voice_token(&self, channel_id: Id) -> Result<VoiceToken, ApiError>;
    fn post_voice_speaking(&self, channel_id: Id, speaking: bool) -> Result<(), ApiError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiCall {
    SetBaseUrl(String),
    SetSessionToken { present: bool },
    Login { username: String },
    Register { username: String },
    Logout,
    GetMe,
    UpdateProfile { display_name: Option<String> },
    UploadAvatar { path: PathBuf },
    DeleteAvatar,
    ListChannels,
    CreateChannel { name: String, kind: ChannelKind },
    ReorderChannels { ids: Vec<Id> },
    GetMessages { channel_id: Id },
    SendMessage { channel_id: Id, text: String },
    PostTyping { channel_id: Id },
    EditMessage { message_id: Id, text: String },
    DeleteMessage { message_id: Id },
    SuppressMessageEmbeds { message_id: Id, suppress: bool },
    ListVoiceParticipants { channel_id: Id },
    GetVoiceToken { channel_id: Id },
    PostVoiceSpeaking { channel_id: Id, speaking: bool },
}

#[derive(Debug, Clone)]
pub struct HttpApi {
    inner: Arc<Mutex<HttpApiInner>>,
}

#[derive(Debug, Clone)]
struct HttpApiInner {
    base_url: String,
    client: Client,
    jar: Arc<Jar>,
}

impl HttpApi {
    pub fn new(server_url: impl Into<String>) -> Result<Self, ApiError> {
        let preferences = normalize_api_server_url(server_url.into())?;
        let (client, jar) = build_client()?;

        Ok(Self {
            inner: Arc::new(Mutex::new(HttpApiInner {
                base_url: preferences.server_url,
                client,
                jar,
            })),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, HttpApiInner>, ApiError> {
        self.inner
            .lock()
            .map_err(|_| ApiError::TransportSetup("HTTP API lock poisoned".to_string()))
    }

    fn request_parts(&self, path: &str) -> Result<(Client, String, String), ApiError> {
        let inner = self.lock()?;
        let base_url = inner.base_url.clone();
        let endpoint = format!("{base_url}{path}");

        Ok((inner.client.clone(), endpoint, base_url))
    }

    fn get_json<T>(&self, path: &str) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        let (client, endpoint, base_url) = self.request_parts(path)?;
        let response = client
            .get(endpoint)
            .send()
            .map_err(|error| ApiError::Unreachable {
                server_url: base_url,
                message: error.to_string(),
            })?;

        decode_response(response)
    }

    fn post_json<B, T>(&self, path: &str, body: &B) -> Result<T, ApiError>
    where
        B: Serialize,
        T: DeserializeOwned,
    {
        let (client, endpoint, base_url) = self.request_parts(path)?;
        let response =
            client
                .post(endpoint)
                .json(body)
                .send()
                .map_err(|error| ApiError::Unreachable {
                    server_url: base_url,
                    message: error.to_string(),
                })?;

        decode_response(response)
    }

    fn put_json<B, T>(&self, path: &str, body: &B) -> Result<T, ApiError>
    where
        B: Serialize,
        T: DeserializeOwned,
    {
        let (client, endpoint, base_url) = self.request_parts(path)?;
        let response =
            client
                .put(endpoint)
                .json(body)
                .send()
                .map_err(|error| ApiError::Unreachable {
                    server_url: base_url,
                    message: error.to_string(),
                })?;

        decode_response(response)
    }

    fn post_json_empty<B>(&self, path: &str, body: &B) -> Result<(), ApiError>
    where
        B: Serialize,
    {
        let (client, endpoint, base_url) = self.request_parts(path)?;
        let response =
            client
                .post(endpoint)
                .json(body)
                .send()
                .map_err(|error| ApiError::Unreachable {
                    server_url: base_url,
                    message: error.to_string(),
                })?;

        decode_empty_response(response)
    }

    fn post_auth_json<B>(&self, path: &str, body: &B) -> Result<AuthSession, ApiError>
    where
        B: Serialize,
    {
        let (client, endpoint, base_url) = self.request_parts(path)?;
        let response =
            client
                .post(endpoint)
                .json(body)
                .send()
                .map_err(|error| ApiError::Unreachable {
                    server_url: base_url,
                    message: error.to_string(),
                })?;
        let session_token = session_token_from_response(&response);
        let user = decode_response(response)?;

        Ok(AuthSession::new(user, session_token))
    }

    fn post_empty(&self, path: &str) -> Result<(), ApiError> {
        let (client, endpoint, base_url) = self.request_parts(path)?;
        let response = client
            .post(endpoint)
            .send()
            .map_err(|error| ApiError::Unreachable {
                server_url: base_url,
                message: error.to_string(),
            })?;

        decode_empty_response(response)
    }

    fn delete_empty(&self, path: &str) -> Result<(), ApiError> {
        let (client, endpoint, base_url) = self.request_parts(path)?;
        let response = client
            .delete(endpoint)
            .send()
            .map_err(|error| ApiError::Unreachable {
                server_url: base_url,
                message: error.to_string(),
            })?;

        decode_empty_response(response)
    }

    fn delete_json<T>(&self, path: &str) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        let (client, endpoint, base_url) = self.request_parts(path)?;
        let response = client
            .delete(endpoint)
            .send()
            .map_err(|error| ApiError::Unreachable {
                server_url: base_url,
                message: error.to_string(),
            })?;

        decode_response(response)
    }

    fn post_multipart_file<T>(
        &self,
        path: &str,
        field: &str,
        file_path: PathBuf,
    ) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        let form = multipart::Form::new()
            .file(field.to_string(), &file_path)
            .map_err(|error| {
                ApiError::InvalidRequest(format!("could not read avatar file: {error}"))
            })?;
        let (client, endpoint, base_url) = self.request_parts(path)?;
        let response = client
            .post(endpoint)
            .multipart(form)
            .send()
            .map_err(|error| ApiError::Unreachable {
                server_url: base_url,
                message: error.to_string(),
            })?;

        decode_response(response)
    }

    fn post_no_body_json<T>(&self, path: &str) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        let (client, endpoint, base_url) = self.request_parts(path)?;
        let response = client
            .post(endpoint)
            .send()
            .map_err(|error| ApiError::Unreachable {
                server_url: base_url,
                message: error.to_string(),
            })?;

        decode_response(response)
    }

    fn clear_cookie_jar(&self) -> Result<(), ApiError> {
        let (client, jar) = build_client()?;
        let mut inner = self.lock()?;

        inner.client = client;
        inner.jar = jar;

        Ok(())
    }

    fn base_url_as_url(&self) -> Result<Url, ApiError> {
        Url::parse(&self.base_url()).map_err(|error| ApiError::InvalidServerUrl(error.to_string()))
    }
}

impl ApiClient for HttpApi {
    fn base_url(&self) -> String {
        match self.lock() {
            Ok(inner) => inner.base_url.clone(),
            Err(_) => DEFAULT_SERVER_URL.to_string(),
        }
    }

    fn set_base_url(&self, server_url: String) -> Result<(), ApiError> {
        let preferences = normalize_api_server_url(server_url)?;
        let (client, jar) = build_client()?;
        let mut inner = self.lock()?;

        inner.base_url = preferences.server_url;
        inner.client = client;
        inner.jar = jar;

        Ok(())
    }

    fn set_session_token(&self, session_token: Option<String>) -> Result<(), ApiError> {
        if let Some(session_token) = session_token {
            let url = self.base_url_as_url()?;
            let inner = self.lock()?;
            inner
                .jar
                .add_cookie_str(&format!("session={session_token}; Path=/"), &url);
            Ok(())
        } else {
            self.clear_cookie_jar()
        }
    }

    fn login(&self, username: String, password: String) -> Result<AuthSession, ApiError> {
        self.post_auth_json("/login", &LoginRequest { username, password })
    }

    fn register(
        &self,
        username: String,
        password: String,
        email: Option<String>,
    ) -> Result<AuthSession, ApiError> {
        self.post_auth_json(
            "/register",
            &RegisterRequest {
                username,
                password,
                email,
            },
        )
    }

    fn get_me(&self) -> Result<User, ApiError> {
        self.get_json("/me")
    }

    fn logout(&self) -> Result<(), ApiError> {
        let result = self.post_empty("/logout");
        let clear_result = self.clear_cookie_jar();

        match (result, clear_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
        }
    }

    fn update_profile(&self, display_name: Option<String>) -> Result<User, ApiError> {
        self.put_json("/me", &UpdateProfileRequest { display_name })
    }

    fn upload_avatar(&self, path: PathBuf) -> Result<User, ApiError> {
        self.post_multipart_file("/me/avatar", "file", path)
    }

    fn delete_avatar(&self) -> Result<User, ApiError> {
        self.delete_json("/me/avatar")
    }

    fn list_channels(&self) -> Result<Vec<Channel>, ApiError> {
        self.get_json("/channels")
    }

    fn create_channel(&self, name: String, kind: ChannelKind) -> Result<Channel, ApiError> {
        self.post_json("/channel", &CreateChannelRequest { name, kind })
    }

    fn reorder_channels(&self, ids: Vec<Id>) -> Result<Vec<Channel>, ApiError> {
        self.put_json("/channels/order", &ReorderChannelsRequest { ids })
    }

    fn get_messages(&self, channel_id: Id) -> Result<Vec<Message>, ApiError> {
        self.get_json(&format!("/messages/{channel_id}"))
    }

    fn send_message(&self, channel_id: Id, text: String) -> Result<Message, ApiError> {
        self.post_json(
            &format!("/message/{channel_id}"),
            &SendMessageRequest { text },
        )
    }

    fn post_typing(&self, channel_id: Id) -> Result<(), ApiError> {
        self.post_empty(&format!("/typing/{channel_id}"))
    }

    fn edit_message(&self, message_id: Id, text: String) -> Result<Message, ApiError> {
        self.put_json(
            &format!("/message/{message_id}"),
            &SendMessageRequest { text },
        )
    }

    fn delete_message(&self, message_id: Id) -> Result<(), ApiError> {
        self.delete_empty(&format!("/message/{message_id}"))
    }

    fn suppress_message_embeds(
        &self,
        message_id: Id,
        suppress: bool,
    ) -> Result<MessageEmbedsUpdatedEvent, ApiError> {
        self.post_json(
            &format!("/message/{message_id}/suppress_embeds"),
            &SuppressEmbedsRequest { suppress },
        )
    }

    fn list_voice_participants(&self, channel_id: Id) -> Result<Vec<VoiceParticipant>, ApiError> {
        self.get_json(&format!("/voice/participants/{channel_id}"))
    }

    fn get_voice_token(&self, channel_id: Id) -> Result<VoiceToken, ApiError> {
        self.post_no_body_json(&format!("/voice/token/{channel_id}"))
    }

    fn post_voice_speaking(&self, channel_id: Id, speaking: bool) -> Result<(), ApiError> {
        self.post_json_empty(
            "/voice/speaking",
            &VoiceSpeakingRequest {
                channel_id,
                speaking,
            },
        )
    }
}

pub fn runtime_api() -> Result<HttpApi, ApiError> {
    static RUNTIME_API: OnceLock<Result<HttpApi, String>> = OnceLock::new();

    RUNTIME_API
        .get_or_init(|| HttpApi::new(DEFAULT_SERVER_URL).map_err(|error| error.to_string()))
        .clone()
        .map_err(ApiError::TransportSetup)
}

fn build_client() -> Result<(Client, Arc<Jar>), ApiError> {
    let jar = Arc::new(Jar::default());
    let client = Client::builder()
        .cookie_provider(Arc::clone(&jar))
        .build()
        .map_err(|error| ApiError::TransportSetup(error.to_string()))?;

    Ok((client, jar))
}

fn normalize_api_server_url(server_url: String) -> Result<Preferences, ApiError> {
    Preferences::with_server_url(server_url)
        .map_err(|error| ApiError::InvalidServerUrl(error.to_string()))
}

fn decode_response<T>(response: Response) -> Result<T, ApiError>
where
    T: DeserializeOwned,
{
    let status = response.status();

    if status.is_success() {
        response
            .json::<T>()
            .map_err(|error| ApiError::Decode(error.to_string()))
    } else {
        Err(error_from_response(status, response_text(response)))
    }
}

fn decode_empty_response(response: Response) -> Result<(), ApiError> {
    let status = response.status();

    if status.is_success() {
        Ok(())
    } else {
        Err(error_from_response(status, response_text(response)))
    }
}

fn response_text(response: Response) -> String {
    response.text().unwrap_or_else(|error| error.to_string())
}

fn error_from_response(status: StatusCode, body: String) -> ApiError {
    let parsed = serde_json::from_str::<ErrorEnvelope>(&body).ok();
    let kind = parsed.as_ref().map(|envelope| envelope.error.kind.clone());
    let message = parsed
        .map(|envelope| envelope.error.message)
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| {
            status
                .canonical_reason()
                .map(str::to_string)
                .unwrap_or_else(|| "request failed".to_string())
        });

    match (status, kind.as_deref()) {
        (_, Some("invalid_credentials")) => ApiError::InvalidCredentials,
        (_, Some("username_taken")) => ApiError::UsernameTaken,
        (_, Some("invalid_request")) => ApiError::InvalidRequest(message),
        (_, Some("unauthorized")) => ApiError::Unauthorized,
        (StatusCode::UNAUTHORIZED, _) => ApiError::Unauthorized,
        _ => ApiError::Server {
            status: status.as_u16(),
            kind,
            message,
        },
    }
}

fn session_token_from_response(response: &Response) -> Option<String> {
    response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(parse_session_cookie)
}

fn parse_session_cookie(value: &str) -> Option<String> {
    value.split(';').next().and_then(|pair| {
        let (name, token) = pair.split_once('=')?;

        if name.trim() == "session" && !token.trim().is_empty() {
            Some(token.trim().to_string())
        } else {
            None
        }
    })
}
