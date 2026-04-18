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
    }
    .insert(&db)
    .await
    .unwrap();
    let a_id = hamlet::generate_id();
    entity::channel::ActiveModel {
        id: Set(a_id),
        name: Set("alpha".to_owned()),
        position: Set(1),
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
    }
    .insert(&db)
    .await
    .unwrap();
    let b_id = hamlet::generate_id();
    entity::channel::ActiveModel {
        id: Set(b_id),
        name: Set("bravo".to_owned()),
        position: Set(2),
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
