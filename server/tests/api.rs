#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use actix_web::{
    App,
    http::{StatusCode, header::ContentType},
    test, web,
};
use hamlet::{auth, broadcast::Broadcaster, configure_app, entity};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

// --- register ---

#[actix_web::test]
async fn test_register_creates_user_and_sets_cookie() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/register")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"username": "alice", "password": "hunter2"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let cookie = resp.headers().get("set-cookie").expect("set-cookie header missing");
    assert!(cookie.to_str().unwrap().starts_with("session="));
}

#[actix_web::test]
async fn test_register_rejects_duplicate_username() {
    let (db, _) = common::setup_db().await;
    auth::register_user(&db, "alice", "hunter2", None).await.unwrap();

    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/register")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"username": "alice", "password": "other"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[actix_web::test]
async fn test_register_rejects_empty_username() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/register")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"username": "", "password": "hunter2"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn test_register_rejects_empty_password() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/register")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"username": "alice", "password": ""}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn test_register_stores_hashed_password() {
    let (db, _) = common::setup_db().await;
    auth::register_user(&db, "alice", "hunter2", None).await.unwrap();

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

// --- login ---

#[actix_web::test]
async fn test_login_succeeds_with_correct_password() {
    let (db, _) = common::setup_db().await;
    auth::register_user(&db, "alice", "hunter2", None).await.unwrap();

    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/login")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"username": "alice", "password": "hunter2"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn test_login_fails_with_wrong_password() {
    let (db, _) = common::setup_db().await;
    auth::register_user(&db, "alice", "hunter2", None).await.unwrap();

    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/login")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"username": "alice", "password": "nope"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_login_fails_with_nonexistent_username() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/login")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"username": "ghost", "password": "whatever"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// --- logout ---

#[actix_web::test]
async fn test_logout_destroys_session() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None).await.unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();
    let token = session.token.clone();

    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db.clone()), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let (name, value) = common::session_cookie_header(&token);
    let req = test::TestRequest::post()
        .uri("/logout")
        .insert_header((name, value))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let remaining = entity::session::Entity::find_by_id(token).one(&db).await.unwrap();
    assert!(remaining.is_none());
}

#[actix_web::test]
async fn test_logout_without_cookie_is_ok_and_clears() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let req = test::TestRequest::post().uri("/logout").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let cookie = resp.headers().get("set-cookie").expect("set-cookie header missing");
    assert!(cookie.to_str().unwrap().starts_with("session="));
}

#[actix_web::test]
async fn test_logout_with_bad_cookie_is_ok_and_clears() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let (name, value) = common::session_cookie_header("not-a-real-token");
    let req = test::TestRequest::post()
        .uri("/logout")
        .insert_header((name, value))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}

// --- /me ---

#[actix_web::test]
async fn test_me_returns_current_user() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None).await.unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
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
    let (db, _) = common::setup_db().await;
    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let req = test::TestRequest::get().uri("/me").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_me_after_logout_is_unauthorized() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None).await.unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();
    let token = session.token.clone();

    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let (name, value) = common::session_cookie_header(&token);
    let logout_req = test::TestRequest::post()
        .uri("/logout")
        .insert_header((name.clone(), value.clone()))
        .to_request();
    assert!(test::call_service(&app, logout_req).await.status().is_success());

    let me_req = test::TestRequest::get()
        .uri("/me")
        .insert_header((name, value))
        .to_request();
    assert_eq!(test::call_service(&app, me_req).await.status(), StatusCode::UNAUTHORIZED);
}

// --- session expiry ---

#[actix_web::test]
async fn test_expired_session_is_unauthorized() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None).await.unwrap();

    entity::session::ActiveModel {
        token: Set("expired-token".to_owned()),
        user_id: Set(user.id),
        created_at: Set(0),
        expires_at: Set(1),
    }
    .insert(&db)
    .await
    .unwrap();

    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let (name, value) = common::session_cookie_header("expired-token");
    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header((name, value))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), StatusCode::UNAUTHORIZED);
}

// --- auth-gated routes ---

#[actix_web::test]
async fn test_get_channels_requires_auth() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let req = test::TestRequest::get().uri("/channels").to_request();
    assert_eq!(test::call_service(&app, req).await.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_get_channels_with_auth() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None).await.unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::get()
        .uri("/channels")
        .insert_header((name, value))
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());
}

#[actix_web::test]
async fn test_get_messages_requires_auth() {
    let (db, chan_id) = common::setup_db().await;
    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{}", chan_id))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_subscribe_requires_auth() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let req = test::TestRequest::get().uri("/messages/subscribe").to_request();
    assert_eq!(test::call_service(&app, req).await.status(), StatusCode::UNAUTHORIZED);
}

// --- message creation ---

#[actix_web::test]
async fn test_message_create_requires_auth() {
    let (db, chan_id) = common::setup_db().await;
    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", chan_id))
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"text": "hi"}).to_string())
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_message_create() {
    let (db, chan_id) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None).await.unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(
        App::new().configure(|cfg| configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))),
    )
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", chan_id))
        .insert_header(ContentType::json())
        .insert_header((name, value))
        .set_payload(serde_json::json!({"text": "test message!"}).to_string())
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());
}
