use std::sync::Arc;

pub mod auth;
pub mod broadcast;
pub mod entity;
pub mod middleware;

use actix_cors::Cors;
use actix_web::{
    App, HttpRequest, HttpResponse, HttpServer, Responder, Result, error, get,
    http::StatusCode,
    middleware::Logger,
    post,
    web::{self, Data},
};
use derive_more::{Display, Error};
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
}

impl From<entity::user::Model> for UserResponse {
    fn from(u: entity::user::Model) -> Self {
        Self {
            id: u.id,
            username: u.username,
            email: u.email,
            email_verified: u.email_verified,
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
        }
    }
}

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
        .map(|(m, u)| MessageResponse {
            id: m.id,
            user_id: m.user_id,
            channel_id: m.channel_id,
            text: m.text,
            username: u.map(|u| u.username).unwrap_or_else(|| "[deleted]".into()),
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

    const DEV_SESSION_TOKEN: &str =
        "devdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdev";
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
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
                .service(me),
        );
}

pub async fn start_server(
    db: DatabaseConnection,
    broadcaster: Arc<Broadcaster>,
) -> std::io::Result<()> {
    let broadcaster_data = web::Data::from(broadcaster);
    let db_data = web::Data::new(db);

    // logger config - should review
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

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
            .configure(|cfg| configure_app(cfg, db_data.clone(), broadcaster_data.clone()))
    })
    .bind(("127.0.0.1", 3030))?
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
