#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use actix_web::{App, http::StatusCode, test};
use common::TestCtx;
use hamlet::{configure_app, entity, generate_id};
use sea_orm::{ActiveModelTrait, Set};

async fn insert_user(
    ctx: &TestCtx,
    username: &str,
    display_name: Option<&str>,
    email: Option<&str>,
    email_verified: bool,
    with_avatar: bool,
) -> i64 {
    let id = generate_id();
    entity::user::ActiveModel {
        id: Set(id),
        username: Set(username.to_owned()),
        display_name: Set(display_name.map(str::to_owned)),
        email: Set(email.map(str::to_owned)),
        email_verified: Set(email_verified),
        avatar_path: Set(with_avatar.then(|| format!("avatars/{id}.webp"))),
        avatar_updated_at: Set(with_avatar.then_some(1_700_000_000)),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    id
}

fn usernames(body: &serde_json::Value) -> Vec<String> {
    body.as_array()
        .unwrap()
        .iter()
        .map(|user| user["username"].as_str().unwrap().to_owned())
        .collect()
}

#[actix_web::test]
async fn test_user_search_requires_auth() {
    let ctx = TestCtx::new().await;
    insert_user(&ctx, "alice", None, None, false, false).await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get().uri("/users?q=alice").to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn test_user_search_supports_empty_query_and_includes_current_user() {
    let ctx = TestCtx::new().await;
    let current = ctx.register("current", "hunter2").await;
    insert_user(&ctx, "alice", None, None, false, false).await;
    insert_user(&ctx, "bob", None, None, false, false).await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri("/users?limit=10")
        .insert_header(current.cookie_header())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;

    assert_eq!(usernames(&body), vec!["alice", "bob", "current"]);
}

#[actix_web::test]
async fn test_user_search_ranks_matches_case_insensitively() {
    let ctx = TestCtx::new().await;
    let current = ctx.register("current", "hunter2").await;
    insert_user(&ctx, "ALP", None, None, false, false).await;
    insert_user(&ctx, "hidden", Some("alp"), None, false, false).await;
    insert_user(&ctx, "alpha", None, None, false, false).await;
    insert_user(&ctx, "calpso", None, None, false, false).await;
    insert_user(&ctx, "a-l-p", None, None, false, false).await;
    insert_user(&ctx, "nomatch", Some("nothing"), None, false, false).await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri("/users?q=AlP&limit=10")
        .insert_header(current.cookie_header())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;

    assert_eq!(
        usernames(&body),
        vec!["ALP", "hidden", "alpha", "calpso", "a-l-p"]
    );
}

#[actix_web::test]
async fn test_user_search_caps_requested_limit() {
    let ctx = TestCtx::new().await;
    let current = ctx.register("current", "hunter2").await;
    for i in 0..55 {
        insert_user(&ctx, &format!("user-{i:03}"), None, None, false, false).await;
    }
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri("/users?limit=999")
        .insert_header(current.cookie_header())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;

    assert_eq!(body.as_array().unwrap().len(), 50);
}

#[actix_web::test]
async fn test_user_search_serializes_only_public_user_fields() {
    let ctx = TestCtx::new().await;
    let current = ctx.register("current", "hunter2").await;
    let private_id = insert_user(
        &ctx,
        "private",
        Some("Private Person"),
        Some("private@example.test"),
        true,
        true,
    )
    .await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri("/users?q=private&limit=1")
        .insert_header(current.cookie_header())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    let users = body.as_array().unwrap();
    assert_eq!(users.len(), 1);
    let user = users[0].as_object().unwrap();

    assert_eq!(user.len(), 4);
    assert_eq!(user["id"].as_i64(), Some(private_id));
    assert_eq!(user["username"].as_str(), Some("private"));
    assert_eq!(user["display_name"].as_str(), Some("Private Person"));
    let expected_avatar_url = format!("/uploads/avatars/{private_id}.webp?v=1700000000");
    assert_eq!(
        user["avatar_url"].as_str(),
        Some(expected_avatar_url.as_str())
    );
    assert!(!user.contains_key("email"));
    assert!(!user.contains_key("email_verified"));
    assert!(!user.contains_key("avatar_path"));
    assert!(!user.contains_key("credentials"));
    assert!(!user.contains_key("sessions"));
}
