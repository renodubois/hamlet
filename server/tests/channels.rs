#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use actix_web::{
    App,
    http::{StatusCode, header::ContentType},
    test,
};
use common::TestCtx;
use hamlet::{configure_app, entity, generate_id};
use sea_orm::{ActiveModelTrait, Set};

#[actix_web::test]
async fn test_get_channels_with_auth() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri("/channels")
        .insert_header(alice.cookie_header())
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());
}

#[actix_web::test]
async fn test_create_channel_requires_auth() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

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
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri("/channel")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
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
        .insert_header(alice.cookie_header())
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
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    for payload in ["", "   ", "\t\n"] {
        let req = test::TestRequest::post()
            .uri("/channel")
            .insert_header(ContentType::json())
            .insert_header(alice.cookie_header())
            .set_payload(serde_json::json!({"name": payload}).to_string())
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::BAD_REQUEST,
            "name {payload:?} should be rejected",
        );
    }
}

#[actix_web::test]
async fn test_get_channels_returns_them_ordered_by_position() {
    let ctx = TestCtx::new().await;
    let seeded = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;

    // Insert two more channels with explicit positions out of insertion order.
    let b_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(b_id),
        name: Set("bravo".to_owned()),
        position: Set(2),
        channel_type: Set("text".to_owned()),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    let a_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(a_id),
        name: Set("alpha".to_owned()),
        position: Set(1),
        channel_type: Set("text".to_owned()),
    }
    .insert(&ctx.db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri("/channels")
        .insert_header(alice.cookie_header())
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
    assert_eq!(ids, vec![seeded, a_id, b_id]);
}

#[actix_web::test]
async fn test_create_channel_assigns_next_position() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    for expected_pos in 1..=2i64 {
        let req = test::TestRequest::post()
            .uri("/channel")
            .insert_header(ContentType::json())
            .insert_header(alice.cookie_header())
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
    let ctx = TestCtx::new().await;
    let general_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;

    let a_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(a_id),
        name: Set("alpha".to_owned()),
        position: Set(1),
        channel_type: Set("text".to_owned()),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    let b_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(b_id),
        name: Set("bravo".to_owned()),
        position: Set(2),
        channel_type: Set("text".to_owned()),
    }
    .insert(&ctx.db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    // Move bravo to the top, general to the bottom.
    let new_order = vec![b_id, a_id, general_id];
    let req = test::TestRequest::put()
        .uri("/channels/order")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
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
        .insert_header(alice.cookie_header())
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
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

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
    let ctx = TestCtx::new().await;
    let general_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;

    let a_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(a_id),
        name: Set("alpha".to_owned()),
        position: Set(1),
        channel_type: Set("text".to_owned()),
    }
    .insert(&ctx.db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    // Missing one of the existing channels.
    let req = test::TestRequest::put()
        .uri("/channels/order")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
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
        .insert_header(alice.cookie_header())
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
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"ids": [general_id, general_id]}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );
}

#[actix_web::test]
async fn test_create_channel_defaults_to_text_type() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri("/channel")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
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
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri("/channel")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"name": "lobby", "type": "voice"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    assert_eq!(json["type"], "voice");
    assert_eq!(json["name"], "lobby");

    let list_req = test::TestRequest::get()
        .uri("/channels")
        .insert_header(alice.cookie_header())
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
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri("/channel")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"name": "bad", "type": "video"}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );
}

#[actix_web::test]
async fn test_create_channel_rejects_too_long_name() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let long_name: String = "a".repeat(129);
    let req = test::TestRequest::post()
        .uri("/channel")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"name": long_name}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );
}
