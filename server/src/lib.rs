use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

pub mod auth;
pub mod broadcast;
pub mod entity;
pub mod middleware;
pub mod voice;

use actix_cors::Cors;
use actix_multipart::Multipart;
use actix_web::{
    App, HttpRequest, HttpResponse, HttpServer, Responder, Result, delete, error, get,
    http::StatusCode,
    middleware::Logger,
    post, put,
    web::{self, Data},
};
use derive_more::{Display, Error};
use futures_util::TryStreamExt;
use rand::Rng;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::broadcast::Broadcaster;
use crate::voice::{VoiceConfig, VoiceParticipant, VoiceState, parse_channel_id, room_name};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SendMessageRequest {
    text: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
    #[serde(default)]
    email: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Clone, Debug, Serialize)]
struct UserResponse {
    id: i64,
    username: String,
    email: Option<String>,
    email_verified: bool,
    avatar_url: Option<String>,
}

fn avatar_url(path: Option<&str>, updated_at: Option<i64>) -> Option<String> {
    match (path, updated_at) {
        (Some(p), Some(ts)) => Some(format!("/uploads/{p}?v={ts}")),
        _ => None,
    }
}

impl From<entity::user::Model> for UserResponse {
    fn from(u: entity::user::Model) -> Self {
        let avatar_url = avatar_url(u.avatar_path.as_deref(), u.avatar_updated_at);
        Self {
            id: u.id,
            username: u.username,
            email: u.email,
            email_verified: u.email_verified,
            avatar_url,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct MessageResponse {
    id: i64,
    user_id: i64,
    channel_id: i64,
    text: String,
    username: String,
    avatar_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct CreateChannelRequest {
    name: String,
    #[serde(default, rename = "type")]
    channel_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ReorderChannelsRequest {
    ids: Vec<i64>,
}

#[derive(Clone, Debug, Serialize)]
struct ChannelResponse {
    id: i64,
    name: String,
    position: i64,
    #[serde(rename = "type")]
    channel_type: String,
}

impl From<entity::channel::Model> for ChannelResponse {
    fn from(c: entity::channel::Model) -> Self {
        Self {
            id: c.id,
            name: c.name,
            position: c.position,
            channel_type: c.channel_type,
        }
    }
}

const CHANNEL_TYPE_TEXT: &str = "text";
const CHANNEL_TYPE_VOICE: &str = "voice";

#[derive(Clone, Debug, Serialize)]
struct MessageDeletedEvent {
    id: i64,
    channel_id: i64,
}

#[derive(Clone, Debug, Serialize)]
struct VoiceParticipantLeftEvent {
    channel_id: i64,
    user_id: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
enum BroadcastEvent {
    Message(MessageResponse),
    MessageUpdated(MessageResponse),
    MessageDeleted(MessageDeletedEvent),
    ChannelCreated(ChannelResponse),
    ChannelsReordered(Vec<ChannelResponse>),
    VoiceParticipantJoined(VoiceParticipant),
    VoiceParticipantLeft(VoiceParticipantLeftEvent),
}

// TODO(reno): return errors as JSON
#[derive(Debug, Display, Error)]
pub enum UserError {
    #[display("No channel found with that ID")]
    NoChannelFoundError, // TODO(reno): return the channel id that was invalid in the error response
    #[display("Internal database error")]
    DbError,
    #[display("Internal server error")]
    InternalError,
    #[display("Unauthorized")]
    Unauthorized,
    #[display("Invalid credentials")]
    InvalidCredentials,
    #[display("Username already taken")]
    UsernameTaken,
    #[display("Invalid request")]
    InvalidRequest,
    #[display("Payload too large")]
    PayloadTooLarge,
    #[display("Not found")]
    NotFound,
    #[display("Forbidden")]
    Forbidden,
    #[display("Service unavailable")]
    ServiceUnavailable,
}
impl error::ResponseError for UserError {
    fn status_code(&self) -> StatusCode {
        match *self {
            UserError::NoChannelFoundError => StatusCode::BAD_REQUEST,
            UserError::DbError => StatusCode::INTERNAL_SERVER_ERROR,
            UserError::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            UserError::Unauthorized => StatusCode::UNAUTHORIZED,
            UserError::InvalidCredentials => StatusCode::UNAUTHORIZED,
            UserError::UsernameTaken => StatusCode::CONFLICT,
            UserError::InvalidRequest => StatusCode::BAD_REQUEST,
            UserError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            UserError::NotFound => StatusCode::NOT_FOUND,
            UserError::Forbidden => StatusCode::FORBIDDEN,
            UserError::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

/// Where avatar files are written on disk. Registered as `web::Data` by
/// `start_server` (and by tests that exercise `/me/avatar`). Files land in
/// `<dir>/avatars/<user_id>.webp` and are served from `/uploads/avatars/<user_id>.webp`
/// by `actix_files::Files::new("/uploads", <dir>)`.
#[derive(Clone, Debug)]
pub struct AvatarStorage {
    pub dir: PathBuf,
}

const AVATAR_MAX_BYTES: usize = 2 * 1024 * 1024;
const AVATARS_SUBDIR: &str = "avatars";

const ID_LENGTH: u32 = 16;
const CHANNEL_NAME_MAX_LEN: usize = 128;

pub fn generate_id() -> i64 {
    let min = 10_i64.pow(ID_LENGTH - 1);
    let max = 10_i64.pow(ID_LENGTH);
    rand::rng().random_range(min..max)
}

#[get("/channels")]
async fn get_channels(db: web::Data<DatabaseConnection>) -> Result<impl Responder, UserError> {
    let channels = entity::channel::Entity::find()
        .order_by_asc(entity::channel::Column::Position)
        .order_by_asc(entity::channel::Column::Id)
        .all(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?;

    let resp: Vec<ChannelResponse> = channels.into_iter().map(ChannelResponse::from).collect();
    Ok(web::Json(resp))
}

#[get("/messages/{channel_id}")]
async fn get_messages(
    db: web::Data<DatabaseConnection>,
    path: web::Path<i64>,
) -> Result<impl Responder, UserError> {
    let channel_id = path.into_inner();

    let channel = entity::channel::Entity::find_by_id(channel_id)
        .one(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?;

    if channel.is_none() {
        return Err(UserError::NoChannelFoundError);
    }

    let rows = entity::message::Entity::find()
        .filter(entity::message::Column::ChannelId.eq(channel_id))
        .find_also_related(entity::user::Entity)
        .all(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?;

    let messages: Vec<MessageResponse> = rows
        .into_iter()
        .map(|(m, u)| {
            let (username, avatar_url) = match u {
                Some(u) => (
                    u.username,
                    avatar_url(u.avatar_path.as_deref(), u.avatar_updated_at),
                ),
                None => ("[deleted]".into(), None),
            };
            MessageResponse {
                id: m.id,
                user_id: m.user_id,
                channel_id: m.channel_id,
                text: m.text,
                username,
                avatar_url,
            }
        })
        .collect();

    Ok(web::Json(messages))
}

#[post("/message/{channel_id}")]
async fn create_message(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
    body: web::Json<SendMessageRequest>,
    user: AuthUser,
) -> Result<impl Responder, UserError> {
    let channel_id = path.into_inner();

    let channel = entity::channel::Entity::find_by_id(channel_id)
        .one(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?;

    if channel.is_none() {
        return Err(UserError::NoChannelFoundError);
    }

    let new_message = entity::message::ActiveModel {
        id: Set(generate_id()),
        user_id: Set(user.id),
        channel_id: Set(channel_id),
        text: Set(body.text.clone()),
    };

    let inserted = new_message
        .insert(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?;

    let resp = MessageResponse {
        id: inserted.id,
        user_id: inserted.user_id,
        channel_id: inserted.channel_id,
        text: inserted.text,
        username: user.username.clone(),
        avatar_url: avatar_url(user.avatar_path.as_deref(), user.avatar_updated_at),
    };
    let payload = serde_json::to_string(&BroadcastEvent::Message(resp.clone()))
        .map_err(|_| UserError::InternalError)?;
    broadcaster.broadcast(&payload).await;

    Ok(web::Json(resp))
}

#[put("/message/{message_id}")]
async fn update_message(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
    body: web::Json<SendMessageRequest>,
    user: AuthUser,
) -> Result<impl Responder, UserError> {
    let message_id = path.into_inner();

    let existing = entity::message::Entity::find_by_id(message_id)
        .one(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?
        .ok_or(UserError::NotFound)?;

    if existing.user_id != user.id {
        return Err(UserError::Forbidden);
    }

    let channel_id = existing.channel_id;
    let mut active: entity::message::ActiveModel = existing.into();
    active.text = Set(body.text.clone());
    let updated = active
        .update(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?;

    let resp = MessageResponse {
        id: updated.id,
        user_id: updated.user_id,
        channel_id,
        text: updated.text,
        username: user.username.clone(),
        avatar_url: avatar_url(user.avatar_path.as_deref(), user.avatar_updated_at),
    };
    let payload = serde_json::to_string(&BroadcastEvent::MessageUpdated(resp.clone()))
        .map_err(|_| UserError::InternalError)?;
    broadcaster.broadcast(&payload).await;

    Ok(web::Json(resp))
}

#[delete("/message/{message_id}")]
async fn delete_message(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<impl Responder, UserError> {
    let message_id = path.into_inner();

    let existing = entity::message::Entity::find_by_id(message_id)
        .one(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?
        .ok_or(UserError::NotFound)?;

    if existing.user_id != user.id {
        return Err(UserError::Forbidden);
    }

    let channel_id = existing.channel_id;
    let active: entity::message::ActiveModel = existing.into();
    active
        .delete(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?;

    let payload = serde_json::to_string(&BroadcastEvent::MessageDeleted(MessageDeletedEvent {
        id: message_id,
        channel_id,
    }))
    .map_err(|_| UserError::InternalError)?;
    broadcaster.broadcast(&payload).await;

    Ok(HttpResponse::NoContent().finish())
}

#[post("/channel")]
async fn create_channel(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    body: web::Json<CreateChannelRequest>,
    _user: AuthUser,
) -> Result<impl Responder, UserError> {
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > CHANNEL_NAME_MAX_LEN {
        return Err(UserError::InvalidRequest);
    }
    let channel_type = match body.channel_type.as_deref() {
        None | Some(CHANNEL_TYPE_TEXT) => CHANNEL_TYPE_TEXT,
        Some(CHANNEL_TYPE_VOICE) => CHANNEL_TYPE_VOICE,
        Some(_) => return Err(UserError::InvalidRequest),
    };

    let max_position = entity::channel::Entity::find()
        .order_by_desc(entity::channel::Column::Position)
        .one(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?
        .map(|c| c.position);
    let next_position = max_position.map(|p| p + 1).unwrap_or(0);

    let new_channel = entity::channel::ActiveModel {
        id: Set(generate_id()),
        name: Set(name.to_owned()),
        position: Set(next_position),
        channel_type: Set(channel_type.to_owned()),
    };

    let inserted = new_channel
        .insert(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?;

    let resp = ChannelResponse::from(inserted);
    let payload = serde_json::to_string(&BroadcastEvent::ChannelCreated(resp.clone()))
        .map_err(|_| UserError::InternalError)?;
    broadcaster.broadcast(&payload).await;

    Ok(web::Json(resp))
}

#[put("/channels/order")]
async fn reorder_channels(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    body: web::Json<ReorderChannelsRequest>,
    _user: AuthUser,
) -> Result<impl Responder, UserError> {
    let existing = entity::channel::Entity::find()
        .all(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?;

    // The request must reference exactly the full set of existing channels
    // with no duplicates or omissions — partial reorders are ambiguous.
    let existing_ids: HashSet<i64> = existing.iter().map(|c| c.id).collect();
    let request_ids_unique: HashSet<i64> = body.ids.iter().copied().collect();
    if body.ids.len() != existing.len() || request_ids_unique != existing_ids {
        return Err(UserError::InvalidRequest);
    }

    let mut by_id: HashMap<i64, entity::channel::Model> =
        existing.into_iter().map(|c| (c.id, c)).collect();

    let mut updated: Vec<entity::channel::Model> = Vec::with_capacity(body.ids.len());
    for (idx, id) in body.ids.iter().enumerate() {
        let model = by_id.remove(id).ok_or(UserError::InvalidRequest)?;
        let mut active: entity::channel::ActiveModel = model.into();
        active.position = Set(idx as i64);
        let saved = active
            .update(db.get_ref())
            .await
            .map_err(|_| UserError::DbError)?;
        updated.push(saved);
    }

    let channels: Vec<ChannelResponse> = updated.into_iter().map(ChannelResponse::from).collect();
    let payload = serde_json::to_string(&BroadcastEvent::ChannelsReordered(channels.clone()))
        .map_err(|_| UserError::InternalError)?;
    broadcaster.broadcast(&payload).await;

    Ok(web::Json(channels))
}

#[get("/messages/subscribe")]
async fn subscribe_to_channel_events(broadcaster: web::Data<Broadcaster>) -> impl Responder {
    broadcaster.new_client().await
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
struct VoiceTokenResponse {
    url: String,
    token: String,
    room: String,
}

#[post("/voice/token/{channel_id}")]
async fn mint_voice_token(
    db: web::Data<DatabaseConnection>,
    voice_cfg: web::Data<Option<VoiceConfig>>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<impl Responder, UserError> {
    let channel_id = path.into_inner();
    let cfg = voice_cfg
        .as_ref()
        .as_ref()
        .ok_or(UserError::ServiceUnavailable)?;

    let channel = entity::channel::Entity::find_by_id(channel_id)
        .one(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?
        .ok_or(UserError::NoChannelFoundError)?;
    if channel.channel_type != CHANNEL_TYPE_VOICE {
        return Err(UserError::InvalidRequest);
    }

    let room = room_name(channel_id);
    let grants = livekit_api::access_token::VideoGrants {
        room_join: true,
        room: room.clone(),
        can_publish: true,
        can_subscribe: true,
        can_publish_data: true,
        ..Default::default()
    };
    let token = livekit_api::access_token::AccessToken::with_api_key(&cfg.api_key, &cfg.api_secret)
        .with_identity(&user.id.to_string())
        .with_name(&user.username)
        .with_grants(grants)
        .to_jwt()
        .map_err(|_| UserError::InternalError)?;

    Ok(web::Json(VoiceTokenResponse {
        url: cfg.url.clone(),
        token,
        room,
    }))
}

#[get("/voice/participants/{channel_id}")]
async fn list_voice_participants(
    voice_state: web::Data<VoiceState>,
    path: web::Path<i64>,
) -> Result<impl Responder, UserError> {
    let channel_id = path.into_inner();
    Ok(web::Json(voice_state.participants(channel_id)))
}

/// Apply a single parsed LiveKit webhook event to in-memory state and
/// broadcast the resulting SSE payload. Split out of the HTTP handler so it
/// can be exercised by unit tests without a signed-JWT round-trip.
async fn apply_voice_webhook(
    db: &DatabaseConnection,
    voice_state: &VoiceState,
    broadcaster: &Broadcaster,
    event: &livekit_protocol::WebhookEvent,
) -> Result<(), UserError> {
    let Some(room) = event.room.as_ref() else {
        return Ok(());
    };
    let Some(channel_id) = parse_channel_id(&room.name) else {
        return Ok(());
    };
    let Some(participant) = event.participant.as_ref() else {
        return Ok(());
    };
    let Ok(user_id) = participant.identity.parse::<i64>() else {
        return Ok(());
    };

    match event.event.as_str() {
        "participant_joined" => {
            // Look up the authoritative username/avatar from the DB so the
            // sidebar doesn't have to trust whatever the client published.
            let Some(user) = entity::user::Entity::find_by_id(user_id)
                .one(db)
                .await
                .map_err(|_| UserError::DbError)?
            else {
                return Ok(());
            };
            let p = VoiceParticipant {
                user_id,
                channel_id,
                username: user.username,
                avatar_url: avatar_url(user.avatar_path.as_deref(), user.avatar_updated_at),
            };
            if voice_state.add_participant(p.clone()) {
                let payload = serde_json::to_string(&BroadcastEvent::VoiceParticipantJoined(p))
                    .map_err(|_| UserError::InternalError)?;
                broadcaster.broadcast(&payload).await;
            }
        }
        "participant_left" | "participant_connection_aborted" => {
            if voice_state
                .remove_participant(channel_id, user_id)
                .is_some()
            {
                let payload = serde_json::to_string(&BroadcastEvent::VoiceParticipantLeft(
                    VoiceParticipantLeftEvent {
                        channel_id,
                        user_id,
                    },
                ))
                .map_err(|_| UserError::InternalError)?;
                broadcaster.broadcast(&payload).await;
            }
        }
        _ => {}
    }
    Ok(())
}

#[post("/livekit/webhook")]
async fn receive_voice_webhook(
    db: web::Data<DatabaseConnection>,
    voice_state: web::Data<VoiceState>,
    broadcaster: web::Data<Broadcaster>,
    voice_cfg: web::Data<Option<VoiceConfig>>,
    req: HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse, UserError> {
    let cfg = voice_cfg
        .as_ref()
        .as_ref()
        .ok_or(UserError::ServiceUnavailable)?;
    let auth = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(UserError::Unauthorized)?;
    let body_str = std::str::from_utf8(&body).map_err(|_| UserError::InvalidRequest)?;

    let verifier =
        livekit_api::access_token::TokenVerifier::with_api_key(&cfg.api_key, &cfg.api_secret);
    let receiver = livekit_api::webhooks::WebhookReceiver::new(verifier);
    let event = receiver
        .receive(body_str, auth)
        .map_err(|_| UserError::Unauthorized)?;

    apply_voice_webhook(
        db.get_ref(),
        voice_state.get_ref(),
        broadcaster.get_ref(),
        &event,
    )
    .await?;
    Ok(HttpResponse::Ok().finish())
}

#[post("/register")]
async fn register(
    db: web::Data<DatabaseConnection>,
    body: web::Json<RegisterRequest>,
) -> Result<HttpResponse, UserError> {
    if body.username.is_empty() || body.password.is_empty() {
        return Err(UserError::InvalidRequest);
    }
    let user = auth::register_user(
        db.get_ref(),
        &body.username,
        &body.password,
        body.email.clone(),
    )
    .await?;
    let session = auth::create_session(db.get_ref(), user.id).await?;
    Ok(HttpResponse::Ok()
        .cookie(auth::session_cookie(session.token))
        .json(UserResponse::from(user)))
}

#[post("/login")]
async fn login(
    db: web::Data<DatabaseConnection>,
    body: web::Json<LoginRequest>,
) -> Result<HttpResponse, UserError> {
    let user = auth::authenticate_password(db.get_ref(), &body.username, &body.password).await?;
    let session = auth::create_session(db.get_ref(), user.id).await?;
    Ok(HttpResponse::Ok()
        .cookie(auth::session_cookie(session.token))
        .json(UserResponse::from(user)))
}

#[post("/logout")]
async fn logout(
    db: web::Data<DatabaseConnection>,
    req: HttpRequest,
) -> Result<HttpResponse, UserError> {
    if let Some(c) = req.cookie(auth::SESSION_COOKIE) {
        auth::destroy_session(db.get_ref(), c.value()).await?;
    }
    Ok(HttpResponse::Ok()
        .cookie(auth::clear_session_cookie())
        .finish())
}

#[get("/me")]
async fn me(
    db: web::Data<DatabaseConnection>,
    user: AuthUser,
) -> Result<impl Responder, UserError> {
    let user = entity::user::Entity::find_by_id(user.id)
        .one(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?
        .ok_or(UserError::Unauthorized)?;
    Ok(web::Json(UserResponse::from(user)))
}

fn now_unix_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[post("/me/avatar")]
async fn upload_avatar(
    db: web::Data<DatabaseConnection>,
    storage: web::Data<AvatarStorage>,
    user: AuthUser,
    mut payload: Multipart,
) -> Result<impl Responder, UserError> {
    // Collect the first (and only) `file` field, bounded by AVATAR_MAX_BYTES.
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;

    while let Some(mut field) = payload
        .try_next()
        .await
        .map_err(|_| UserError::InvalidRequest)?
    {
        let is_file = field
            .content_disposition()
            .and_then(|d| d.get_name())
            .is_some_and(|n| n == "file");
        if !is_file {
            continue;
        }
        content_type = field.content_type().map(|m| m.essence_str().to_owned());

        let mut buf = Vec::new();
        while let Some(chunk) = field
            .try_next()
            .await
            .map_err(|_| UserError::InvalidRequest)?
        {
            if buf.len() + chunk.len() > AVATAR_MAX_BYTES {
                return Err(UserError::PayloadTooLarge);
            }
            buf.extend_from_slice(&chunk);
        }
        file_bytes = Some(buf);
        break;
    }

    let bytes = file_bytes.ok_or(UserError::InvalidRequest)?;
    let ct = content_type.unwrap_or_default();
    if !matches!(ct.as_str(), "image/jpeg" | "image/png" | "image/webp") {
        return Err(UserError::InvalidRequest);
    }

    // Decode, cover-crop to a square, resize to 256×256, re-encode as WebP.
    let img = image::load_from_memory(&bytes).map_err(|_| UserError::InvalidRequest)?;
    let (w, h) = (img.width(), img.height());
    let side = w.min(h);
    let x = (w - side) / 2;
    let y = (h - side) / 2;
    let cropped = img.crop_imm(x, y, side, side);
    let resized = cropped.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);

    let mut out = Cursor::new(Vec::new());
    resized
        .write_to(&mut out, image::ImageFormat::WebP)
        .map_err(|_| UserError::InternalError)?;
    let webp = out.into_inner();

    // Atomic write: <dir>/avatars/<id>.webp.tmp -> <dir>/avatars/<id>.webp
    let avatars_dir = storage.dir.join(AVATARS_SUBDIR);
    std::fs::create_dir_all(&avatars_dir).map_err(|_| UserError::InternalError)?;
    let filename = format!("{}.webp", user.id);
    let final_path = avatars_dir.join(&filename);
    let tmp_path = avatars_dir.join(format!("{}.webp.tmp", user.id));
    std::fs::write(&tmp_path, &webp).map_err(|_| UserError::InternalError)?;
    std::fs::rename(&tmp_path, &final_path).map_err(|_| UserError::InternalError)?;

    let rel_path = format!("{AVATARS_SUBDIR}/{filename}");
    let now = now_unix_secs();
    let existing = entity::user::Entity::find_by_id(user.id)
        .one(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?
        .ok_or(UserError::Unauthorized)?;
    let mut model: entity::user::ActiveModel = existing.into();
    model.avatar_path = Set(Some(rel_path));
    model.avatar_updated_at = Set(Some(now));
    let updated = model
        .update(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?;

    Ok(web::Json(UserResponse::from(updated)))
}

#[delete("/me/avatar")]
async fn delete_avatar(
    db: web::Data<DatabaseConnection>,
    storage: web::Data<AvatarStorage>,
    user: AuthUser,
) -> Result<impl Responder, UserError> {
    let existing = entity::user::Entity::find_by_id(user.id)
        .one(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?
        .ok_or(UserError::Unauthorized)?;

    if let Some(rel) = existing.avatar_path.clone() {
        let path = storage.dir.join(&rel);
        // Ignore ENOENT so delete is idempotent.
        if let Err(e) = std::fs::remove_file(&path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            return Err(UserError::InternalError);
        }
    }

    let mut model: entity::user::ActiveModel = existing.into();
    model.avatar_path = Set(None);
    model.avatar_updated_at = Set(None);
    let updated = model
        .update(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?;

    Ok(web::Json(UserResponse::from(updated)))
}

fn default_avatar_webp() -> Vec<u8> {
    use image::{Rgb, RgbImage};
    let size = 256u32;
    let mut img = RgbImage::new(size, size);
    let cx = size as i32 / 2;
    let cy = size as i32 / 2;
    let r2 = (size as i32 / 2 - 8).pow(2);
    let bg = Rgb([44u8, 82, 130]);
    let fg = Rgb([129u8, 230, 217]);
    for y in 0..size {
        for x in 0..size {
            let dx = x as i32 - cx;
            let dy = y as i32 - cy;
            let pixel = if dx * dx + dy * dy < r2 { fg } else { bg };
            img.put_pixel(x, y, pixel);
        }
    }
    let mut out = Cursor::new(Vec::new());
    let _ = image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::WebP);
    out.into_inner()
}

#[allow(clippy::unwrap_used)]
pub async fn seed_development_data(db: &DatabaseConnection) {
    use std::time::{SystemTime, UNIX_EPOCH};

    db.get_schema_registry("hamlet::entity::*")
        .sync(db)
        .await
        .unwrap();

    let general_channel = entity::channel::ActiveModel {
        id: Set(generate_id()),
        name: Set("general".to_owned()),
        position: Set(0),
        channel_type: Set(CHANNEL_TYPE_TEXT.to_owned()),
    };
    general_channel.insert(db).await.unwrap();

    let voice_channel = entity::channel::ActiveModel {
        id: Set(generate_id()),
        name: Set("voice".to_owned()),
        position: Set(1),
        channel_type: Set(CHANNEL_TYPE_VOICE.to_owned()),
    };
    voice_channel.insert(db).await.unwrap();

    let dev_user = auth::register_user(db, "baipas", "password", None)
        .await
        .unwrap();

    auth::register_user(db, "teo", "password", None)
        .await
        .unwrap();

    // Seed a default avatar so the dev user exercises the full pipeline on first boot.
    let uploads_dir = PathBuf::from("./uploads");
    let avatars_dir = uploads_dir.join(AVATARS_SUBDIR);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if std::fs::create_dir_all(&avatars_dir).is_ok() {
        let filename = format!("{}.webp", dev_user.id);
        if std::fs::write(avatars_dir.join(&filename), default_avatar_webp()).is_ok() {
            let mut model: entity::user::ActiveModel = dev_user.clone().into();
            model.avatar_path = Set(Some(format!("{AVATARS_SUBDIR}/{filename}")));
            model.avatar_updated_at = Set(Some(now));
            let _ = model.update(db).await;
        }
    }

    const DEV_SESSION_TOKEN: &str =
        "devdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdev";
    entity::session::ActiveModel {
        token: Set(DEV_SESSION_TOKEN.to_owned()),
        user_id: Set(dev_user.id),
        created_at: Set(now),
        expires_at: Set(now + 60 * 60 * 24 * 365),
    }
    .insert(db)
    .await
    .unwrap();

    println!("=== DEV: baipas session active — set cookie: session={DEV_SESSION_TOKEN} ===");
}

/// Three-argument form for tests and any deployment that doesn't need voice.
/// Calls `configure_app_with_voice` with an unconfigured `VoiceConfig` and a
/// fresh `VoiceState`, so `/voice/*` endpoints reply with 503 but everything
/// else works identically.
pub fn configure_app(
    cfg: &mut web::ServiceConfig,
    db_data: Data<DatabaseConnection>,
    broadcaster: Data<Broadcaster>,
) {
    configure_app_with_voice(
        cfg,
        db_data,
        broadcaster,
        Data::new(None::<VoiceConfig>),
        Data::new(VoiceState::new()),
    );
}

pub fn configure_app_with_voice(
    cfg: &mut web::ServiceConfig,
    db_data: Data<DatabaseConnection>,
    broadcaster: Data<Broadcaster>,
    voice_cfg: Data<Option<VoiceConfig>>,
    voice_state: Data<VoiceState>,
) {
    cfg.app_data(db_data.clone())
        .app_data(broadcaster)
        .app_data(voice_cfg)
        .app_data(voice_state)
        // Public — no auth required
        .service(register)
        .service(login)
        .service(logout)
        // LiveKit webhooks are authenticated via a signed JWT in the body,
        // not a session cookie, so they must live outside the `require_auth`
        // scope.
        .service(receive_voice_webhook)
        // Everything else requires auth
        .service(
            web::scope("")
                .wrap(actix_web::middleware::from_fn(middleware::require_auth))
                // subscribe must be registered before get_messages to win over /{channel_id}
                .service(subscribe_to_channel_events)
                .service(get_channels)
                .service(get_messages)
                .service(create_message)
                .service(update_message)
                .service(delete_message)
                .service(create_channel)
                .service(reorder_channels)
                .service(mint_voice_token)
                .service(list_voice_participants)
                .service(me)
                .service(upload_avatar)
                .service(delete_avatar),
        );
}

pub async fn start_server(
    db: DatabaseConnection,
    broadcaster: Arc<Broadcaster>,
) -> std::io::Result<()> {
    let broadcaster_data = web::Data::from(broadcaster);
    let db_data = web::Data::new(db);
    let uploads_dir = PathBuf::from("./uploads");
    std::fs::create_dir_all(uploads_dir.join(AVATARS_SUBDIR))?;
    let avatar_storage = web::Data::new(AvatarStorage {
        dir: uploads_dir.clone(),
    });
    let voice_cfg = VoiceConfig::from_env();
    if voice_cfg.is_none() {
        eprintln!(
            "LIVEKIT_URL / LIVEKIT_API_KEY / LIVEKIT_API_SECRET not set — voice endpoints will return 503"
        );
    }
    let voice_cfg_data = web::Data::new(voice_cfg);
    let voice_state_data = web::Data::new(VoiceState::new());

    // logger config - should review
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let bind_addr =
        std::env::var("HAMLET_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3030".to_string());

    HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin_fn(|origin, _| {
                // TODO(reno): this is my lazy way to get CORS working for local envs. probably
                // worth revisiting/removing entirely when deploying to a production environment
                origin.as_bytes().starts_with(b"http://localhost")
                    || origin.as_bytes().starts_with(b"http://127.0.0.1")
            })
            // NOTE(reno): These are dangerous - probably worth reconsidering if keeping CORS
            // in production mode.
            .allow_any_method()
            .allow_any_header()
            .supports_credentials();

        App::new()
            .wrap(Logger::default())
            .wrap(cors)
            .app_data(avatar_storage.clone())
            .service(actix_files::Files::new("/uploads", uploads_dir.clone()))
            .configure(|cfg| {
                configure_app_with_voice(
                    cfg,
                    db_data.clone(),
                    broadcaster_data.clone(),
                    voice_cfg_data.clone(),
                    voice_state_data.clone(),
                )
            })
    })
    .bind(&bind_addr)?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use actix_web::http::header::ContentType;
    use actix_web::test;
    use sea_orm::Database;

    use super::*;

    #[allow(clippy::unwrap_used)]
    async fn setup_db() -> (DatabaseConnection, i64) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let url = format!("sqlite:file:hamlet_inline_test_{n}?mode=memory&cache=shared");
        let db = Database::connect(&url).await.unwrap();
        db.get_schema_registry("hamlet::entity::*")
            .sync(&db)
            .await
            .unwrap();

        let chan_id = generate_id();
        entity::channel::ActiveModel {
            id: Set(chan_id),
            name: Set("general".to_owned()),
            position: Set(0),
            channel_type: Set(CHANNEL_TYPE_TEXT.to_owned()),
        }
        .insert(&db)
        .await
        .unwrap();

        (db, chan_id)
    }

    fn session_cookie_header(token: &str) -> (String, String) {
        (
            "Cookie".to_owned(),
            format!("{}={}", auth::SESSION_COOKIE, token),
        )
    }

    #[actix_web::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn test_message_create_broadcasts_to_clients() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let broadcaster_data = web::Data::from(broadcaster);

        let db_data = web::Data::new(db);
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri(&format!("/message/{}", chan_id))
            .insert_header(ContentType::json())
            .insert_header((name, value))
            .set_payload(
                serde_json::to_string(&SendMessageRequest {
                    text: "hello".into(),
                })
                .unwrap(),
            )
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — broadcast was never sent")
            .expect("channel closed");

        let event_str = format!("{:?}", event);
        assert!(
            event_str.contains("hello"),
            "broadcast event should contain the message text; got {event_str}"
        );
        assert!(
            event_str.contains("alice"),
            "broadcast event should contain the author username; got {event_str}"
        );
        assert!(
            event_str.contains("kind\\\":\\\"message\\\""),
            "broadcast event should be tagged as kind=message; got {event_str}"
        );
    }

    #[actix_web::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn test_message_delete_broadcasts_to_clients() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let msg_id = generate_id();
        entity::message::ActiveModel {
            id: Set(msg_id),
            user_id: Set(user.id),
            channel_id: Set(chan_id),
            text: Set("bye".into()),
        }
        .insert(&db)
        .await
        .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let broadcaster_data = web::Data::from(broadcaster);

        let db_data = web::Data::new(db);
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::delete()
            .uri(&format!("/message/{}", msg_id))
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — broadcast was never sent")
            .expect("channel closed");

        let event_str = format!("{:?}", event);
        assert!(
            event_str.contains("kind\\\":\\\"message_deleted\\\""),
            "broadcast event should be tagged as kind=message_deleted; got {event_str}"
        );
        assert!(
            event_str.contains(&msg_id.to_string()),
            "broadcast event should contain the deleted message id; got {event_str}"
        );
    }

    #[actix_web::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn test_reorder_channels_broadcasts_to_clients() {
        let (db, general_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let other_id = generate_id();
        entity::channel::ActiveModel {
            id: Set(other_id),
            name: Set("other".to_owned()),
            position: Set(1),
            channel_type: Set(CHANNEL_TYPE_TEXT.to_owned()),
        }
        .insert(&db)
        .await
        .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let broadcaster_data = web::Data::from(broadcaster);

        let db_data = web::Data::new(db);
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::put()
            .uri("/channels/order")
            .insert_header(ContentType::json())
            .insert_header((name, value))
            .set_payload(serde_json::json!({"ids": [other_id, general_id]}).to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — broadcast was never sent")
            .expect("channel closed");
        let event_str = format!("{:?}", event);
        assert!(
            event_str.contains("kind\\\":\\\"channels_reordered\\\""),
            "broadcast event should be tagged as kind=channels_reordered; got {event_str}"
        );
    }

    #[actix_web::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn test_create_channel_broadcasts_to_clients() {
        let (db, _) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let broadcaster_data = web::Data::from(broadcaster);

        let db_data = web::Data::new(db);
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri("/channel")
            .insert_header(ContentType::json())
            .insert_header((name, value))
            .set_payload(serde_json::json!({"name": "random"}).to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — broadcast was never sent")
            .expect("channel closed");

        let event_str = format!("{:?}", event);
        assert!(
            event_str.contains("kind\\\":\\\"channel_created\\\""),
            "broadcast event should be tagged as kind=channel_created; got {event_str}"
        );
        assert!(
            event_str.contains("random"),
            "broadcast event should contain the channel name; got {event_str}"
        );
    }

    fn make_webhook_event(
        event: &str,
        channel_id: i64,
        user_id: i64,
        username: &str,
    ) -> livekit_protocol::WebhookEvent {
        livekit_protocol::WebhookEvent {
            event: event.to_string(),
            room: Some(livekit_protocol::Room {
                name: room_name(channel_id),
                ..Default::default()
            }),
            participant: Some(livekit_protocol::ParticipantInfo {
                identity: user_id.to_string(),
                name: username.to_string(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[actix_web::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn test_voice_webhook_join_updates_state_and_broadcasts() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();

        let event = make_webhook_event("participant_joined", chan_id, user.id, &user.username);
        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        let participants = voice_state.participants(chan_id);
        assert_eq!(participants.len(), 1);
        assert_eq!(participants[0].user_id, user.id);
        assert_eq!(participants[0].username, "alice");

        let broadcast = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let s = format!("{:?}", broadcast);
        assert!(
            s.contains("voice_participant_joined"),
            "broadcast should be voice_participant_joined, got {s}"
        );
        assert!(s.contains("alice"), "should include username, got {s}");
    }

    #[actix_web::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn test_voice_webhook_leave_removes_state() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let voice_state = VoiceState::new();
        voice_state.add_participant(VoiceParticipant {
            user_id: user.id,
            channel_id: chan_id,
            username: user.username.clone(),
            avatar_url: None,
        });

        let event = make_webhook_event("participant_left", chan_id, user.id, &user.username);
        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        assert!(voice_state.participants(chan_id).is_empty());
    }

    #[actix_web::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn test_voice_webhook_ignores_unknown_room() {
        let (db, _chan) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();

        // A room that doesn't match our `channel-{id}` scheme must be ignored.
        let event = livekit_protocol::WebhookEvent {
            event: "participant_joined".into(),
            room: Some(livekit_protocol::Room {
                name: "some-other-tenant".into(),
                ..Default::default()
            }),
            participant: Some(livekit_protocol::ParticipantInfo {
                identity: user.id.to_string(),
                name: user.username.clone(),
                ..Default::default()
            }),
            ..Default::default()
        };

        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        assert!(
            tokio::time::timeout(Duration::from_millis(100), rx.recv())
                .await
                .is_err(),
            "no broadcast should be sent for an unrelated room"
        );
    }

    #[actix_web::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn test_mint_voice_token_requires_voice_channel() {
        // This channel is text-only; the token endpoint should reject it.
        let (db, text_chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let voice_cfg = VoiceConfig {
            url: "ws://localhost:7880".to_string(),
            api_key: "devkey".to_string(),
            api_secret: "devsecretdevsecretdevsecretdevsecret".to_string(),
        };
        let db_data = web::Data::new(db);
        let app = test::init_service(App::new().configure(|cfg| {
            configure_app_with_voice(
                cfg,
                db_data,
                web::Data::from(Broadcaster::new()),
                web::Data::new(Some(voice_cfg)),
                web::Data::new(VoiceState::new()),
            )
        }))
        .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri(&format!("/voice/token/{}", text_chan_id))
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "text channel should be rejected");
    }

    #[actix_web::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn test_mint_voice_token_returns_jwt_for_voice_channel() {
        let (db, _text) = setup_db().await;
        let voice_chan_id = generate_id();
        entity::channel::ActiveModel {
            id: Set(voice_chan_id),
            name: Set("lounge".into()),
            position: Set(1),
            channel_type: Set(CHANNEL_TYPE_VOICE.into()),
        }
        .insert(&db)
        .await
        .unwrap();

        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let voice_cfg = VoiceConfig {
            url: "ws://localhost:7880".to_string(),
            api_key: "devkey".to_string(),
            api_secret: "devsecretdevsecretdevsecretdevsecret".to_string(),
        };
        let db_data = web::Data::new(db);
        let app = test::init_service(App::new().configure(|cfg| {
            configure_app_with_voice(
                cfg,
                db_data,
                web::Data::from(Broadcaster::new()),
                web::Data::new(Some(voice_cfg.clone())),
                web::Data::new(VoiceState::new()),
            )
        }))
        .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri(&format!("/voice/token/{}", voice_chan_id))
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body: VoiceTokenResponse = test::read_body_json(resp).await;
        assert_eq!(body.url, voice_cfg.url);
        assert_eq!(body.room, room_name(voice_chan_id));

        let verifier = livekit_api::access_token::TokenVerifier::with_api_key(
            &voice_cfg.api_key,
            &voice_cfg.api_secret,
        );
        let claims = verifier.verify(&body.token).expect("token must verify");
        assert_eq!(claims.sub, user.id.to_string());
        assert_eq!(claims.name, "alice");
        assert_eq!(claims.video.room, room_name(voice_chan_id));
        assert!(claims.video.room_join);
    }

    #[actix_web::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn test_mint_voice_token_returns_503_when_unconfigured() {
        let (db, _) = setup_db().await;
        let voice_chan_id = generate_id();
        entity::channel::ActiveModel {
            id: Set(voice_chan_id),
            name: Set("lounge".into()),
            position: Set(1),
            channel_type: Set(CHANNEL_TYPE_VOICE.into()),
        }
        .insert(&db)
        .await
        .unwrap();

        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();
        let db_data = web::Data::new(db);
        // Three-argument configure_app wires in None voice config.
        let app = test::init_service(
            App::new()
                .configure(|cfg| configure_app(cfg, db_data, web::Data::from(Broadcaster::new()))),
        )
        .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri(&format!("/voice/token/{}", voice_chan_id))
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 503);
    }
}
