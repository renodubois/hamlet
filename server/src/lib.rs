use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

pub mod auth;
pub mod broadcast;
pub mod entity;
pub mod middleware;

use actix_cors::Cors;
use actix_multipart::Multipart;
use actix_web::{
    App, HttpRequest, HttpResponse, HttpServer, Responder, Result, delete, error, get,
    http::StatusCode,
    middleware::Logger,
    post,
    web::{self, Data},
};
use derive_more::{Display, Error};
use futures_util::TryStreamExt;
use rand::Rng;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::broadcast::Broadcaster;

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
}

#[derive(Clone, Debug, Serialize)]
struct ChannelResponse {
    id: i64,
    name: String,
}

impl From<entity::channel::Model> for ChannelResponse {
    fn from(c: entity::channel::Model) -> Self {
        Self {
            id: c.id,
            name: c.name,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
enum BroadcastEvent {
    Message(MessageResponse),
    ChannelCreated(ChannelResponse),
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
        .all(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?;

    Ok(web::Json(channels))
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

    let new_channel = entity::channel::ActiveModel {
        id: Set(generate_id()),
        name: Set(name.to_owned()),
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

#[get("/messages/subscribe")]
async fn subscribe_to_channel_events(broadcaster: web::Data<Broadcaster>) -> impl Responder {
    broadcaster.new_client().await
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
    };
    general_channel.insert(db).await.unwrap();

    let dev_user = auth::register_user(db, "baipas", "password", None)
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

pub fn configure_app(
    cfg: &mut web::ServiceConfig,
    db_data: Data<DatabaseConnection>,
    broadcaster: Data<Broadcaster>,
) {
    cfg.app_data(db_data.clone())
        .app_data(broadcaster)
        // Public — no auth required
        .service(register)
        .service(login)
        .service(logout)
        // Everything else requires auth
        .service(
            web::scope("")
                .wrap(actix_web::middleware::from_fn(middleware::require_auth))
                // subscribe must be registered before get_messages to win over /{channel_id}
                .service(subscribe_to_channel_events)
                .service(get_channels)
                .service(get_messages)
                .service(create_message)
                .service(create_channel)
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
            .configure(|cfg| configure_app(cfg, db_data.clone(), broadcaster_data.clone()))
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
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.get_schema_registry("hamlet::entity::*")
            .sync(&db)
            .await
            .unwrap();

        let chan_id = generate_id();
        entity::channel::ActiveModel {
            id: Set(chan_id),
            name: Set("general".to_owned()),
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
}
