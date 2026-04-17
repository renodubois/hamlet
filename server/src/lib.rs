use std::sync::Arc;

pub mod auth;
pub mod broadcast;
pub mod entity;

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

    let messages = entity::message::Entity::find()
        .filter(entity::message::Column::ChannelId.eq(channel_id))
        .all(db.get_ref())
        .await
        .map_err(|_| UserError::DbError)?;

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

    broadcaster.broadcast(&body.text).await;

    Ok(web::Json(inserted))
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
    let user =
        auth::register_user(db.get_ref(), &body.username, &body.password, body.email.clone())
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

pub async fn seed_development_data(db: &DatabaseConnection) {
    db.get_schema_registry("hamlet::entity::*")
        .sync(db)
        .await
        .unwrap();

    // Seed default channel
    let general_channel = entity::channel::ActiveModel {
        id: Set(generate_id()),
        name: Set("general".to_owned()),
    };
    general_channel.insert(db).await.unwrap();
}

pub fn configure_app(
    cfg: &mut web::ServiceConfig,
    db_data: Data<DatabaseConnection>,
    broadcaster: Data<Broadcaster>,
) {
    cfg.app_data(db_data.clone())
        .app_data(broadcaster)
        .service(subscribe_to_channel_events)
        .service(get_channels)
        .service(get_messages)
        .service(create_message)
        .service(register)
        .service(login)
        .service(logout)
        .service(me);
}

pub async fn start_server(db: DatabaseConnection, broadcaster: Arc<Broadcaster>) -> std::io::Result<()> {
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

    use actix_web::http::{Method, header::ContentType};
    use actix_web::test;
    use sea_orm::Database;

    use super::*;

    async fn setup_db() -> (DatabaseConnection, i64) {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.get_schema_registry("disclone_server::entity::*")
            .sync(&db)
            .await
            .unwrap();

        let chan_id = generate_id();
        let test_channel = entity::channel::ActiveModel {
            id: Set(chan_id),
            name: Set("general".to_owned()),
        };
        test_channel.insert(&db).await.unwrap();

        (db, chan_id)
    }

    fn session_cookie_header(token: &str) -> (String, String) {
        (
            "Cookie".to_owned(),
            format!("{}={}", auth::SESSION_COOKIE, token),
        )
    }

    #[actix_web::test]
    async fn test_register_creates_user_and_sets_cookie() {
        let (db, _) = setup_db().await;
        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());

        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let body = serde_json::json!({"username": "alice", "password": "hunter2"});
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_payload(body.to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let cookie = resp
            .headers()
            .get("set-cookie")
            .expect("set-cookie header missing");
        assert!(cookie.to_str().unwrap().starts_with("session="));
    }

    #[actix_web::test]
    async fn test_register_rejects_duplicate_username() {
        let (db, _) = setup_db().await;
        auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let body = serde_json::json!({"username": "alice", "password": "other"});
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_payload(body.to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[actix_web::test]
    async fn test_login_succeeds_with_correct_password() {
        let (db, _) = setup_db().await;
        auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let body = serde_json::json!({"username": "alice", "password": "hunter2"});
        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .set_payload(body.to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_login_fails_with_wrong_password() {
        let (db, _) = setup_db().await;
        auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let body = serde_json::json!({"username": "alice", "password": "nope"});
        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .set_payload(body.to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_me_returns_current_user() {
        let (db, _) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::get()
            .uri("/me")
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body = test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["username"], "alice");
    }

    #[actix_web::test]
    async fn test_me_without_cookie_is_unauthorized() {
        let (db, _) = setup_db().await;
        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let req = test::TestRequest::get().uri("/me").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_logout_destroys_session() {
        let (db, _) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();
        let token = session.token.clone();

        let db_data = web::Data::new(db.clone());
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let (name, value) = session_cookie_header(&token);
        let req = test::TestRequest::post()
            .uri("/logout")
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let remaining = entity::session::Entity::find_by_id(token)
            .one(&db)
            .await
            .unwrap();
        assert!(remaining.is_none());
    }

    #[actix_web::test]
    async fn test_message_create_requires_auth() {
        let (db, chan_id) = setup_db().await;

        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let body = serde_json::to_string(&SendMessageRequest {
            text: "hi".to_string(),
        })
        .unwrap();
        let req = test::TestRequest::post()
            .uri(&format!("/message/{}", chan_id))
            .insert_header(ContentType::json())
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_message_create() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let db_data = web::Data::new(db);
        let broadcaster = Broadcaster::create();
        let broadcaster_data = web::Data::from(broadcaster);

        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let body = serde_json::to_string(&SendMessageRequest {
            text: "test message!".to_string(),
        })
        .unwrap();

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::with_uri(format!("/message/{:?}", chan_id).as_str())
            .method(Method::POST)
            .insert_header(ContentType::json())
            .insert_header((name, value))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_login_fails_with_nonexistent_username() {
        let (db, _) = setup_db().await;
        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let body = serde_json::json!({"username": "ghost", "password": "whatever"});
        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .set_payload(body.to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_expired_session_is_unauthorized() {
        let (db, _) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let expired = entity::session::ActiveModel {
            token: Set("expired-token".to_owned()),
            user_id: Set(user.id),
            created_at: Set(0),
            expires_at: Set(1),
        };
        expired.insert(&db).await.unwrap();

        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let (name, value) = session_cookie_header("expired-token");
        let req = test::TestRequest::get()
            .uri("/me")
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_me_after_logout_is_unauthorized() {
        let (db, _) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();
        let token = session.token.clone();

        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let (name, value) = session_cookie_header(&token);
        let logout_req = test::TestRequest::post()
            .uri("/logout")
            .insert_header((name.clone(), value.clone()))
            .to_request();
        let logout_resp = test::call_service(&app, logout_req).await;
        assert!(logout_resp.status().is_success());

        let me_req = test::TestRequest::get()
            .uri("/me")
            .insert_header((name, value))
            .to_request();
        let me_resp = test::call_service(&app, me_req).await;
        assert_eq!(me_resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_logout_without_cookie_is_ok_and_clears() {
        let (db, _) = setup_db().await;
        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let req = test::TestRequest::post().uri("/logout").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let cookie = resp
            .headers()
            .get("set-cookie")
            .expect("set-cookie header missing");
        assert!(cookie.to_str().unwrap().starts_with("session="));
    }

    #[actix_web::test]
    async fn test_logout_with_bad_cookie_is_ok_and_clears() {
        let (db, _) = setup_db().await;
        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let (name, value) = session_cookie_header("not-a-real-token");
        let req = test::TestRequest::post()
            .uri("/logout")
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_register_rejects_empty_username() {
        let (db, _) = setup_db().await;
        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let body = serde_json::json!({"username": "", "password": "hunter2"});
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_payload(body.to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_register_rejects_empty_password() {
        let (db, _) = setup_db().await;
        let db_data = web::Data::new(db);
        let broadcaster_data = web::Data::from(Broadcaster::new());
        let app = test::init_service(
            App::new().configure(|cfg| configure_app(cfg, db_data, broadcaster_data)),
        )
        .await;

        let body = serde_json::json!({"username": "alice", "password": ""});
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_payload(body.to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_register_stores_hashed_password() {
        let (db, _) = setup_db().await;
        auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let credential = entity::credential::Entity::find()
            .filter(entity::credential::Column::Provider.eq(auth::PASSWORD_PROVIDER))
            .filter(entity::credential::Column::ExternalId.eq("alice"))
            .one(&db)
            .await
            .unwrap()
            .expect("credential should exist");

        let secret = credential.secret.expect("password credential must have a secret");
        assert_ne!(secret, "hunter2");
        assert!(secret.starts_with("$argon2"));
        assert!(auth::verify_password("hunter2", &secret));
    }

    #[actix_web::test]
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
            "broadcast event should contain the message text"
        );
    }
}
