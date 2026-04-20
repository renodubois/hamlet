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
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let req = test::TestRequest::post()
        .uri("/register")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"username": "alice", "password": "hunter2"}).to_string())
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
    let (db, _) = common::setup_db().await;
    auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
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
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
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
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
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

    let secret = credential
        .secret
        .expect("password credential must have a secret");
    assert_ne!(secret, "hunter2");
    assert!(secret.starts_with("$argon2"));
    assert!(auth::verify_password("hunter2", &secret));
}

// --- login ---

#[actix_web::test]
async fn test_login_succeeds_with_correct_password() {
    let (db, _) = common::setup_db().await;
    auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
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
    auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
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
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
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
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();
    let token = session.token.clone();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(
            cfg,
            web::Data::new(db.clone()),
            web::Data::from(Broadcaster::new()),
        )
    }))
    .await;

    let (name, value) = common::session_cookie_header(&token);
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
async fn test_logout_without_cookie_is_ok_and_clears() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
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
    let (db, _) = common::setup_db().await;
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
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
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
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
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let req = test::TestRequest::get().uri("/me").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_me_after_logout_is_unauthorized() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();
    let token = session.token.clone();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&token);
    let logout_req = test::TestRequest::post()
        .uri("/logout")
        .insert_header((name.clone(), value.clone()))
        .to_request();
    assert!(
        test::call_service(&app, logout_req)
            .await
            .status()
            .is_success()
    );

    let me_req = test::TestRequest::get()
        .uri("/me")
        .insert_header((name, value))
        .to_request();
    assert_eq!(
        test::call_service(&app, me_req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

// --- display name ---

#[actix_web::test]
async fn test_me_returns_null_display_name_initially() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header((name, value))
        .to_request();
    let body = test::read_body(test::call_service(&app, req).await).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["display_name"].is_null());
}

#[actix_web::test]
async fn test_update_me_sets_and_clears_display_name() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    // Set a display name.
    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::put()
        .uri("/me")
        .insert_header(ContentType::json())
        .insert_header((name.clone(), value.clone()))
        .set_payload(serde_json::json!({"display_name": "Alice Wonderland"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    assert_eq!(json["display_name"], "Alice Wonderland");
    assert_eq!(json["username"], "alice");

    // /me reflects the new value.
    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header((name.clone(), value.clone()))
        .to_request();
    let json: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(json["display_name"], "Alice Wonderland");

    // Clear it back to null by sending null.
    let req = test::TestRequest::put()
        .uri("/me")
        .insert_header(ContentType::json())
        .insert_header((name.clone(), value.clone()))
        .set_payload(serde_json::json!({"display_name": null}).to_string())
        .to_request();
    let json: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert!(json["display_name"].is_null());

    // Whitespace-only is also treated as clear.
    let req = test::TestRequest::put()
        .uri("/me")
        .insert_header(ContentType::json())
        .insert_header((name.clone(), value.clone()))
        .set_payload(serde_json::json!({"display_name": "   "}).to_string())
        .to_request();
    let json: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert!(json["display_name"].is_null());
}

#[actix_web::test]
async fn test_update_me_rejects_overlong_display_name() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    // 65 chars — one over the 64-char limit.
    let long = "a".repeat(65);
    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::put()
        .uri("/me")
        .insert_header(ContentType::json())
        .insert_header((name, value))
        .set_payload(serde_json::json!({"display_name": long}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn test_update_me_requires_auth() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let req = test::TestRequest::put()
        .uri("/me")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"display_name": "anyone"}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn test_messages_include_display_name_when_set() {
    let (db, chan_id) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(
            cfg,
            web::Data::new(db.clone()),
            web::Data::from(Broadcaster::new()),
        )
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);

    // Set display name first.
    let req = test::TestRequest::put()
        .uri("/me")
        .insert_header(ContentType::json())
        .insert_header((name.clone(), value.clone()))
        .set_payload(serde_json::json!({"display_name": "Ally"}).to_string())
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());

    // Post a message — the response should carry the display name.
    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .insert_header((name.clone(), value.clone()))
        .set_payload(serde_json::json!({"text": "hi"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    assert_eq!(json["display_name"], "Ally");
    assert_eq!(json["username"], "alice");

    // Listing messages returns the same display name.
    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header((name, value))
        .to_request();
    let json: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    let rows = json.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["display_name"], "Ally");
    assert_eq!(rows[0]["username"], "alice");
}

// --- session expiry ---

#[actix_web::test]
async fn test_expired_session_is_unauthorized() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();

    entity::session::ActiveModel {
        token: Set("expired-token".to_owned()),
        user_id: Set(user.id),
        created_at: Set(0),
        expires_at: Set(1),
    }
    .insert(&db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header("expired-token");
    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header((name, value))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

// --- auth-gated routes ---

#[actix_web::test]
async fn test_get_channels_requires_auth() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let req = test::TestRequest::get().uri("/channels").to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn test_get_channels_with_auth() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
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
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{}", chan_id))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn test_subscribe_requires_auth() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let req = test::TestRequest::get()
        .uri("/messages/subscribe")
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

// --- message creation ---

#[actix_web::test]
async fn test_message_create_requires_auth() {
    let (db, chan_id) = common::setup_db().await;
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", chan_id))
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"text": "hi"}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn test_message_create() {
    let (db, chan_id) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
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

// --- message edit ---

async fn insert_message(
    db: &sea_orm::DatabaseConnection,
    user_id: i64,
    channel_id: i64,
    text: &str,
) -> i64 {
    let id = hamlet::generate_id();
    entity::message::ActiveModel {
        id: Set(id),
        user_id: Set(user_id),
        channel_id: Set(channel_id),
        text: Set(text.to_owned()),
        suppress_embeds: Set(false),
    }
    .insert(db)
    .await
    .unwrap();
    id
}

#[actix_web::test]
async fn test_message_edit_requires_auth() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let req = test::TestRequest::put()
        .uri("/message/12345")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"text": "new"}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn test_message_edit_updates_text_and_returns_updated() {
    let (db, chan_id) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();
    let msg_id = insert_message(&db, user.id, chan_id, "original").await;

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(
            cfg,
            web::Data::new(db.clone()),
            web::Data::from(Broadcaster::new()),
        )
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::put()
        .uri(&format!("/message/{}", msg_id))
        .insert_header(ContentType::json())
        .insert_header((name, value))
        .set_payload(serde_json::json!({"text": "edited"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body = test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["text"], "edited");
    assert_eq!(json["id"], msg_id);

    let stored = entity::message::Entity::find_by_id(msg_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.text, "edited");
}

#[actix_web::test]
async fn test_message_edit_rejects_other_users_messages() {
    let (db, chan_id) = common::setup_db().await;
    let author = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let intruder = auth::register_user(&db, "mallory", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, intruder.id).await.unwrap();
    let msg_id = insert_message(&db, author.id, chan_id, "original").await;

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(
            cfg,
            web::Data::new(db.clone()),
            web::Data::from(Broadcaster::new()),
        )
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::put()
        .uri(&format!("/message/{}", msg_id))
        .insert_header(ContentType::json())
        .insert_header((name, value))
        .set_payload(serde_json::json!({"text": "hijacked"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let stored = entity::message::Entity::find_by_id(msg_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.text, "original");
}

// --- message delete ---

#[actix_web::test]
async fn test_message_delete_requires_auth() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let req = test::TestRequest::delete()
        .uri("/message/12345")
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn test_message_delete_removes_own_message() {
    let (db, chan_id) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();
    let msg_id = insert_message(&db, user.id, chan_id, "hi").await;

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(
            cfg,
            web::Data::new(db.clone()),
            web::Data::from(Broadcaster::new()),
        )
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::delete()
        .uri(&format!("/message/{}", msg_id))
        .insert_header((name, value))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let stored = entity::message::Entity::find_by_id(msg_id)
        .one(&db)
        .await
        .unwrap();
    assert!(stored.is_none());
}

#[actix_web::test]
async fn test_message_delete_rejects_other_users_messages() {
    let (db, chan_id) = common::setup_db().await;
    let author = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let intruder = auth::register_user(&db, "mallory", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, intruder.id).await.unwrap();
    let msg_id = insert_message(&db, author.id, chan_id, "untouchable").await;

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(
            cfg,
            web::Data::new(db.clone()),
            web::Data::from(Broadcaster::new()),
        )
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::delete()
        .uri(&format!("/message/{}", msg_id))
        .insert_header((name, value))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let stored = entity::message::Entity::find_by_id(msg_id)
        .one(&db)
        .await
        .unwrap();
    assert!(stored.is_some());
}

#[actix_web::test]
async fn test_message_delete_returns_404_for_unknown_message() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::delete()
        .uri("/message/1234567890123456")
        .insert_header((name, value))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn test_message_edit_returns_404_for_unknown_message() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::put()
        .uri("/message/1234567890123456")
        .insert_header(ContentType::json())
        .insert_header((name, value))
        .set_payload(serde_json::json!({"text": "oops"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// --- channel creation ---

#[actix_web::test]
async fn test_create_channel_requires_auth() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let req = test::TestRequest::post()
        .uri("/channel")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"name": "random"}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn test_create_channel_happy_path() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::post()
        .uri("/channel")
        .insert_header(ContentType::json())
        .insert_header((name.clone(), value.clone()))
        .set_payload(serde_json::json!({"name": "random"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body = test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "random");
    assert!(json["id"].is_number());

    // Verify it shows up in GET /channels
    let list_req = test::TestRequest::get()
        .uri("/channels")
        .insert_header((name, value))
        .to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert!(list_resp.status().is_success());
    let list_body = test::read_body(list_resp).await;
    let list_json: serde_json::Value = serde_json::from_slice(&list_body).unwrap();
    let names: Vec<&str> = list_json
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"random"));
}

#[actix_web::test]
async fn test_create_channel_trims_and_rejects_empty_name() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    for payload in ["", "   ", "\t\n"] {
        let req = test::TestRequest::post()
            .uri("/channel")
            .insert_header(ContentType::json())
            .insert_header((name.clone(), value.clone()))
            .set_payload(serde_json::json!({"name": payload}).to_string())
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::BAD_REQUEST,
            "name {payload:?} should be rejected",
        );
    }
}

// --- channel ordering & reordering ---

#[actix_web::test]
async fn test_get_channels_returns_them_ordered_by_position() {
    let (db, seeded) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    // Insert two more channels with explicit positions out of insertion order.
    let b_id = hamlet::generate_id();
    entity::channel::ActiveModel {
        id: Set(b_id),
        name: Set("bravo".to_owned()),
        position: Set(2),
        channel_type: Set("text".to_owned()),
    }
    .insert(&db)
    .await
    .unwrap();
    let a_id = hamlet::generate_id();
    entity::channel::ActiveModel {
        id: Set(a_id),
        name: Set("alpha".to_owned()),
        position: Set(1),
        channel_type: Set("text".to_owned()),
    }
    .insert(&db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::get()
        .uri("/channels")
        .insert_header((name, value))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body = test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let ids: Vec<i64> = json
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_i64().unwrap())
        .collect();
    // seeded "general" has position 0, alpha has 1, bravo has 2.
    assert_eq!(ids, vec![seeded, a_id, b_id]);
}

#[actix_web::test]
async fn test_create_channel_assigns_next_position() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    for expected_pos in 1..=2i64 {
        let req = test::TestRequest::post()
            .uri("/channel")
            .insert_header(ContentType::json())
            .insert_header((name.clone(), value.clone()))
            .set_payload(serde_json::json!({"name": format!("ch-{expected_pos}")}).to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
        assert_eq!(json["position"].as_i64().unwrap(), expected_pos);
    }
}

#[actix_web::test]
async fn test_reorder_channels_persists_new_order() {
    let (db, general_id) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let a_id = hamlet::generate_id();
    entity::channel::ActiveModel {
        id: Set(a_id),
        name: Set("alpha".to_owned()),
        position: Set(1),
        channel_type: Set("text".to_owned()),
    }
    .insert(&db)
    .await
    .unwrap();
    let b_id = hamlet::generate_id();
    entity::channel::ActiveModel {
        id: Set(b_id),
        name: Set("bravo".to_owned()),
        position: Set(2),
        channel_type: Set("text".to_owned()),
    }
    .insert(&db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    // Move bravo to the top, general to the bottom.
    let new_order = vec![b_id, a_id, general_id];
    let req = test::TestRequest::put()
        .uri("/channels/order")
        .insert_header(ContentType::json())
        .insert_header((name.clone(), value.clone()))
        .set_payload(serde_json::json!({"ids": new_order}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body = test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let returned_ids: Vec<i64> = json
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_i64().unwrap())
        .collect();
    assert_eq!(returned_ids, new_order);
    let returned_positions: Vec<i64> = json
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["position"].as_i64().unwrap())
        .collect();
    assert_eq!(returned_positions, vec![0, 1, 2]);

    // GET /channels now reflects the new order.
    let list_req = test::TestRequest::get()
        .uri("/channels")
        .insert_header((name, value))
        .to_request();
    let list_resp = test::call_service(&app, list_req).await;
    let list_body = test::read_body(list_resp).await;
    let list_json: serde_json::Value = serde_json::from_slice(&list_body).unwrap();
    let listed_ids: Vec<i64> = list_json
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_i64().unwrap())
        .collect();
    assert_eq!(listed_ids, new_order);
}

#[actix_web::test]
async fn test_reorder_channels_requires_auth() {
    let (db, chan_id) = common::setup_db().await;
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let req = test::TestRequest::put()
        .uri("/channels/order")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"ids": [chan_id]}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn test_reorder_channels_rejects_partial_id_set() {
    let (db, general_id) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let a_id = hamlet::generate_id();
    entity::channel::ActiveModel {
        id: Set(a_id),
        name: Set("alpha".to_owned()),
        position: Set(1),
        channel_type: Set("text".to_owned()),
    }
    .insert(&db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);

    // Missing one of the existing channels.
    let req = test::TestRequest::put()
        .uri("/channels/order")
        .insert_header(ContentType::json())
        .insert_header((name.clone(), value.clone()))
        .set_payload(serde_json::json!({"ids": [general_id]}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );

    // Contains an unknown channel id.
    let req = test::TestRequest::put()
        .uri("/channels/order")
        .insert_header(ContentType::json())
        .insert_header((name.clone(), value.clone()))
        .set_payload(serde_json::json!({"ids": [general_id, a_id, 999999]}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );

    // Duplicate ids.
    let req = test::TestRequest::put()
        .uri("/channels/order")
        .insert_header(ContentType::json())
        .insert_header((name, value))
        .set_payload(serde_json::json!({"ids": [general_id, general_id]}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );
}

#[actix_web::test]
async fn test_create_channel_defaults_to_text_type() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::post()
        .uri("/channel")
        .insert_header(ContentType::json())
        .insert_header((name, value))
        .set_payload(serde_json::json!({"name": "random"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body = test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "text");
}

#[actix_web::test]
async fn test_create_voice_channel() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::post()
        .uri("/channel")
        .insert_header(ContentType::json())
        .insert_header((name.clone(), value.clone()))
        .set_payload(serde_json::json!({"name": "lobby", "type": "voice"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    assert_eq!(json["type"], "voice");
    assert_eq!(json["name"], "lobby");

    // It's persisted and listed by /channels with its type.
    let list_req = test::TestRequest::get()
        .uri("/channels")
        .insert_header((name, value))
        .to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert!(list_resp.status().is_success());
    let list_json: serde_json::Value =
        serde_json::from_slice(&test::read_body(list_resp).await).unwrap();
    let types: Vec<&str> = list_json
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["type"].as_str().unwrap())
        .collect();
    assert!(types.contains(&"voice"));
}

#[actix_web::test]
async fn test_create_channel_rejects_unknown_type() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::post()
        .uri("/channel")
        .insert_header(ContentType::json())
        .insert_header((name, value))
        .set_payload(serde_json::json!({"name": "bad", "type": "video"}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );
}

#[actix_web::test]
async fn test_create_channel_rejects_too_long_name() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let long_name: String = "a".repeat(129);
    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::post()
        .uri("/channel")
        .insert_header(ContentType::json())
        .insert_header((name, value))
        .set_payload(serde_json::json!({"name": long_name}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );
}

// --- embeds / suppress_embeds ---

#[actix_web::test]
async fn test_message_create_returns_empty_embeds_and_not_suppressed() {
    let (db, chan_id) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(
            cfg,
            web::Data::new(db.clone()),
            web::Data::from(Broadcaster::new()),
        )
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", chan_id))
        .insert_header(ContentType::json())
        .insert_header((name, value))
        .set_payload(serde_json::json!({"text": "hello https://example.com"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    // Fetch is disabled in tests, so even though the text has a URL the
    // response still returns an empty embed list. What we care about here is
    // that the fields are present and well-typed.
    assert_eq!(body["suppress_embeds"], false);
    assert!(body["embeds"].is_array());
    assert_eq!(body["embeds"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn test_suppress_embeds_toggles_flag_and_returns_state() {
    let (db, chan_id) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();
    let msg_id = insert_message(&db, user.id, chan_id, "check this https://example.com").await;

    // Seed one embed row so we can see the flip reflected in what's returned.
    let embed_id = hamlet::generate_id();
    entity::embed::ActiveModel {
        id: Set(embed_id),
        message_id: Set(msg_id),
        url: Set("https://example.com".into()),
        title: Set(Some("Example".into())),
        description: Set(None),
        image_url: Set(None),
        site_name: Set(None),
        embed_type: Set("link".into()),
        iframe_url: Set(None),
        iframe_width: Set(None),
        iframe_height: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(
            cfg,
            web::Data::new(db.clone()),
            web::Data::from(Broadcaster::new()),
        )
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::post()
        .uri(&format!("/message/{}/suppress_embeds", msg_id))
        .insert_header(ContentType::json())
        .insert_header((name.clone(), value.clone()))
        .set_payload(serde_json::json!({"suppress": true}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["suppress_embeds"], true);
    assert_eq!(body["id"], msg_id);
    assert_eq!(body["embeds"].as_array().unwrap().len(), 1);

    // Row is persisted and the suppressed flag is set.
    let stored = entity::message::Entity::find_by_id(msg_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.suppress_embeds);

    // Un-suppressing works too.
    let req = test::TestRequest::post()
        .uri(&format!("/message/{}/suppress_embeds", msg_id))
        .insert_header(ContentType::json())
        .insert_header((name, value))
        .set_payload(serde_json::json!({"suppress": false}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["suppress_embeds"], false);
}

#[actix_web::test]
async fn test_suppress_embeds_rejects_other_users_messages() {
    let (db, chan_id) = common::setup_db().await;
    let author = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let intruder = auth::register_user(&db, "mallory", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, intruder.id).await.unwrap();
    let msg_id = insert_message(&db, author.id, chan_id, "hi").await;

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(
            cfg,
            web::Data::new(db.clone()),
            web::Data::from(Broadcaster::new()),
        )
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::post()
        .uri(&format!("/message/{}/suppress_embeds", msg_id))
        .insert_header(ContentType::json())
        .insert_header((name, value))
        .set_payload(serde_json::json!({"suppress": true}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn test_suppress_embeds_requires_auth() {
    let (db, _) = common::setup_db().await;
    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let req = test::TestRequest::post()
        .uri("/message/12345/suppress_embeds")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"suppress": true}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn test_suppress_embeds_returns_404_for_unknown_message() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::post()
        .uri("/message/99999/suppress_embeds")
        .insert_header(ContentType::json())
        .insert_header((name, value))
        .set_payload(serde_json::json!({"suppress": true}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[actix_web::test]
async fn test_get_messages_surfaces_embeds() {
    let (db, chan_id) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();
    let msg_id = insert_message(&db, user.id, chan_id, "https://example.com").await;

    let embed_id = hamlet::generate_id();
    entity::embed::ActiveModel {
        id: Set(embed_id),
        message_id: Set(msg_id),
        url: Set("https://example.com".into()),
        title: Set(Some("Example".into())),
        description: Set(Some("desc".into())),
        image_url: Set(Some("https://example.com/img.png".into())),
        site_name: Set(Some("Example Site".into())),
        embed_type: Set("video".into()),
        iframe_url: Set(Some("https://www.youtube.com/embed/abc".into())),
        iframe_width: Set(Some(560)),
        iframe_height: Set(Some(315)),
    }
    .insert(&db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| {
        configure_app(
            cfg,
            web::Data::new(db.clone()),
            web::Data::from(Broadcaster::new()),
        )
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::get()
        .uri(&format!("/messages/{}", chan_id))
        .insert_header((name, value))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    let msgs = body.as_array().unwrap();
    let msg = msgs.iter().find(|m| m["id"] == msg_id).unwrap();
    let embeds = msg["embeds"].as_array().unwrap();
    assert_eq!(embeds.len(), 1);
    assert_eq!(embeds[0]["title"], "Example");
    assert_eq!(embeds[0]["url"], "https://example.com");
    assert_eq!(embeds[0]["embed_type"], "video");
    assert_eq!(embeds[0]["iframe_url"], "https://www.youtube.com/embed/abc");
    assert_eq!(embeds[0]["iframe_width"], 560);
    assert_eq!(embeds[0]["iframe_height"], 315);
    assert_eq!(msg["suppress_embeds"], false);
}
