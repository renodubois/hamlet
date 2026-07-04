#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use actix_web::{
    App,
    http::{StatusCode, header::ContentType},
    test, web,
};
use common::TestCtx;
use hamlet::{Config, auth, configure_app, entity};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

fn config_with_registration(account_registration_enabled: bool) -> Config {
    let mut config = Config::from_env();
    config.account_registration_enabled = account_registration_enabled;
    config
}

// --- public config ---

#[actix_web::test]
async fn test_public_config_reports_enabled_registration() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config_with_registration(true)))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::get().uri("/config").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    assert_eq!(body["account_registration_enabled"], true);
}

#[actix_web::test]
async fn test_public_config_reports_disabled_registration() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config_with_registration(false)))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::get().uri("/config").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    assert_eq!(body["account_registration_enabled"], false);
}

// --- register ---

#[actix_web::test]
async fn test_register_creates_user_and_sets_cookie() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config_with_registration(true)))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
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
    let ctx = TestCtx::new().await;
    auth::register_user(&ctx.db, "alice", "hunter2", None)
        .await
        .unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config_with_registration(true)))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
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
    let ctx = TestCtx::new().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config_with_registration(true)))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
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
    let ctx = TestCtx::new().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config_with_registration(true)))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
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
async fn test_register_returns_clear_error_when_registration_is_disabled() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config_with_registration(false)))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/register")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"username": "alice", "password": "hunter2"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(resp.headers().get("set-cookie").is_none());
    let body: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    assert_eq!(body["error"]["kind"], "registration_disabled");
    assert_eq!(body["error"]["message"], "account registration is disabled");

    let created = entity::user::Entity::find()
        .filter(entity::user::Column::Username.eq("alice"))
        .one(&ctx.db)
        .await
        .unwrap();
    assert!(created.is_none());
}

#[actix_web::test]
async fn test_login_still_works_when_registration_is_disabled() {
    let ctx = TestCtx::new().await;
    auth::register_user(&ctx.db, "alice", "hunter2", None)
        .await
        .unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config_with_registration(false)))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/login")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"username": "alice", "password": "hunter2"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
    assert!(resp.headers().get("set-cookie").is_some());
}

#[actix_web::test]
async fn test_register_stores_hashed_password() {
    let ctx = TestCtx::new().await;
    auth::register_user(&ctx.db, "alice", "hunter2", None)
        .await
        .unwrap();

    let credential = entity::credential::Entity::find()
        .filter(entity::credential::Column::Provider.eq(auth::PASSWORD_PROVIDER))
        .filter(entity::credential::Column::ExternalId.eq("alice"))
        .one(&ctx.db)
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
    let ctx = TestCtx::new().await;
    auth::register_user(&ctx.db, "alice", "hunter2", None)
        .await
        .unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

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
    let ctx = TestCtx::new().await;
    auth::register_user(&ctx.db, "alice", "hunter2", None)
        .await
        .unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

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
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

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
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let token = alice.token.clone();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri("/logout")
        .insert_header(alice.cookie_header())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let remaining = entity::session::Entity::find_by_id(token)
        .one(&ctx.db)
        .await
        .unwrap();
    assert!(remaining.is_none());
}

#[actix_web::test]
async fn test_logout_without_cookie_is_ok_and_clears() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

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
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri("/logout")
        .insert_header((
            "Cookie".to_owned(),
            format!("{}=not-a-real-token", auth::SESSION_COOKIE),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}

// --- /me ---

#[actix_web::test]
async fn test_me_returns_current_user() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header(alice.cookie_header())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body = test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["username"], "alice");
}

#[actix_web::test]
async fn test_me_without_cookie_is_unauthorized() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get().uri("/me").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_me_after_logout_is_unauthorized() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let logout_req = test::TestRequest::post()
        .uri("/logout")
        .insert_header(alice.cookie_header())
        .to_request();
    assert!(
        test::call_service(&app, logout_req)
            .await
            .status()
            .is_success()
    );

    let me_req = test::TestRequest::get()
        .uri("/me")
        .insert_header(alice.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, me_req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

// --- display name ---

#[actix_web::test]
async fn test_me_returns_null_display_name_initially() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header(alice.cookie_header())
        .to_request();
    let body = test::read_body(test::call_service(&app, req).await).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["display_name"].is_null());
}

#[actix_web::test]
async fn test_update_me_sets_and_clears_display_name() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    // Set a display name.
    let req = test::TestRequest::put()
        .uri("/me")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
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
        .insert_header(alice.cookie_header())
        .to_request();
    let json: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(json["display_name"], "Alice Wonderland");

    // Clear it back to null by sending null.
    let req = test::TestRequest::put()
        .uri("/me")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
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
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"display_name": "   "}).to_string())
        .to_request();
    let json: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert!(json["display_name"].is_null());
}

#[actix_web::test]
async fn test_update_me_rejects_overlong_display_name() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    // 65 chars — one over the 64-char limit.
    let long = "a".repeat(65);
    let req = test::TestRequest::put()
        .uri("/me")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"display_name": long}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn test_update_me_requires_auth() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

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

// --- password changes ---

#[actix_web::test]
async fn test_change_password_updates_credential_and_keeps_session() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::put()
        .uri("/me/password")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(
            serde_json::json!({"current_password": "hunter2", "new_password": "correct horse"})
                .to_string(),
        )
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let me_req = test::TestRequest::get()
        .uri("/me")
        .insert_header(alice.cookie_header())
        .to_request();
    assert!(test::call_service(&app, me_req).await.status().is_success());

    let old_login_req = test::TestRequest::post()
        .uri("/login")
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"username": "alice", "password": "hunter2"}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, old_login_req).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let new_login_req = test::TestRequest::post()
        .uri("/login")
        .insert_header(ContentType::json())
        .set_payload(
            serde_json::json!({"username": "alice", "password": "correct horse"}).to_string(),
        )
        .to_request();
    assert!(
        test::call_service(&app, new_login_req)
            .await
            .status()
            .is_success()
    );
}

#[actix_web::test]
async fn test_change_password_rejects_wrong_current_password_without_changing_credential() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::put()
        .uri("/me/password")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(
            serde_json::json!({"current_password": "wrong", "new_password": "correct horse"})
                .to_string(),
        )
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    assert!(
        auth::authenticate_password(&ctx.db, "alice", "hunter2")
            .await
            .is_ok()
    );
    assert!(
        auth::authenticate_password(&ctx.db, "alice", "correct horse")
            .await
            .is_err()
    );
}

#[actix_web::test]
async fn test_change_password_rejects_empty_password_fields() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    for body in [
        serde_json::json!({"current_password": "", "new_password": "newpass"}),
        serde_json::json!({"current_password": "hunter2", "new_password": ""}),
    ] {
        let req = test::TestRequest::put()
            .uri("/me/password")
            .insert_header(ContentType::json())
            .insert_header(alice.cookie_header())
            .set_payload(body.to_string())
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::BAD_REQUEST
        );
    }

    assert!(
        auth::authenticate_password(&ctx.db, "alice", "hunter2")
            .await
            .is_ok()
    );
}

#[actix_web::test]
async fn test_change_password_requires_auth() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::put()
        .uri("/me/password")
        .insert_header(ContentType::json())
        .set_payload(
            serde_json::json!({"current_password": "old", "new_password": "new"}).to_string(),
        )
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

// --- session expiry ---

#[actix_web::test]
async fn test_expired_session_is_unauthorized() {
    let ctx = TestCtx::new().await;
    let user = auth::register_user(&ctx.db, "alice", "hunter2", None)
        .await
        .unwrap();
    entity::session::ActiveModel {
        token: Set("expired-token".to_owned()),
        user_id: Set(user.id),
        created_at: Set(0),
        expires_at: Set(1),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header((
            "Cookie".to_owned(),
            format!("{}=expired-token", auth::SESSION_COOKIE),
        ))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

// --- auth-gated routes ---

#[actix_web::test]
async fn test_get_channels_requires_auth() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get().uri("/channels").to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn test_get_messages_requires_auth() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn test_subscribe_requires_auth() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri("/messages/subscribe")
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}
