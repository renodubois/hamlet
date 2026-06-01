#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use actix_web::{
    App,
    http::{StatusCode, header::ContentType},
    test,
};
use common::{TestCtx, insert_message};
use hamlet::{configure_app, entity, generate_id, now_unix_micros};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

async fn insert_message_with_parent(
    db: &sea_orm::DatabaseConnection,
    user_id: i64,
    channel_id: i64,
    parent_id: Option<i64>,
    created_at: i64,
    text: &str,
) -> i64 {
    let id = generate_id();
    entity::message::ActiveModel {
        id: Set(id),
        user_id: Set(user_id),
        channel_id: Set(channel_id),
        parent_id: Set(parent_id),
        created_at: Set(created_at),
        deleted_at: Set(None),
        text: Set(text.to_owned()),
        suppress_embeds: Set(false),
    }
    .insert(db)
    .await
    .unwrap();
    id
}

#[actix_web::test]
async fn test_message_create_requires_auth() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
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
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "test message!"}).to_string())
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());
}

#[actix_web::test]
async fn test_thread_reply_create_and_history_exclusion() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message(&ctx.db, alice.user_id, chan_id, "root message").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/thread/{root_id}/reply"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "thread reply"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let reply_json: serde_json::Value =
        serde_json::from_slice(&test::read_body(resp).await).unwrap();
    let reply_id = reply_json["id"].as_i64().unwrap();
    assert_eq!(reply_json["parent_id"], root_id);
    assert_eq!(reply_json["channel_id"], chan_id);
    assert_eq!(reply_json["text"], "thread reply");

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    let rows = history.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], root_id);

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(thread["root"]["id"], root_id);
    assert_eq!(thread["root"]["text"], "root message");
    let replies = thread["replies"].as_array().unwrap();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0]["id"], reply_id);
    assert_eq!(replies[0]["parent_id"], root_id);
    assert_eq!(replies[0]["text"], "thread reply");
}

#[actix_web::test]
async fn test_thread_get_defaults_to_newest_50_replies_and_loads_older_pages() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "root message",
    )
    .await;

    let mut reply_ids = Vec::new();
    for index in 1..=55 {
        let reply_id = insert_message_with_parent(
            &ctx.db,
            alice.user_id,
            chan_id,
            Some(root_id),
            1_700_000_000_000_000 + index,
            &format!("reply {index}"),
        )
        .await;
        reply_ids.push(reply_id);
    }

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    let replies = thread["replies"].as_array().unwrap();
    assert_eq!(replies.len(), 50);
    assert_eq!(thread["has_more_replies"], true);
    assert_eq!(replies[0]["text"], "reply 6");
    assert_eq!(replies[49]["text"], "reply 55");

    let oldest_created_at = replies[0]["created_at"].as_i64().unwrap();
    let oldest_id = replies[0]["id"].as_i64().unwrap();
    let req = test::TestRequest::get()
        .uri(&format!(
            "/thread/{root_id}?before_created_at={oldest_created_at}&before_id={oldest_id}"
        ))
        .insert_header(alice.cookie_header())
        .to_request();
    let older_thread: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    let older_replies = older_thread["replies"].as_array().unwrap();
    assert_eq!(older_replies.len(), 5);
    assert_eq!(older_thread["has_more_replies"], false);
    assert_eq!(older_replies[0]["text"], "reply 1");
    assert_eq!(older_replies[4]["text"], "reply 5");

    let returned_older_ids: Vec<i64> = older_replies
        .iter()
        .map(|reply| reply["id"].as_i64().unwrap())
        .collect();
    assert_eq!(returned_older_ids, reply_ids[..5].to_vec());
}

#[actix_web::test]
async fn test_thread_get_honors_reply_limit() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message(&ctx.db, alice.user_id, chan_id, "root message").await;

    for index in 1..=4 {
        insert_message_with_parent(
            &ctx.db,
            alice.user_id,
            chan_id,
            Some(root_id),
            1_700_000_000_000_000 + index,
            &format!("reply {index}"),
        )
        .await;
    }

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}?limit=2"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    let replies = thread["replies"].as_array().unwrap();
    assert_eq!(replies.len(), 2);
    assert_eq!(thread["has_more_replies"], true);
    assert_eq!(replies[0]["text"], "reply 3");
    assert_eq!(replies[1]["text"], "reply 4");
}

#[actix_web::test]
async fn test_channel_history_includes_thread_summaries_for_roots_with_replies() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let root_with_replies = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "root with replies",
    )
    .await;
    let root_without_replies = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_001_000_000,
        "quiet root",
    )
    .await;
    insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_with_replies),
        1_700_000_010_000_000,
        "older reply",
    )
    .await;
    insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_with_replies),
        1_700_000_020_000_000,
        "newest reply",
    )
    .await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    let rows = history.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], root_with_replies);
    assert_eq!(rows[0]["thread_summary"]["reply_count"], 2);
    assert_eq!(
        rows[0]["thread_summary"]["last_reply_created_at"],
        1_700_000_020_000_000_i64
    );
    assert_eq!(rows[1]["id"], root_without_replies);
    assert!(rows[1].get("thread_summary").is_none());
}

#[actix_web::test]
async fn test_channel_history_thread_summary_recalculates_after_reply_deletes() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "root",
    )
    .await;
    let older_reply = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        1_700_000_010_000_000,
        "older reply",
    )
    .await;
    let newer_reply = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        1_700_000_020_000_000,
        "newer reply",
    )
    .await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{newer_reply}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(history[0]["thread_summary"]["reply_count"], 1);
    assert_eq!(
        history[0]["thread_summary"]["last_reply_created_at"],
        1_700_000_010_000_000_i64
    );

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{older_reply}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert!(history[0].get("thread_summary").is_none());
}

#[actix_web::test]
async fn test_thread_reply_lifecycle_permissions_embeds_and_deletion() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let root_id = insert_message(&ctx.db, alice.user_id, chan_id, "root message").await;
    let reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        1_700_000_010_000_000,
        "reply with https://example.com",
    )
    .await;

    entity::embed::ActiveModel {
        id: Set(generate_id()),
        message_id: Set(reply_id),
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
    .insert(&ctx.db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::put()
        .uri(&format!("/message/{reply_id}"))
        .insert_header(ContentType::json())
        .insert_header(bob.cookie_header())
        .set_payload(serde_json::json!({"text": "hijacked"}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::FORBIDDEN
    );

    let req = test::TestRequest::put()
        .uri(&format!("/message/{reply_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "edited reply https://example.com"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let edited: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(edited["text"], "edited reply https://example.com");
    assert_eq!(edited["parent_id"], root_id);

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(
        thread["replies"][0]["text"],
        "edited reply https://example.com"
    );
    assert_eq!(thread["replies"][0]["suppress_embeds"], false);
    assert_eq!(thread["replies"][0]["embeds"].as_array().unwrap().len(), 1);

    let req = test::TestRequest::post()
        .uri(&format!("/message/{reply_id}/suppress_embeds"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"suppress": true}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let suppressed: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(suppressed["suppress_embeds"], true);
    assert_eq!(suppressed["embeds"].as_array().unwrap().len(), 1);

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{reply_id}"))
        .insert_header(bob.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::FORBIDDEN
    );

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{reply_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert!(thread["replies"].as_array().unwrap().is_empty());

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert!(history[0].get("thread_summary").is_none());
}

#[actix_web::test]
async fn test_deleting_root_with_replies_tombstones_root_and_preserves_thread() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let root_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "root to delete",
    )
    .await;
    insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        Some(root_id),
        1_700_000_010_000_000,
        "first preserved reply",
    )
    .await;
    insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        Some(root_id),
        1_700_000_020_000_000,
        "second preserved reply",
    )
    .await;

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{root_id}"))
        .insert_header(bob.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::FORBIDDEN
    );

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    let stored = entity::message::Entity::find_by_id(root_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.deleted_at.is_some());
    assert_eq!(stored.text, "");
    assert!(stored.suppress_embeds);

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(history.as_array().unwrap().len(), 1);
    assert_eq!(history[0]["id"], root_id);
    assert!(history[0]["deleted_at"].as_i64().is_some());
    assert_eq!(history[0]["text"], "");
    assert_eq!(history[0]["thread_summary"]["reply_count"], 2);

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(thread["root"]["id"], root_id);
    assert!(thread["root"]["deleted_at"].as_i64().is_some());
    assert_eq!(thread["root"]["text"], "");
    let replies = thread["replies"].as_array().unwrap();
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0]["text"], "first preserved reply");
    assert_eq!(replies[1]["text"], "second preserved reply");
}

#[actix_web::test]
async fn test_participated_threads_filters_sorts_truncates_and_includes_tombstones() {
    let ctx = TestCtx::new().await;
    let general_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let charlie = ctx.register("charlie", "hunter2").await;
    let random_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(random_id),
        name: Set("random".to_owned()),
        position: Set(1),
        channel_type: Set("text".to_owned()),
    }
    .insert(&ctx.db)
    .await
    .unwrap();

    let tombstoned_root_id = generate_id();
    entity::message::ActiveModel {
        id: Set(tombstoned_root_id),
        user_id: Set(alice.user_id),
        channel_id: Set(general_id),
        parent_id: Set(None),
        created_at: Set(1_700_000_000_000_000),
        deleted_at: Set(Some(1_700_000_005_000_000)),
        text: Set(String::new()),
        suppress_embeds: Set(true),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        general_id,
        Some(tombstoned_root_id),
        1_700_000_010_000_000,
        "reply to deleted root",
    )
    .await;

    let reply_participated_root_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        random_id,
        None,
        1_700_000_020_000_000,
        "root alice replied to",
    )
    .await;
    for (author_id, created_at, text) in [
        (bob.user_id, 1_700_000_021_000_000, "old preview reply"),
        (alice.user_id, 1_700_000_022_000_000, "alice participated"),
        (charlie.user_id, 1_700_000_023_000_000, "third newest"),
        (bob.user_id, 1_700_000_024_000_000, "second newest"),
        (alice.user_id, 1_700_000_025_000_000, "newest preview reply"),
    ] {
        insert_message_with_parent(
            &ctx.db,
            author_id,
            random_id,
            Some(reply_participated_root_id),
            created_at,
            text,
        )
        .await;
    }

    let bob_only_root_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        general_id,
        None,
        1_700_000_030_000_000,
        "bob-only root",
    )
    .await;
    insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        general_id,
        Some(bob_only_root_id),
        1_700_000_040_000_000,
        "bob-only newest reply",
    )
    .await;

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;
    let req = test::TestRequest::get()
        .uri("/threads/participated")
        .insert_header(alice.cookie_header())
        .to_request();
    let previews: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let rows = previews.as_array().unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["root"]["id"], reply_participated_root_id);
    assert_eq!(rows[0]["channel"]["name"], "random");
    assert_eq!(rows[0]["reply_count"], 5);
    assert_eq!(rows[0]["last_reply_created_at"], 1_700_000_025_000_000_i64);
    let recent: Vec<&str> = rows[0]["recent_replies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|reply| reply["text"].as_str().unwrap())
        .collect();
    assert_eq!(
        recent,
        vec!["third newest", "second newest", "newest preview reply"]
    );
    assert!(!recent.contains(&"old preview reply"));

    assert_eq!(rows[1]["root"]["id"], tombstoned_root_id);
    assert_eq!(rows[1]["channel"]["name"], "general");
    assert_eq!(rows[1]["root"]["text"], "");
    assert!(rows[1]["root"]["deleted_at"].as_i64().is_some());
    assert_eq!(
        rows[1]["recent_replies"][0]["text"],
        "reply to deleted root"
    );

    let serialized = serde_json::to_string(&previews).unwrap();
    assert!(!serialized.contains("bob-only root"));
    assert!(!serialized.contains("bob-only newest reply"));
}

#[actix_web::test]
async fn test_thread_reply_rejects_missing_root() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri("/thread/999999999999999/reply")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "nope"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn test_thread_reply_rejects_reply_as_root() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message(&ctx.db, alice.user_id, chan_id, "root").await;
    let reply_id = generate_id();
    entity::message::ActiveModel {
        id: Set(reply_id),
        user_id: Set(alice.user_id),
        channel_id: Set(chan_id),
        parent_id: Set(Some(root_id)),
        created_at: Set(now_unix_micros()),
        deleted_at: Set(None),
        text: Set("existing reply".to_owned()),
        suppress_embeds: Set(false),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/thread/{reply_id}/reply"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "nested"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn test_thread_reply_rejects_non_text_channel_root() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let voice_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(voice_id),
        name: Set("voice".to_owned()),
        position: Set(1),
        channel_type: Set("voice".to_owned()),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    let root_id = insert_message(&ctx.db, alice.user_id, voice_id, "voice root").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/thread/{root_id}/reply"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "unsupported"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn test_messages_include_display_name_when_set() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    // Set display name first.
    let req = test::TestRequest::put()
        .uri("/me")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"display_name": "Ally"}).to_string())
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());

    // Post a message — the response should carry the display name.
    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "hi"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    assert_eq!(json["display_name"], "Ally");
    assert_eq!(json["username"], "alice");

    // Listing messages returns the same display name.
    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let json: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    let rows = json.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["display_name"], "Ally");
    assert_eq!(rows[0]["username"], "alice");
}

// --- message edit ---

#[actix_web::test]
async fn test_message_edit_requires_auth() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

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
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let msg_id = insert_message(&ctx.db, alice.user_id, chan_id, "original").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::put()
        .uri(&format!("/message/{msg_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "edited"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body = test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["text"], "edited");
    assert_eq!(json["id"], msg_id);

    let stored = entity::message::Entity::find_by_id(msg_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.text, "edited");
}

#[actix_web::test]
async fn test_message_edit_rejects_other_users_messages() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let mallory = ctx.register("mallory", "hunter2").await;
    let msg_id = insert_message(&ctx.db, alice.user_id, chan_id, "original").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::put()
        .uri(&format!("/message/{msg_id}"))
        .insert_header(ContentType::json())
        .insert_header(mallory.cookie_header())
        .set_payload(serde_json::json!({"text": "hijacked"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let stored = entity::message::Entity::find_by_id(msg_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.text, "original");
}

// --- message delete ---

#[actix_web::test]
async fn test_message_delete_requires_auth() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

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
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let msg_id = insert_message(&ctx.db, alice.user_id, chan_id, "hi").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{msg_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let stored = entity::message::Entity::find_by_id(msg_id)
        .one(&ctx.db)
        .await
        .unwrap();
    assert!(stored.is_none());
}

#[actix_web::test]
async fn test_message_delete_rejects_other_users_messages() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let mallory = ctx.register("mallory", "hunter2").await;
    let msg_id = insert_message(&ctx.db, alice.user_id, chan_id, "untouchable").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{msg_id}"))
        .insert_header(mallory.cookie_header())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let stored = entity::message::Entity::find_by_id(msg_id)
        .one(&ctx.db)
        .await
        .unwrap();
    assert!(stored.is_some());
}

#[actix_web::test]
async fn test_message_delete_returns_404_for_unknown_message() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::delete()
        .uri("/message/1234567890123456")
        .insert_header(alice.cookie_header())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn test_message_edit_returns_404_for_unknown_message() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::put()
        .uri("/message/1234567890123456")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "oops"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// --- embeds / suppress_embeds ---

#[actix_web::test]
async fn test_message_create_returns_empty_embeds_and_not_suppressed() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "hello https://example.com"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    // Fetch is disabled in tests, so even though the text has a URL the
    // response still returns an empty embed list.
    assert_eq!(body["suppress_embeds"], false);
    assert!(body["embeds"].is_array());
    assert_eq!(body["embeds"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn test_suppress_embeds_toggles_flag_and_returns_state() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let msg_id = insert_message(
        &ctx.db,
        alice.user_id,
        chan_id,
        "check this https://example.com",
    )
    .await;

    let embed_id = generate_id();
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
    .insert(&ctx.db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{msg_id}/suppress_embeds"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"suppress": true}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["suppress_embeds"], true);
    assert_eq!(body["id"], msg_id);
    assert_eq!(body["embeds"].as_array().unwrap().len(), 1);

    let stored = entity::message::Entity::find_by_id(msg_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.suppress_embeds);

    // Un-suppressing works too.
    let req = test::TestRequest::post()
        .uri(&format!("/message/{msg_id}/suppress_embeds"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"suppress": false}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["suppress_embeds"], false);
}

#[actix_web::test]
async fn test_suppress_embeds_rejects_other_users_messages() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let mallory = ctx.register("mallory", "hunter2").await;
    let msg_id = insert_message(&ctx.db, alice.user_id, chan_id, "hi").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{msg_id}/suppress_embeds"))
        .insert_header(ContentType::json())
        .insert_header(mallory.cookie_header())
        .set_payload(serde_json::json!({"suppress": true}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn test_suppress_embeds_requires_auth() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

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
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri("/message/99999/suppress_embeds")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"suppress": true}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[actix_web::test]
async fn test_get_messages_surfaces_embeds() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let msg_id = insert_message(&ctx.db, alice.user_id, chan_id, "https://example.com").await;

    let embed_id = generate_id();
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
    .insert(&ctx.db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
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
