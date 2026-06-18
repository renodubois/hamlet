#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use actix_web::{
    App,
    http::{StatusCode, header::ContentType},
    test, web,
};
use common::{TestCtx, insert_message, make_tmp_uploads_dir};
use hamlet::{AttachmentStorage, configure_app, entity, generate_id, mentions, now_unix_micros};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder, Set,
};

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
        reply_to_message_id: Set(None),
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

async fn insert_inline_reply_with_target(
    db: &sea_orm::DatabaseConnection,
    user_id: i64,
    channel_id: i64,
    target_id: i64,
    created_at: i64,
    text: &str,
) -> i64 {
    let id = generate_id();
    entity::message::ActiveModel {
        id: Set(id),
        user_id: Set(user_id),
        channel_id: Set(channel_id),
        parent_id: Set(None),
        reply_to_message_id: Set(Some(target_id)),
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

async fn set_display_name(db: &sea_orm::DatabaseConnection, user_id: i64, display_name: &str) {
    let mut user = entity::user::Entity::find_by_id(user_id)
        .one(db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    user.display_name = Set(Some(display_name.to_owned()));
    user.update(db).await.unwrap();
}

async fn insert_public_user(
    db: &sea_orm::DatabaseConnection,
    username: &str,
    display_name: Option<&str>,
) -> i64 {
    let id = generate_id();
    entity::user::ActiveModel {
        id: Set(id),
        username: Set(username.to_owned()),
        display_name: Set(display_name.map(str::to_owned)),
        email: Set(Some(format!("{username}@example.test"))),
        email_verified: Set(false),
        avatar_path: Set(None),
        avatar_updated_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    id
}

async fn message_mention_rows(
    db: &sea_orm::DatabaseConnection,
    message_id: i64,
) -> Vec<entity::message_mention::Model> {
    entity::message_mention::Entity::find()
        .filter(entity::message_mention::Column::MessageId.eq(message_id))
        .order_by_asc(entity::message_mention::Column::Position)
        .all(db)
        .await
        .unwrap()
}

async fn insert_message_mention(
    db: &sea_orm::DatabaseConnection,
    message_id: i64,
    user_id: i64,
    position: i32,
) {
    entity::message_mention::ActiveModel {
        id: Set(generate_id()),
        message_id: Set(message_id),
        user_id: Set(user_id),
        position: Set(position),
        created_at: Set(now_unix_micros()),
    }
    .insert(db)
    .await
    .unwrap();
}

async fn insert_custom_emoji(
    db: &sea_orm::DatabaseConnection,
    user_id: i64,
    name: &str,
    animated: bool,
    deleted_at: Option<i64>,
) -> entity::emoji::Model {
    let id = generate_id();
    entity::emoji::ActiveModel {
        id: Set(id),
        image_path: Set(format!(
            "emojis/{id}.{}",
            if animated { "gif" } else { "webp" }
        )),
        name: Set(name.to_owned()),
        normalized_name: Set(name.to_ascii_lowercase()),
        animated: Set(animated),
        created_by_user_id: Set(user_id),
        created_at: Set(1_700_000_000),
        updated_at: Set(1_700_000_001),
        deleted_at: Set(deleted_at),
    }
    .insert(db)
    .await
    .unwrap()
}

async fn insert_native_reaction(
    db: &sea_orm::DatabaseConnection,
    message_id: i64,
    user_id: i64,
    emoji: &str,
) {
    entity::message_reaction::ActiveModel {
        id: Set(generate_id()),
        message_id: Set(message_id),
        user_id: Set(user_id),
        emoji_kind: Set("native".to_owned()),
        emoji: Set(emoji.to_owned()),
        emoji_key: Set(format!("native:{emoji}")),
        created_at: Set(now_unix_micros()),
    }
    .insert(db)
    .await
    .unwrap();
}

async fn insert_custom_reaction(
    db: &sea_orm::DatabaseConnection,
    message_id: i64,
    user_id: i64,
    emoji_id: i64,
) {
    entity::message_reaction::ActiveModel {
        id: Set(generate_id()),
        message_id: Set(message_id),
        user_id: Set(user_id),
        emoji_kind: Set("custom".to_owned()),
        emoji: Set(emoji_id.to_string()),
        emoji_key: Set(format!("custom:{emoji_id}")),
        created_at: Set(now_unix_micros()),
    }
    .insert(db)
    .await
    .unwrap();
}

async fn insert_message_attachment(
    db: &sea_orm::DatabaseConnection,
    message_id: i64,
    position: i32,
) -> i64 {
    let id = generate_id();
    entity::message_attachment::ActiveModel {
        id: Set(id),
        message_id: Set(message_id),
        position: Set(position),
        content_type: Set("image/webp".to_owned()),
        byte_size: Set(12_345),
        width: Set(640),
        height: Set(480),
        storage_path: Set(format!("attachments/{id}/full.webp")),
        thumbnail_content_type: Set("image/webp".to_owned()),
        thumbnail_byte_size: Set(2_345),
        thumbnail_width: Set(320),
        thumbnail_height: Set(240),
        thumbnail_storage_path: Set(format!("attachments/{id}/thumb.webp")),
        created_at: Set(now_unix_micros()),
    }
    .insert(db)
    .await
    .unwrap();
    id
}

async fn attachment_row(
    db: &sea_orm::DatabaseConnection,
    attachment_id: i64,
) -> entity::message_attachment::Model {
    entity::message_attachment::Entity::find_by_id(attachment_id)
        .one(db)
        .await
        .unwrap()
        .unwrap()
}

fn write_attachment_file(root: &std::path::Path, relative_path: &str, bytes: &[u8]) {
    let path = root.join(relative_path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

async fn insert_message_attachment_with_files(
    db: &sea_orm::DatabaseConnection,
    storage_root: &std::path::Path,
    message_id: i64,
    position: i32,
) -> (i64, String, String) {
    let attachment_id = insert_message_attachment(db, message_id, position).await;
    let row = attachment_row(db, attachment_id).await;
    write_attachment_file(storage_root, &row.storage_path, b"full");
    write_attachment_file(storage_root, &row.thumbnail_storage_path, b"thumb");
    (attachment_id, row.storage_path, row.thumbnail_storage_path)
}

fn assert_empty_attachments(message: &serde_json::Value) {
    assert_eq!(message["attachments"], serde_json::json!([]));
}

fn assert_empty_mentions(message: &serde_json::Value) {
    assert_eq!(message["mentions"], serde_json::json!([]));
}

fn assert_reply_metadata_null(message: &serde_json::Value) {
    assert_eq!(
        message.get("reply_to_message_id"),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(message.get("reply_to"), Some(&serde_json::Value::Null));
}

async fn assert_error_response<B>(
    resp: actix_web::dev::ServiceResponse<B>,
    status: StatusCode,
    kind: &str,
) where
    B: actix_web::body::MessageBody,
{
    assert_eq!(resp.status(), status);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["kind"], kind);
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
async fn test_message_create_without_inline_reply_returns_explicit_null_reply_fields() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "plain top-level"}).to_string())
        .to_request();
    let created: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;

    assert_eq!(created["text"], "plain top-level");
    assert_empty_mentions(&created);
    assert_reply_metadata_null(&created);
}

#[actix_web::test]
async fn test_channel_json_mentions_persist_hydrate_history_and_sse() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob_id = insert_public_user(&ctx.db, "bob", Some("Bobby Tables")).await;
    let carol_id = insert_public_user(&ctx.db, "carol", None).await;
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let text = format!("hi <@{bob_id}> twice <@{bob_id}> and <@{carol_id}>");
    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": text}).to_string())
        .to_request();
    let created: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let created_id = created["id"].as_i64().unwrap();

    assert_eq!(created["text"], text);
    assert_eq!(created["mentions"].as_array().unwrap().len(), 2);
    assert_eq!(created["mentions"][0]["id"], bob_id);
    assert_eq!(created["mentions"][0]["username"], "bob");
    assert_eq!(created["mentions"][0]["display_name"], "Bobby Tables");
    assert_eq!(created["mentions"][1]["id"], carol_id);
    assert_eq!(created["mentions"][1]["username"], "carol");
    assert!(created["mentions"][0].get("email").is_none());
    assert!(created["mentions"][0].get("email_verified").is_none());
    assert!(created["mentions"][0].get("avatar_path").is_none());

    let mention_rows = message_mention_rows(&ctx.db, created_id).await;
    assert_eq!(mention_rows.len(), 2);
    assert_eq!(mention_rows[0].user_id, bob_id);
    assert_eq!(mention_rows[0].position, 0);
    assert_eq!(mention_rows[1].user_id, carol_id);
    assert_eq!(mention_rows[1].position, 1);

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for message broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"message\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{bob_id}")));
    assert!(event_str.contains("Bobby Tables"));
    assert!(event_str.contains(&format!("\\\"id\\\":{carol_id}")));

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let created_row = history
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == created_id)
        .unwrap();
    assert_eq!(created_row["mentions"][0]["id"], bob_id);
    assert_eq!(created_row["mentions"][1]["id"], carol_id);
}

#[actix_web::test]
async fn test_channel_json_mentions_ignore_malformed_markers_and_reject_invalid_batches() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(
            serde_json::json!({"text": "ordinary <@> <@abc> <@-1> <@123abc> <@123"}).to_string(),
        )
        .to_request();
    let created: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_empty_mentions(&created);

    for text in [
        "unsafe zero <@0>".to_owned(),
        "unsafe js <@9007199254740992>".to_owned(),
        "missing user <@900000000000000>".to_owned(),
    ] {
        let req = test::TestRequest::post()
            .uri(&format!("/message/{chan_id}"))
            .insert_header(ContentType::json())
            .insert_header(alice.cookie_header())
            .set_payload(serde_json::json!({"text": text}).to_string())
            .to_request();
        assert_error_response(
            test::call_service(&app, req).await,
            StatusCode::BAD_REQUEST,
            "invalid_request",
        )
        .await;
    }

    let mut marker_text = String::new();
    for index in 0..=mentions::MAX_UNIQUE_MENTIONS_PER_MESSAGE {
        let user_id = insert_public_user(&ctx.db, &format!("mention_cap_{index}"), None).await;
        marker_text.push_str(&format!("<@{user_id}> "));
    }
    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": marker_text}).to_string())
        .to_request();
    assert_error_response(
        test::call_service(&app, req).await,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    )
    .await;
}

#[actix_web::test]
async fn test_thread_mentions_round_trip_fetch_previews_and_sse() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob_id = insert_public_user(&ctx.db, "bob", Some("Bobby Tables")).await;
    let carol_id = insert_public_user(&ctx.db, "carol", None).await;

    let root_text = format!("root mentions <@{bob_id}>");
    let root_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        &root_text,
    )
    .await;
    insert_message_mention(&ctx.db, root_id, bob_id, 0).await;

    let older_reply_text = format!("older mentions <@{carol_id}>");
    let older_reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        1_700_000_010_000_000,
        &older_reply_text,
    )
    .await;
    insert_message_mention(&ctx.db, older_reply_id, carol_id, 0).await;

    let middle_reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        1_700_000_020_000_000,
        "middle without mention",
    )
    .await;

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let invalid_req = test::TestRequest::post()
        .uri(&format!("/thread/{root_id}/reply"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "invalid <@0>"}).to_string())
        .to_request();
    assert_error_response(
        test::call_service(&app, invalid_req).await,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    )
    .await;

    let mut rx = ctx.broadcaster.test_client();
    let newest_reply_text = format!("newest mentions <@{bob_id}>");
    let req = test::TestRequest::post()
        .uri(&format!("/thread/{root_id}/reply"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": newest_reply_text}).to_string())
        .to_request();
    let created: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let newest_reply_id = created["id"].as_i64().unwrap();
    let newest_created_at = created["created_at"].as_i64().unwrap();

    assert_eq!(created["parent_id"], root_id);
    assert_reply_metadata_null(&created);
    assert_eq!(created["mentions"].as_array().unwrap().len(), 1);
    assert_eq!(created["mentions"][0]["id"], bob_id);
    assert_eq!(created["mentions"][0]["username"], "bob");
    assert_eq!(created["mentions"][0]["display_name"], "Bobby Tables");
    let newest_mentions = message_mention_rows(&ctx.db, newest_reply_id).await;
    assert_eq!(newest_mentions.len(), 1);
    assert_eq!(newest_mentions[0].user_id, bob_id);

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for thread reply mention broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"thread_reply_created\\\""));
    assert!(event_str.contains(&format!("\\\"root_message_id\\\":{root_id}")));
    assert!(event_str.contains(&format!("\\\"id\\\":{newest_reply_id}")));
    assert!(event_str.contains(&format!("\\\"id\\\":{bob_id}")));
    assert!(event_str.contains("Bobby Tables"));

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(thread["root"]["id"], root_id);
    assert_eq!(thread["root"]["mentions"][0]["id"], bob_id);
    assert_eq!(
        thread["root"]["mentions"][0]["display_name"],
        "Bobby Tables"
    );
    let replies = thread["replies"].as_array().unwrap();
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0]["id"], older_reply_id);
    assert_eq!(replies[0]["mentions"][0]["id"], carol_id);
    assert_eq!(replies[1]["id"], middle_reply_id);
    assert_empty_mentions(&replies[1]);
    assert_eq!(replies[2]["id"], newest_reply_id);
    assert_eq!(replies[2]["mentions"][0]["id"], bob_id);

    let req = test::TestRequest::get()
        .uri("/threads/participated")
        .insert_header(alice.cookie_header())
        .to_request();
    let previews: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let preview = previews
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["root"]["id"] == root_id)
        .unwrap();
    assert_eq!(preview["channel"]["id"], chan_id);
    assert_eq!(preview["reply_count"], 3);
    assert_eq!(preview["last_reply_created_at"], newest_created_at);
    assert_eq!(preview["root"]["mentions"][0]["id"], bob_id);
    let recent = preview["recent_replies"].as_array().unwrap();
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0]["id"], older_reply_id);
    assert_eq!(recent[0]["mentions"][0]["id"], carol_id);
    assert_eq!(recent[1]["id"], middle_reply_id);
    assert_empty_mentions(&recent[1]);
    assert_eq!(recent[2]["id"], newest_reply_id);
    assert_eq!(recent[2]["mentions"][0]["id"], bob_id);
}

#[actix_web::test]
async fn test_thread_reply_edit_replaces_mentions_and_broadcasts_hydrated_update() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob_id = insert_public_user(&ctx.db, "bob", Some("Bobby Tables")).await;
    let carol_id = insert_public_user(&ctx.db, "carol", Some("Carolyn")).await;
    let root_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "root",
    )
    .await;
    let reply_text = format!("initial <@{bob_id}>");
    let reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        1_700_000_010_000_000,
        &reply_text,
    )
    .await;
    insert_message_mention(&ctx.db, reply_id, bob_id, 0).await;
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let invalid_req = test::TestRequest::put()
        .uri(&format!("/message/{reply_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "invalid <@0>"}).to_string())
        .to_request();
    assert_error_response(
        test::call_service(&app, invalid_req).await,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    )
    .await;
    let unchanged_mentions = message_mention_rows(&ctx.db, reply_id).await;
    assert_eq!(unchanged_mentions.len(), 1);
    assert_eq!(unchanged_mentions[0].user_id, bob_id);

    let edited_text = format!("edited <@{carol_id}>");
    let req = test::TestRequest::put()
        .uri(&format!("/message/{reply_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": edited_text}).to_string())
        .to_request();
    let edited: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(edited["id"], reply_id);
    assert_eq!(edited["parent_id"], root_id);
    assert_eq!(edited["mentions"].as_array().unwrap().len(), 1);
    assert_eq!(edited["mentions"][0]["id"], carol_id);
    assert_eq!(edited["mentions"][0]["display_name"], "Carolyn");

    let replaced_mentions = message_mention_rows(&ctx.db, reply_id).await;
    assert_eq!(replaced_mentions.len(), 1);
    assert_eq!(replaced_mentions[0].user_id, carol_id);
    assert_eq!(replaced_mentions[0].position, 0);

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for thread reply edit broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"message_updated\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{reply_id}")));
    assert!(event_str.contains(&format!("\\\"parent_id\\\":{root_id}")));
    assert!(event_str.contains(&format!("\\\"id\\\":{carol_id}")));
    assert!(event_str.contains("Carolyn"));

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(thread["replies"][0]["id"], reply_id);
    assert_eq!(thread["replies"][0]["mentions"][0]["id"], carol_id);
    assert_eq!(
        thread["replies"][0]["mentions"][0]["display_name"],
        "Carolyn"
    );
}

#[actix_web::test]
async fn test_inline_reply_create_persists_reference_and_hydrates_history() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    set_display_name(&ctx.db, bob.user_id, "Bobby Tables").await;
    let target_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "target message text",
    )
    .await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(
            serde_json::json!({"text": "inline reply body", "reply_to_message_id": target_id})
                .to_string(),
        )
        .to_request();
    let created: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let created_id = created["id"].as_i64().unwrap();

    assert_eq!(created["parent_id"], serde_json::Value::Null);
    assert_eq!(created["reply_to_message_id"], target_id);
    assert_eq!(created["reply_to"]["id"], target_id);
    assert_eq!(created["reply_to"]["user_id"], bob.user_id);
    assert_eq!(created["reply_to"]["channel_id"], chan_id);
    assert_eq!(created["reply_to"]["text"], "target message text");
    assert_eq!(created["reply_to"]["username"], "bob");
    assert_eq!(created["reply_to"]["display_name"], "Bobby Tables");

    let stored = entity::message::Entity::find_by_id(created_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.parent_id, None);
    assert_eq!(stored.reply_to_message_id, Some(target_id));

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let rows = history.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], target_id);
    assert_reply_metadata_null(&rows[0]);
    assert_eq!(rows[1]["id"], created_id);
    assert_eq!(rows[1]["parent_id"], serde_json::Value::Null);
    assert_eq!(rows[1]["reply_to"]["id"], target_id);
    assert_eq!(rows[1]["reply_to"]["text"], "target message text");
}

#[actix_web::test]
async fn test_reply_reference_loader_compacts_duplicates_deleted_missing_and_author_data() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    set_display_name(&ctx.db, bob.user_id, "Bobby Tables").await;

    let target_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "target with rich extras",
    )
    .await;
    insert_message_attachment(&ctx.db, target_id, 0).await;
    entity::embed::ActiveModel {
        id: Set(generate_id()),
        message_id: Set(target_id),
        url: Set("https://example.com".into()),
        title: Set(Some("Example".into())),
        description: Set(Some("rich description".into())),
        image_url: Set(None),
        site_name: Set(Some("Example".into())),
        embed_type: Set("link".into()),
        iframe_url: Set(None),
        iframe_width: Set(None),
        iframe_height: Set(None),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    insert_native_reaction(&ctx.db, target_id, alice.user_id, "👍").await;
    insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(target_id),
        1_700_000_010_000_000,
        "thread reply creates target summary",
    )
    .await;

    let first_reply_id = insert_inline_reply_with_target(
        &ctx.db,
        alice.user_id,
        chan_id,
        target_id,
        1_700_000_020_000_000,
        "first duplicate reference",
    )
    .await;
    let second_reply_id = insert_inline_reply_with_target(
        &ctx.db,
        alice.user_id,
        chan_id,
        target_id,
        1_700_000_030_000_000,
        "second duplicate reference",
    )
    .await;

    let deleted_target_id = generate_id();
    entity::message::ActiveModel {
        id: Set(deleted_target_id),
        user_id: Set(bob.user_id),
        channel_id: Set(chan_id),
        parent_id: Set(None),
        reply_to_message_id: Set(None),
        created_at: Set(1_700_000_040_000_000),
        deleted_at: Set(Some(1_700_000_041_000_000)),
        text: Set(String::new()),
        suppress_embeds: Set(true),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    let reply_to_deleted_id = insert_inline_reply_with_target(
        &ctx.db,
        alice.user_id,
        chan_id,
        deleted_target_id,
        1_700_000_050_000_000,
        "reply to deleted target",
    )
    .await;

    let missing_target_id = generate_id();
    let reply_to_missing_id = insert_inline_reply_with_target(
        &ctx.db,
        alice.user_id,
        chan_id,
        missing_target_id,
        1_700_000_060_000_000,
        "reply to missing target",
    )
    .await;

    let deleted_reply_id = generate_id();
    entity::message::ActiveModel {
        id: Set(deleted_reply_id),
        user_id: Set(alice.user_id),
        channel_id: Set(chan_id),
        parent_id: Set(None),
        reply_to_message_id: Set(Some(target_id)),
        created_at: Set(1_700_000_070_000_000),
        deleted_at: Set(Some(1_700_000_071_000_000)),
        text: Set(String::new()),
        suppress_embeds: Set(true),
    }
    .insert(&ctx.db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;
    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let rows = history.as_array().unwrap();
    let find_row = |id: i64| rows.iter().find(|row| row["id"] == id).unwrap();

    for reply_id in [first_reply_id, second_reply_id] {
        let reply = find_row(reply_id);
        let reference = &reply["reply_to"];
        assert_eq!(reply["reply_to_message_id"], target_id);
        assert_eq!(reference["id"], target_id);
        assert_eq!(reference["user_id"], bob.user_id);
        assert_eq!(reference["channel_id"], chan_id);
        assert_eq!(reference["created_at"], 1_700_000_000_000_000_i64);
        assert_eq!(reference["text"], "target with rich extras");
        assert_eq!(reference["username"], "bob");
        assert_eq!(reference["display_name"], "Bobby Tables");
        assert_eq!(reference["attachment_count"], 1);
        assert!(reference.get("attachments").is_none());
        assert!(reference.get("embeds").is_none());
        assert!(reference.get("reactions").is_none());
        assert!(reference.get("thread_summary").is_none());
        assert!(reference.get("reply_to").is_none());
    }

    let deleted_target_reply = find_row(reply_to_deleted_id);
    assert_eq!(deleted_target_reply["reply_to"]["id"], deleted_target_id);
    assert_eq!(
        deleted_target_reply["reply_to"]["deleted_at"],
        1_700_000_041_000_000_i64
    );
    assert_eq!(deleted_target_reply["reply_to"]["username"], "bob");
    assert_eq!(
        deleted_target_reply["reply_to"]["display_name"],
        "Bobby Tables"
    );

    let missing_target_reply = find_row(reply_to_missing_id);
    assert_eq!(
        missing_target_reply["reply_to_message_id"],
        missing_target_id
    );
    assert_eq!(
        missing_target_reply.get("reply_to"),
        Some(&serde_json::Value::Null)
    );

    let deleted_reply = find_row(deleted_reply_id);
    assert_eq!(deleted_reply["deleted_at"], 1_700_000_071_000_000_i64);
    assert_eq!(deleted_reply["reply_to_message_id"], target_id);
    assert_eq!(
        deleted_reply.get("reply_to"),
        Some(&serde_json::Value::Null)
    );
}

#[actix_web::test]
async fn test_thread_and_participated_surfaces_include_reply_metadata_or_explicit_nulls() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let target_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "referenced from thread root",
    )
    .await;
    let root_id = insert_inline_reply_with_target(
        &ctx.db,
        alice.user_id,
        chan_id,
        target_id,
        1_700_000_010_000_000,
        "inline root with thread replies",
    )
    .await;
    let older_reply_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        Some(root_id),
        1_700_000_020_000_000,
        "older thread reply",
    )
    .await;
    let newer_reply_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        Some(root_id),
        1_700_000_030_000_000,
        "newer thread reply",
    )
    .await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}?limit=1"))
        .insert_header(alice.cookie_header())
        .to_request();
    let newest_page: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(newest_page["root"]["reply_to_message_id"], target_id);
    assert_eq!(newest_page["root"]["reply_to"]["id"], target_id);
    assert_eq!(
        newest_page["root"]["reply_to"]["text"],
        "referenced from thread root"
    );
    assert_eq!(newest_page["replies"].as_array().unwrap().len(), 1);
    assert_eq!(newest_page["replies"][0]["id"], newer_reply_id);
    assert_reply_metadata_null(&newest_page["replies"][0]);
    assert_eq!(newest_page["has_more_replies"], true);

    let cursor_created_at = newest_page["replies"][0]["created_at"].as_i64().unwrap();
    let req = test::TestRequest::get()
        .uri(&format!(
            "/thread/{root_id}?limit=1&before_created_at={cursor_created_at}&before_id={newer_reply_id}"
        ))
        .insert_header(alice.cookie_header())
        .to_request();
    let older_page: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(older_page["root"]["reply_to_message_id"], target_id);
    assert_eq!(older_page["root"]["reply_to"]["id"], target_id);
    assert_eq!(older_page["replies"][0]["id"], older_reply_id);
    assert_reply_metadata_null(&older_page["replies"][0]);
    assert_eq!(older_page["has_more_replies"], false);

    let req = test::TestRequest::get()
        .uri("/threads/participated")
        .insert_header(alice.cookie_header())
        .to_request();
    let previews: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let preview = previews
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["root"]["id"] == root_id)
        .unwrap();
    assert_eq!(preview["root"]["reply_to_message_id"], target_id);
    assert_eq!(preview["root"]["reply_to"]["id"], target_id);
    assert_eq!(preview["recent_replies"].as_array().unwrap().len(), 2);
    assert_reply_metadata_null(&preview["recent_replies"][0]);
    assert_reply_metadata_null(&preview["recent_replies"][1]);
}

#[actix_web::test]
async fn test_message_edit_preserves_inline_reply_metadata_in_response_and_broadcast() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    set_display_name(&ctx.db, bob.user_id, "Bobby Tables").await;
    let target_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "edit target text",
    )
    .await;
    let reply_id = insert_inline_reply_with_target(
        &ctx.db,
        alice.user_id,
        chan_id,
        target_id,
        1_700_000_010_000_000,
        "inline before edit",
    )
    .await;
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::put()
        .uri(&format!("/message/{reply_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "inline after edit"}).to_string())
        .to_request();
    let edited: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(edited["id"], reply_id);
    assert_eq!(edited["text"], "inline after edit");
    assert_eq!(edited["reply_to_message_id"], target_id);
    assert_eq!(edited["reply_to"]["id"], target_id);
    assert_eq!(edited["reply_to"]["text"], "edit target text");
    assert_eq!(edited["reply_to"]["username"], "bob");
    assert_eq!(edited["reply_to"]["display_name"], "Bobby Tables");

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for edit broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"message_updated\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{reply_id}")));
    assert!(event_str.contains(&format!("\\\"reply_to_message_id\\\":{target_id}")));
    assert!(event_str.contains("edit target text"));
    assert!(event_str.contains("Bobby Tables"));
}

#[actix_web::test]
async fn test_tombstoned_inline_reply_root_preserves_durable_reply_target_field() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let target_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "target that survives inline root tombstone",
    )
    .await;
    let root_id = insert_inline_reply_with_target(
        &ctx.db,
        alice.user_id,
        chan_id,
        target_id,
        1_700_000_010_000_000,
        "inline root to tombstone",
    )
    .await;
    insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        Some(root_id),
        1_700_000_020_000_000,
        "reply preserving tombstone root",
    )
    .await;
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for tombstone broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"message_updated\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{root_id}")));
    assert!(event_str.contains(&format!("\\\"reply_to_message_id\\\":{target_id}")));
    assert!(event_str.contains("\\\"reply_to\\\":null"));

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let tombstone = history
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == root_id)
        .unwrap();
    assert!(tombstone["deleted_at"].as_i64().is_some());
    assert_eq!(tombstone["reply_to_message_id"], target_id);
    assert_eq!(tombstone.get("reply_to"), Some(&serde_json::Value::Null));
}

#[actix_web::test]
async fn test_inline_reply_create_rejects_invalid_json_targets_with_clear_errors() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let other_channel_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(other_channel_id),
        name: Set("random".to_owned()),
        position: Set(1),
        channel_type: Set("text".to_owned()),
    }
    .insert(&ctx.db)
    .await
    .unwrap();

    let same_channel_target = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "same channel target",
    )
    .await;
    let cross_channel_target = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        other_channel_id,
        None,
        1_700_000_001_000_000,
        "cross channel target",
    )
    .await;
    let thread_reply_target = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(same_channel_target),
        1_700_000_002_000_000,
        "thread reply target",
    )
    .await;
    let deleted_target = generate_id();
    entity::message::ActiveModel {
        id: Set(deleted_target),
        user_id: Set(alice.user_id),
        channel_id: Set(chan_id),
        parent_id: Set(None),
        reply_to_message_id: Set(None),
        created_at: Set(1_700_000_003_000_000),
        deleted_at: Set(Some(1_700_000_004_000_000)),
        text: Set(String::new()),
        suppress_embeds: Set(true),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .set_payload(
            serde_json::json!({"text": "unauth", "reply_to_message_id": same_channel_target})
                .to_string(),
        )
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "null", "reply_to_message_id": null}).to_string())
        .to_request();
    let created: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(created["text"], "null");
    assert_reply_metadata_null(&created);

    for payload in [
        serde_json::json!({"text": "malformed", "reply_to_message_id": "nope"}),
        serde_json::json!({"text": "zero", "reply_to_message_id": 0}),
        serde_json::json!({"text": "negative", "reply_to_message_id": -1}),
        serde_json::json!({"text": "unsafe", "reply_to_message_id": 9_007_199_254_740_992_i64}),
    ] {
        let req = test::TestRequest::post()
            .uri(&format!("/message/{chan_id}"))
            .insert_header(ContentType::json())
            .insert_header(alice.cookie_header())
            .set_payload(payload.to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        let expected_kind = if payload["reply_to_message_id"].is_string() {
            "invalid_request"
        } else {
            "reply_target_unsafe"
        };
        assert_error_response(resp, StatusCode::BAD_REQUEST, expected_kind).await;
    }

    for (target_id, status, kind) in [
        (
            9_000_000_000_000_000_i64,
            StatusCode::NOT_FOUND,
            "reply_target_not_found",
        ),
        (
            cross_channel_target,
            StatusCode::BAD_REQUEST,
            "reply_target_cross_channel",
        ),
        (
            thread_reply_target,
            StatusCode::BAD_REQUEST,
            "reply_target_not_top_level",
        ),
        (
            deleted_target,
            StatusCode::BAD_REQUEST,
            "reply_target_deleted",
        ),
    ] {
        let req = test::TestRequest::post()
            .uri(&format!("/message/{chan_id}"))
            .insert_header(ContentType::json())
            .insert_header(alice.cookie_header())
            .set_payload(
                serde_json::json!({"text": "bad target", "reply_to_message_id": target_id})
                    .to_string(),
            )
            .to_request();
        assert_error_response(test::call_service(&app, req).await, status, kind).await;
    }
}

#[actix_web::test]
async fn test_inline_reply_can_target_an_inline_reply_without_recursive_reference() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let original_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "original message",
    )
    .await;
    let inline_target_id = generate_id();
    entity::message::ActiveModel {
        id: Set(inline_target_id),
        user_id: Set(alice.user_id),
        channel_id: Set(chan_id),
        parent_id: Set(None),
        reply_to_message_id: Set(Some(original_id)),
        created_at: Set(1_700_000_001_000_000),
        deleted_at: Set(None),
        text: Set("first inline reply".to_owned()),
        suppress_embeds: Set(false),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(
            serde_json::json!({"text": "second inline reply", "reply_to_message_id": inline_target_id})
                .to_string(),
        )
        .to_request();
    let created: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;

    assert_eq!(created["parent_id"], serde_json::Value::Null);
    assert_eq!(created["reply_to_message_id"], inline_target_id);
    assert_eq!(created["reply_to"]["id"], inline_target_id);
    assert_eq!(created["reply_to"]["text"], "first inline reply");
    assert!(created["reply_to"].get("reply_to_message_id").is_none());
    assert!(created["reply_to"].get("reply_to").is_none());
}

#[actix_web::test]
async fn test_inline_replies_do_not_change_thread_summaries_or_thread_pages() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "root with one thread reply",
    )
    .await;
    let thread_reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        1_700_000_010_000_000,
        "real thread reply",
    )
    .await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(
            serde_json::json!({"text": "inline reply outside thread", "reply_to_message_id": root_id})
                .to_string(),
        )
        .to_request();
    let inline: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    let inline_id = inline["id"].as_i64().unwrap();
    assert_eq!(inline["parent_id"], serde_json::Value::Null);

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let rows = history.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let root = rows.iter().find(|row| row["id"] == root_id).unwrap();
    let inline_row = rows.iter().find(|row| row["id"] == inline_id).unwrap();
    assert_eq!(root["thread_summary"]["reply_count"], 1);
    assert_eq!(
        root["thread_summary"]["last_reply_created_at"],
        1_700_000_010_000_000_i64
    );
    assert!(inline_row.get("thread_summary").is_none());

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    let replies = thread["replies"].as_array().unwrap();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0]["id"], thread_reply_id);

    let req = test::TestRequest::get()
        .uri("/threads/participated")
        .insert_header(alice.cookie_header())
        .to_request();
    let previews: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let preview = previews
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["root"]["id"] == root_id)
        .unwrap();
    assert_eq!(preview["reply_count"], 1);
    assert_eq!(preview["last_reply_created_at"], 1_700_000_010_000_000_i64);
    assert_eq!(preview["recent_replies"].as_array().unwrap().len(), 1);
    assert_eq!(preview["recent_replies"][0]["id"], thread_reply_id);
}

#[actix_web::test]
async fn test_text_only_message_surfaces_return_empty_attachments() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "root"}).to_string())
        .to_request();
    let created: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_empty_attachments(&created);
    let root_id = created["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/thread/{root_id}/reply"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "reply"}).to_string())
        .to_request();
    let reply: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_empty_attachments(&reply);

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_empty_attachments(&history[0]);

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_empty_attachments(&thread["root"]);
    assert_empty_attachments(&thread["replies"][0]);

    let req = test::TestRequest::get()
        .uri("/threads/participated")
        .insert_header(alice.cookie_header())
        .to_request();
    let previews: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_empty_attachments(&previews[0]["root"]);
    assert_empty_attachments(&previews[0]["recent_replies"][0]);

    let req = test::TestRequest::put()
        .uri(&format!("/message/{root_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "edited root"}).to_string())
        .to_request();
    let edited: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_empty_attachments(&edited);
}

#[actix_web::test]
async fn test_attachment_metadata_uses_canonical_message_response_shape() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message(&ctx.db, alice.user_id, chan_id, "root with photo").await;
    let attachment_id = insert_message_attachment(&ctx.db, root_id, 0).await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let attachment = &history[0]["attachments"][0];
    assert_eq!(attachment["id"], attachment_id);
    assert_eq!(attachment["message_id"], root_id);
    assert_eq!(attachment["position"], 0);
    assert_eq!(attachment["content_type"], "image/webp");
    assert_eq!(attachment["byte_size"], 12_345);
    assert_eq!(attachment["width"], 640);
    assert_eq!(attachment["height"], 480);
    assert_eq!(attachment["url"], format!("/attachments/{attachment_id}"));
    assert_eq!(
        attachment["thumbnail_url"],
        format!("/attachments/{attachment_id}/thumbnail")
    );
    assert_eq!(attachment["thumbnail_content_type"], "image/webp");
    assert_eq!(attachment["thumbnail_byte_size"], 2_345);
    assert_eq!(attachment["thumbnail_width"], 320);
    assert_eq!(attachment["thumbnail_height"], 240);
}

#[actix_web::test]
async fn test_message_edit_updates_only_text_and_preserves_photo_attachments() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message(&ctx.db, alice.user_id, chan_id, "caption").await;
    let root_attachment = insert_message_attachment(&ctx.db, root_id, 0).await;
    let reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        now_unix_micros(),
        "reply caption",
    )
    .await;
    let reply_attachment = insert_message_attachment(&ctx.db, reply_id, 0).await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::put()
        .uri(&format!("/message/{root_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": ""}).to_string())
        .to_request();
    let edited_root: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(edited_root["text"], "");
    assert_eq!(edited_root["attachments"][0]["id"], root_attachment);

    let req = test::TestRequest::put()
        .uri(&format!("/message/{reply_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "edited reply caption"}).to_string())
        .to_request();
    let edited_reply: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(edited_reply["text"], "edited reply caption");
    assert_eq!(edited_reply["attachments"][0]["id"], reply_attachment);

    assert!(
        entity::message_attachment::Entity::find_by_id(root_attachment)
            .one(&ctx.db)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        entity::message_attachment::Entity::find_by_id(reply_attachment)
            .one(&ctx.db)
            .await
            .unwrap()
            .is_some()
    );

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(thread["root"]["text"], "");
    assert_eq!(thread["root"]["attachments"][0]["id"], root_attachment);
    assert_eq!(thread["replies"][0]["text"], "edited reply caption");
    assert_eq!(
        thread["replies"][0]["attachments"][0]["id"],
        reply_attachment
    );
}

#[actix_web::test]
async fn test_native_message_reactions_are_idempotent_and_included_in_history() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let message_id = insert_message(&ctx.db, alice.user_id, chan_id, "react here").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{message_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"kind": "native", "emoji": "👍"}).to_string())
        .to_request();
    let first: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(first.as_array().unwrap().len(), 1);
    assert_eq!(first[0]["kind"], "native");
    assert_eq!(first[0]["emoji"], "👍");
    assert_eq!(first[0]["count"], 1);
    assert_eq!(first[0]["me_reacted"], true);
    assert_eq!(first[0]["reactors"], serde_json::json!(["You"]));

    let req = test::TestRequest::post()
        .uri(&format!("/message/{message_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"kind": "native", "emoji": "👍"}).to_string())
        .to_request();
    let second: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(second[0]["count"], 1);

    let req = test::TestRequest::post()
        .uri(&format!("/message/{message_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(bob.cookie_header())
        .set_payload(serde_json::json!({"kind": "native", "emoji": "👍"}).to_string())
        .to_request();
    let third: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(third[0]["count"], 2);
    assert_eq!(third[0]["me_reacted"], true);
    assert_eq!(third[0]["reactors"], serde_json::json!(["You", "alice"]));

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(history[0]["id"], message_id);
    assert_eq!(history[0]["reactions"][0]["count"], 2);
    assert_eq!(history[0]["reactions"][0]["me_reacted"], true);
    assert_eq!(
        history[0]["reactions"][0]["reactors"],
        serde_json::json!(["You", "bob"])
    );
}

#[actix_web::test]
async fn test_message_reaction_summaries_cap_reactor_previews_and_prefer_display_names() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let carol = ctx.register("carol", "hunter2").await;
    let dave = ctx.register("dave", "hunter2").await;
    let erin = ctx.register("erin", "hunter2").await;
    let frank = ctx.register("frank", "hunter2").await;
    let grace = ctx.register("grace", "hunter2").await;
    set_display_name(&ctx.db, bob.user_id, "Bobby Tables").await;
    set_display_name(&ctx.db, carol.user_id, "Carolyn").await;
    let message_id = insert_message(&ctx.db, alice.user_id, chan_id, "react here").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    for session in [&bob, &carol, &dave, &erin, &frank, &grace, &alice] {
        let req = test::TestRequest::post()
            .uri(&format!("/message/{message_id}/reactions"))
            .insert_header(ContentType::json())
            .insert_header(session.cookie_header())
            .set_payload(serde_json::json!({"kind": "native", "emoji": "👍"}).to_string())
            .to_request();
        assert!(test::call_service(&app, req).await.status().is_success());
    }

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    let summary = &history[0]["reactions"][0];
    assert_eq!(summary["count"], 7);
    assert_eq!(summary["me_reacted"], true);
    assert_eq!(
        summary["reactors"],
        serde_json::json!(["You", "Bobby Tables", "Carolyn", "dave", "erin"])
    );
}

#[actix_web::test]
async fn test_native_message_reaction_remove_is_idempotent_and_drops_empty_summary() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let message_id = insert_message(&ctx.db, alice.user_id, chan_id, "react here").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{message_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"kind": "native", "emoji": "❤️"}).to_string())
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());

    for _ in 0..2 {
        let req = test::TestRequest::delete()
            .uri(&format!("/message/{message_id}/reactions"))
            .insert_header(ContentType::json())
            .insert_header(alice.cookie_header())
            .set_payload(serde_json::json!({"kind": "native", "emoji": "❤️"}).to_string())
            .to_request();
        let body: serde_json::Value =
            serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
                .unwrap();
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(history[0]["reactions"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn test_custom_message_reactions_use_immutable_ids_and_current_display_data() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let emoji = insert_custom_emoji(&ctx.db, alice.user_id, "Party", true, None).await;
    let message_id = insert_message(&ctx.db, alice.user_id, chan_id, "react here").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{message_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"kind": "custom", "emoji_id": emoji.id}).to_string())
        .to_request();
    let first: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(first.as_array().unwrap().len(), 1);
    assert_eq!(first[0]["kind"], "custom");
    assert_eq!(first[0]["emoji_id"], emoji.id);
    assert_eq!(first[0]["name"], "Party");
    assert_eq!(
        first[0]["image_url"],
        format!("/uploads/emojis/{}.gif?v=1700000001", emoji.id)
    );
    assert_eq!(first[0]["animated"], true);
    assert_eq!(first[0]["count"], 1);
    assert_eq!(first[0]["me_reacted"], true);
    assert_eq!(first[0]["reactors"], serde_json::json!(["You"]));

    let req = test::TestRequest::post()
        .uri(&format!("/message/{message_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(bob.cookie_header())
        .set_payload(serde_json::json!({"kind": "custom", "emoji_id": emoji.id}).to_string())
        .to_request();
    let second: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(second[0]["count"], 2);
    assert_eq!(second[0]["me_reacted"], true);
    assert_eq!(second[0]["reactors"], serde_json::json!(["You", "alice"]));

    let mut active = emoji.into_active_model();
    active.name = Set("Renamed".to_owned());
    active.normalized_name = Set("renamed".to_owned());
    active.updated_at = Set(1_700_000_050);
    active.update(&ctx.db).await.unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(
        history[0]["reactions"][0]["emoji_id"],
        second[0]["emoji_id"]
    );
    assert_eq!(history[0]["reactions"][0]["name"], "Renamed");
    assert_eq!(
        history[0]["reactions"][0]["image_url"],
        format!(
            "/uploads/emojis/{}.gif?v=1700000050",
            second[0]["emoji_id"].as_i64().unwrap()
        )
    );
    assert_eq!(history[0]["reactions"][0]["count"], 2);
    assert_eq!(history[0]["reactions"][0]["me_reacted"], true);
    assert_eq!(
        history[0]["reactions"][0]["reactors"],
        serde_json::json!(["You", "bob"])
    );
}

#[actix_web::test]
async fn test_custom_message_reaction_remove_and_deleted_add_rejection() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let emoji = insert_custom_emoji(&ctx.db, alice.user_id, "Static", false, None).await;
    let deleted = insert_custom_emoji(
        &ctx.db,
        alice.user_id,
        "Deleted",
        false,
        Some(1_700_000_010),
    )
    .await;
    let message_id = insert_message(&ctx.db, alice.user_id, chan_id, "react here").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{message_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"kind": "custom", "emoji_id": emoji.id}).to_string())
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{message_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"kind": "custom", "emoji_id": emoji.id}).to_string())
        .to_request();
    let removed: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(removed.as_array().unwrap().len(), 0);

    let req = test::TestRequest::post()
        .uri(&format!("/message/{message_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"kind": "custom", "emoji_id": deleted.id}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );

    let req = test::TestRequest::post()
        .uri(&format!("/message/{message_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"kind": "custom", "emoji_id": 987654321}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );
}

#[actix_web::test]
async fn test_soft_deleted_custom_reaction_stays_visible_removable_but_not_addable() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let emoji = insert_custom_emoji(&ctx.db, alice.user_id, "Ghost", false, None).await;
    let emoji_id = emoji.id;
    let message_id = insert_message(&ctx.db, alice.user_id, chan_id, "react here").await;
    insert_custom_reaction(&ctx.db, message_id, alice.user_id, emoji_id).await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let mut active = emoji.into_active_model();
    active.deleted_at = Set(Some(1_700_000_100));
    active.updated_at = Set(1_700_000_101);
    active.update(&ctx.db).await.unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(history[0]["reactions"][0]["kind"], "custom");
    assert_eq!(history[0]["reactions"][0]["emoji_id"], emoji_id);
    assert_eq!(history[0]["reactions"][0]["name"], "Ghost");
    assert_eq!(history[0]["reactions"][0]["deleted_at"], 1_700_000_100_i64);
    assert_eq!(history[0]["reactions"][0]["me_reacted"], true);

    let req = test::TestRequest::post()
        .uri(&format!("/message/{message_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(bob.cookie_header())
        .set_payload(serde_json::json!({"kind": "custom", "emoji_id": emoji_id}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{message_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"kind": "custom", "emoji_id": emoji_id}).to_string())
        .to_request();
    let removed: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert!(removed.as_array().unwrap().is_empty());
}

#[actix_web::test]
async fn test_reactions_reject_deleted_messages_and_hide_tombstone_rows() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message(&ctx.db, alice.user_id, chan_id, "deleted root").await;
    let reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        now_unix_micros(),
        "deleted reply",
    )
    .await;
    insert_native_reaction(&ctx.db, root_id, alice.user_id, "👍").await;
    insert_native_reaction(&ctx.db, reply_id, alice.user_id, "❤️").await;

    let mut root = entity::message::Entity::find_by_id(root_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    root.deleted_at = Set(Some(1_700_000_010_000_000));
    root.text = Set(String::new());
    root.suppress_embeds = Set(true);
    root.update(&ctx.db).await.unwrap();

    let mut reply = entity::message::Entity::find_by_id(reply_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    reply.deleted_at = Set(Some(1_700_000_020_000_000));
    reply.text = Set(String::new());
    reply.suppress_embeds = Set(true);
    reply.update(&ctx.db).await.unwrap();

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    for method in ["post", "delete"] {
        let req = match method {
            "post" => test::TestRequest::post(),
            _ => test::TestRequest::delete(),
        }
        .uri(&format!("/message/{root_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"kind": "native", "emoji": "👍"}).to_string())
        .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::NOT_FOUND
        );
    }

    let req = test::TestRequest::post()
        .uri("/message/999999999999999/reactions")
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"kind": "native", "emoji": "👍"}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NOT_FOUND
    );

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(history[0]["id"], root_id);
    assert!(history[0]["deleted_at"].as_i64().is_some());
    assert!(history[0]["reactions"].as_array().unwrap().is_empty());

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert!(thread["root"]["reactions"].as_array().unwrap().is_empty());
    assert!(thread["replies"][0]["deleted_at"].as_i64().is_some());
    assert!(
        thread["replies"][0]["reactions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[actix_web::test]
async fn test_message_reaction_requests_reject_discriminator_mismatches() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let message_id = insert_message(&ctx.db, alice.user_id, chan_id, "react here").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    for payload in [
        serde_json::json!({"kind": "native", "emoji_id": 1}),
        serde_json::json!({"kind": "custom", "emoji": "👍"}),
        serde_json::json!({"kind": "custom", "emoji_id": -1}),
        serde_json::json!({"emoji": "👍"}),
    ] {
        let req = test::TestRequest::post()
            .uri(&format!("/message/{message_id}/reactions"))
            .insert_header(ContentType::json())
            .insert_header(alice.cookie_header())
            .set_payload(payload.to_string())
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
}

#[actix_web::test]
async fn test_native_message_reactions_reject_unsupported_and_allow_thread_reply_targets() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message(&ctx.db, alice.user_id, chan_id, "root").await;
    let reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        now_unix_micros(),
        "reply",
    )
    .await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{root_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"kind": "native", "emoji": "🫠"}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );

    let req = test::TestRequest::post()
        .uri(&format!("/message/{reply_id}/reactions"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"kind": "native", "emoji": "👍"}).to_string())
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
async fn test_thread_get_includes_attachments_on_root_newest_and_older_replies() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "root with photo",
    )
    .await;
    let root_attachment = insert_message_attachment(&ctx.db, root_id, 0).await;
    let older_reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        1_700_000_010_000_000,
        "older reply with photo",
    )
    .await;
    let older_attachment = insert_message_attachment(&ctx.db, older_reply_id, 0).await;
    let newer_reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        1_700_000_020_000_000,
        "newer reply with photo",
    )
    .await;
    let newer_attachment = insert_message_attachment(&ctx.db, newer_reply_id, 0).await;

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}?limit=1"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(thread["root"]["attachments"][0]["id"], root_attachment);
    assert_eq!(thread["root"]["attachments"][0]["message_id"], root_id);
    assert_eq!(thread["replies"].as_array().unwrap().len(), 1);
    assert_eq!(thread["replies"][0]["id"], newer_reply_id);
    assert_eq!(
        thread["replies"][0]["attachments"][0]["id"],
        newer_attachment
    );
    assert_eq!(thread["has_more_replies"], true);

    let cursor_created_at = thread["replies"][0]["created_at"].as_i64().unwrap();
    let req = test::TestRequest::get()
        .uri(&format!(
            "/thread/{root_id}?limit=1&before_created_at={cursor_created_at}&before_id={newer_reply_id}"
        ))
        .insert_header(alice.cookie_header())
        .to_request();
    let older_thread: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(
        older_thread["root"]["attachments"][0]["id"],
        root_attachment
    );
    assert_eq!(older_thread["replies"].as_array().unwrap().len(), 1);
    assert_eq!(older_thread["replies"][0]["id"], older_reply_id);
    assert_eq!(
        older_thread["replies"][0]["attachments"][0]["id"],
        older_attachment
    );
    assert_eq!(older_thread["has_more_replies"], false);
}

#[actix_web::test]
async fn test_participated_threads_include_root_and_recent_reply_attachments_and_strip_tombstones()
{
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;

    let root_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "root with photo",
    )
    .await;
    let root_attachment = insert_message_attachment(&ctx.db, root_id, 0).await;
    let old_reply_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        Some(root_id),
        1_700_000_010_000_000,
        "old reply excluded from preview",
    )
    .await;
    let _old_attachment = insert_message_attachment(&ctx.db, old_reply_id, 0).await;
    let recent_reply_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        Some(root_id),
        1_700_000_011_000_000,
        "recent reply with photo",
    )
    .await;
    let recent_attachment = insert_message_attachment(&ctx.db, recent_reply_id, 0).await;
    let middle_reply_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        Some(root_id),
        1_700_000_012_000_000,
        "recent text reply",
    )
    .await;
    let newest_reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        1_700_000_013_000_000,
        "newest reply with photo",
    )
    .await;
    let newest_attachment = insert_message_attachment(&ctx.db, newest_reply_id, 0).await;

    let tombstoned_root_id = generate_id();
    entity::message::ActiveModel {
        id: Set(tombstoned_root_id),
        user_id: Set(alice.user_id),
        channel_id: Set(chan_id),
        parent_id: Set(None),
        reply_to_message_id: Set(None),
        created_at: Set(1_700_000_020_000_000),
        deleted_at: Set(Some(1_700_000_021_000_000)),
        text: Set(String::new()),
        suppress_embeds: Set(true),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    let _tombstone_attachment = insert_message_attachment(&ctx.db, tombstoned_root_id, 0).await;
    insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        Some(tombstoned_root_id),
        1_700_000_022_000_000,
        "reply under tombstone",
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
    let preview = rows
        .iter()
        .find(|row| row["root"]["id"] == root_id)
        .unwrap();
    assert_eq!(preview["root"]["attachments"][0]["id"], root_attachment);
    assert_eq!(preview["root"]["attachments"][0]["message_id"], root_id);
    assert_eq!(
        preview["root"]["attachments"][0]["url"],
        format!("/attachments/{root_attachment}")
    );

    let recent = preview["recent_replies"].as_array().unwrap();
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0]["id"], recent_reply_id);
    assert_eq!(recent[0]["attachments"][0]["id"], recent_attachment);
    assert_eq!(recent[1]["id"], middle_reply_id);
    assert_empty_attachments(&recent[1]);
    assert_eq!(recent[2]["id"], newest_reply_id);
    assert_eq!(recent[2]["attachments"][0]["id"], newest_attachment);
    assert!(
        !preview
            .to_string()
            .contains("old reply excluded from preview")
    );

    let tombstone = rows
        .iter()
        .find(|row| row["root"]["id"] == tombstoned_root_id)
        .unwrap();
    assert_eq!(tombstone["root"]["deleted_at"], 1_700_000_021_000_000_i64);
    assert_empty_attachments(&tombstone["root"]);

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(thread["root"]["attachments"][0]["id"], root_attachment);
    let thread_replies = thread["replies"].as_array().unwrap();
    let full_recent = thread_replies
        .iter()
        .find(|reply| reply["id"] == recent_reply_id)
        .unwrap();
    assert_eq!(full_recent["attachments"][0]["id"], recent_attachment);
    let full_newest = thread_replies
        .iter()
        .find(|reply| reply["id"] == newest_reply_id)
        .unwrap();
    assert_eq!(full_newest["attachments"][0]["id"], newest_attachment);
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
async fn test_thread_payloads_include_root_and_reply_reactions_without_changing_thread_metadata() {
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
        "root message",
    )
    .await;
    let older_reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        1_700_000_001_000_000,
        "older reply",
    )
    .await;
    let newer_reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        1_700_000_002_000_000,
        "newer reply",
    )
    .await;

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let before_history: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    let before_summary = before_history[0]["thread_summary"].clone();
    let before_created_at = before_history[0]["created_at"].clone();

    for (message_id, emoji) in [(root_id, "👍"), (older_reply_id, "❤️")] {
        let req = test::TestRequest::post()
            .uri(&format!("/message/{message_id}/reactions"))
            .insert_header(ContentType::json())
            .insert_header(bob.cookie_header())
            .set_payload(serde_json::json!({"kind": "native", "emoji": emoji}).to_string())
            .to_request();
        assert!(test::call_service(&app, req).await.status().is_success());
    }

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let after_history: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(after_history.as_array().unwrap().len(), 1);
    assert_eq!(after_history[0]["id"], root_id);
    assert_eq!(after_history[0]["created_at"], before_created_at);
    assert_eq!(after_history[0]["thread_summary"], before_summary);
    assert_eq!(after_history[0]["reactions"][0]["emoji"], "👍");
    assert_eq!(after_history[0]["reactions"][0]["count"], 1);

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}?limit=1"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(thread["root"]["reactions"][0]["emoji"], "👍");
    assert_eq!(thread["replies"].as_array().unwrap().len(), 1);
    assert_eq!(thread["replies"][0]["id"], newer_reply_id);
    assert!(
        thread["replies"][0]["reactions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(thread["has_more_replies"], true);

    let cursor_created_at = thread["replies"][0]["created_at"].as_i64().unwrap();
    let req = test::TestRequest::get()
        .uri(&format!(
            "/thread/{root_id}?limit=1&before_created_at={cursor_created_at}&before_id={newer_reply_id}"
        ))
        .insert_header(alice.cookie_header())
        .to_request();
    let older_thread: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(older_thread["replies"].as_array().unwrap().len(), 1);
    assert_eq!(older_thread["replies"][0]["id"], older_reply_id);
    assert_eq!(older_thread["replies"][0]["reactions"][0]["emoji"], "❤️");
    assert_eq!(older_thread["replies"][0]["reactions"][0]["count"], 1);
}

#[actix_web::test]
async fn test_reactions_do_not_change_participated_thread_ordering() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let older_root_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "older thread",
    )
    .await;
    let older_reply_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(older_root_id),
        1_700_000_001_000_000,
        "older reply",
    )
    .await;
    let newer_root_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_002_000_000,
        "newer thread",
    )
    .await;
    insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(newer_root_id),
        1_700_000_003_000_000,
        "newer reply",
    )
    .await;

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri("/threads/participated")
        .insert_header(alice.cookie_header())
        .to_request();
    let before: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(before[0]["root"]["id"], newer_root_id);
    assert_eq!(before[1]["root"]["id"], older_root_id);
    let older_last_reply = before[1]["last_reply_created_at"].clone();

    for message_id in [older_root_id, older_reply_id] {
        let req = test::TestRequest::post()
            .uri(&format!("/message/{message_id}/reactions"))
            .insert_header(ContentType::json())
            .insert_header(bob.cookie_header())
            .set_payload(serde_json::json!({"kind": "native", "emoji": "👍"}).to_string())
            .to_request();
        assert!(test::call_service(&app, req).await.status().is_success());
    }

    let req = test::TestRequest::get()
        .uri("/threads/participated")
        .insert_header(alice.cookie_header())
        .to_request();
    let after: serde_json::Value =
        serde_json::from_slice(&test::read_body(test::call_service(&app, req).await).await)
            .unwrap();
    assert_eq!(after[0]["root"]["id"], newer_root_id);
    assert_eq!(after[1]["root"]["id"], older_root_id);
    assert_eq!(after[1]["reply_count"], 1);
    assert_eq!(after[1]["last_reply_created_at"], older_last_reply);
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
    let private_dir = make_tmp_uploads_dir();
    let (attachment_id, full_path, thumbnail_path) =
        insert_message_attachment_with_files(&ctx.db, &private_dir, root_id, 0).await;
    entity::embed::ActiveModel {
        id: Set(generate_id()),
        message_id: Set(root_id),
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
    insert_native_reaction(&ctx.db, root_id, alice.user_id, "👍").await;
    insert_native_reaction(&ctx.db, root_id, bob.user_id, "👍").await;

    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

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

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for tombstone broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"message_updated\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{root_id}")));
    assert!(event_str.contains("\\\"text\\\":\\\"\\\""));
    assert!(event_str.contains("\\\"suppress_embeds\\\":true"));
    assert!(event_str.contains("\\\"attachments\\\":[]"));
    assert!(event_str.contains("\\\"embeds\\\":[]"));
    assert!(event_str.contains("\\\"reactions\\\":[]"));
    assert!(event_str.contains("\\\"reply_count\\\":2"));

    let stored = entity::message::Entity::find_by_id(root_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.deleted_at.is_some());
    assert_eq!(stored.text, "");
    assert!(stored.suppress_embeds);
    let remaining_reactions = entity::message_reaction::Entity::find()
        .filter(entity::message_reaction::Column::MessageId.eq(root_id))
        .all(&ctx.db)
        .await
        .unwrap();
    assert!(remaining_reactions.is_empty());
    let remaining_attachments = entity::message_attachment::Entity::find()
        .filter(entity::message_attachment::Column::MessageId.eq(root_id))
        .all(&ctx.db)
        .await
        .unwrap();
    assert!(remaining_attachments.is_empty());
    let remaining_embeds = entity::embed::Entity::find()
        .filter(entity::embed::Column::MessageId.eq(root_id))
        .all(&ctx.db)
        .await
        .unwrap();
    assert!(remaining_embeds.is_empty());
    assert!(
        entity::message_attachment::Entity::find_by_id(attachment_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(!private_dir.join(full_path).exists());
    assert!(!private_dir.join(thumbnail_path).exists());

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
    assert_empty_attachments(&history[0]);
    assert!(history[0]["reactions"].as_array().unwrap().is_empty());
    assert_eq!(history[0]["thread_summary"]["reply_count"], 2);

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(thread["root"]["id"], root_id);
    assert!(thread["root"]["deleted_at"].as_i64().is_some());
    assert_eq!(thread["root"]["text"], "");
    assert_empty_attachments(&thread["root"]);
    assert!(thread["root"]["reactions"].as_array().unwrap().is_empty());
    let replies = thread["replies"].as_array().unwrap();
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0]["text"], "first preserved reply");
    assert_eq!(replies[1]["text"], "second preserved reply");

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_deleting_message_with_inline_references_tombstones_and_hydrates_deleted_fallbacks() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let target_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "referenced secret text",
    )
    .await;
    let private_dir = make_tmp_uploads_dir();
    let (attachment_id, full_path, thumbnail_path) =
        insert_message_attachment_with_files(&ctx.db, &private_dir, target_id, 0).await;
    entity::embed::ActiveModel {
        id: Set(generate_id()),
        message_id: Set(target_id),
        url: Set("https://example.com/secret".into()),
        title: Set(Some("Secret preview".into())),
        description: Set(Some("Secret description".into())),
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
    insert_native_reaction(&ctx.db, target_id, bob.user_id, "👍").await;
    let reply_id = insert_inline_reply_with_target(
        &ctx.db,
        bob.user_id,
        chan_id,
        target_id,
        1_700_000_010_000_000,
        "reply preserving deleted original",
    )
    .await;

    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{target_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for inline-reference tombstone broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{event:?}");
    assert!(event_str.contains("kind\\\":\\\"message_updated\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{target_id}")));
    assert!(event_str.contains("\\\"text\\\":\\\"\\\""));
    assert!(event_str.contains("\\\"attachments\\\":[]"));
    assert!(event_str.contains("\\\"embeds\\\":[]"));
    assert!(event_str.contains("\\\"reactions\\\":[]"));
    assert!(!event_str.contains("referenced secret text"));
    assert!(!event_str.contains("Secret preview"));

    let stored = entity::message::Entity::find_by_id(target_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.deleted_at.is_some());
    assert_eq!(stored.text, "");
    assert!(stored.suppress_embeds);
    assert!(
        entity::message_attachment::Entity::find_by_id(attachment_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(!private_dir.join(full_path).exists());
    assert!(!private_dir.join(thumbnail_path).exists());
    assert!(
        entity::embed::Entity::find()
            .filter(entity::embed::Column::MessageId.eq(target_id))
            .all(&ctx.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        entity::message_reaction::Entity::find()
            .filter(entity::message_reaction::Column::MessageId.eq(target_id))
            .all(&ctx.db)
            .await
            .unwrap()
            .is_empty()
    );

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(bob.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let rows = history.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let tombstone = rows.iter().find(|row| row["id"] == target_id).unwrap();
    assert!(tombstone["deleted_at"].as_i64().is_some());
    assert_eq!(tombstone["text"], "");
    assert_empty_attachments(tombstone);
    assert!(tombstone["embeds"].as_array().unwrap().is_empty());
    assert!(tombstone["reactions"].as_array().unwrap().is_empty());

    let reply = rows.iter().find(|row| row["id"] == reply_id).unwrap();
    assert_eq!(reply["reply_to_message_id"], target_id);
    assert_eq!(reply["reply_to"]["id"], target_id);
    assert!(reply["reply_to"]["deleted_at"].as_i64().is_some());
    assert_eq!(reply["reply_to"]["text"], "");
    assert_eq!(reply["reply_to"]["attachment_count"], 0);
    assert_eq!(reply["text"], "reply preserving deleted original");
    let serialized = serde_json::to_string(&history).unwrap();
    assert!(!serialized.contains("referenced secret text"));
    assert!(!serialized.contains(&format!("/attachments/{attachment_id}")));
    assert!(!serialized.contains("Secret preview"));

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_unreferenced_inline_reply_hard_deletes_normally() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let target_id = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "original survives",
    )
    .await;
    let inline_reply_id = insert_inline_reply_with_target(
        &ctx.db,
        alice.user_id,
        chan_id,
        target_id,
        1_700_000_010_000_000,
        "unreferenced inline reply",
    )
    .await;
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{inline_reply_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for inline hard-delete broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{event:?}");
    assert!(event_str.contains("kind\\\":\\\"message_deleted\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{inline_reply_id}")));

    assert!(
        entity::message::Entity::find_by_id(inline_reply_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        entity::message::Entity::find_by_id(target_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .is_some()
    );

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let rows = history.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], target_id);
}

#[actix_web::test]
async fn test_deleting_referenced_inline_reply_tombstones_and_preserves_incoming_reference() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let original_id = insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        None,
        1_700_000_000_000_000,
        "original target remains visible",
    )
    .await;
    let referenced_inline_id = insert_inline_reply_with_target(
        &ctx.db,
        alice.user_id,
        chan_id,
        original_id,
        1_700_000_010_000_000,
        "inline reply secret text",
    )
    .await;
    let incoming_reply_id = insert_inline_reply_with_target(
        &ctx.db,
        bob.user_id,
        chan_id,
        referenced_inline_id,
        1_700_000_020_000_000,
        "reply to the inline reply",
    )
    .await;
    insert_native_reaction(&ctx.db, referenced_inline_id, bob.user_id, "👍").await;

    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{referenced_inline_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for referenced inline tombstone broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{event:?}");
    assert!(event_str.contains("kind\\\":\\\"message_updated\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{referenced_inline_id}")));
    assert!(event_str.contains(&format!("\\\"reply_to_message_id\\\":{original_id}")));
    assert!(event_str.contains("\\\"reply_to\\\":null"));
    assert!(event_str.contains("\\\"reactions\\\":[]"));
    assert!(!event_str.contains("inline reply secret text"));

    let stored = entity::message::Entity::find_by_id(referenced_inline_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.deleted_at.is_some());
    assert_eq!(stored.text, "");
    assert_eq!(stored.reply_to_message_id, Some(original_id));

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let rows = history.as_array().unwrap();
    assert_eq!(rows.len(), 3);
    let tombstone = rows
        .iter()
        .find(|row| row["id"] == referenced_inline_id)
        .unwrap();
    assert!(tombstone["deleted_at"].as_i64().is_some());
    assert_eq!(tombstone["reply_to_message_id"], original_id);
    assert_eq!(tombstone.get("reply_to"), Some(&serde_json::Value::Null));

    let incoming = rows
        .iter()
        .find(|row| row["id"] == incoming_reply_id)
        .unwrap();
    assert_eq!(incoming["text"], "reply to the inline reply");
    assert_eq!(incoming["reply_to_message_id"], referenced_inline_id);
    assert_eq!(incoming["reply_to"]["id"], referenced_inline_id);
    assert!(incoming["reply_to"]["deleted_at"].as_i64().is_some());
    assert_eq!(incoming["reply_to"]["text"], "");
    let serialized = serde_json::to_string(&history).unwrap();
    assert!(!serialized.contains("inline reply secret text"));
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
        reply_to_message_id: Set(None),
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
    assert_empty_attachments(&rows[1]["root"]);
    assert_eq!(
        rows[1]["recent_replies"][0]["text"],
        "reply to deleted root"
    );

    let serialized = serde_json::to_string(&previews).unwrap();
    assert!(!serialized.contains("bob-only root"));
    assert!(!serialized.contains("bob-only newest reply"));
}

#[actix_web::test]
async fn test_thread_reply_json_rejects_inline_reply_target() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message(&ctx.db, alice.user_id, chan_id, "root").await;
    let target_id = insert_message(&ctx.db, alice.user_id, chan_id, "target").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/thread/{root_id}/reply"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(
            serde_json::json!({"text": "nested inline should fail", "reply_to_message_id": target_id})
                .to_string(),
        )
        .to_request();
    assert_error_response(
        test::call_service(&app, req).await,
        StatusCode::BAD_REQUEST,
        "thread_inline_reply_not_allowed",
    )
    .await;

    let req = test::TestRequest::post()
        .uri(&format!("/thread/{root_id}/reply"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(
            serde_json::json!({"text": "plain thread", "reply_to_message_id": null}).to_string(),
        )
        .to_request();
    let reply: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(reply["parent_id"], root_id);
    assert_reply_metadata_null(&reply);
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
        reply_to_message_id: Set(None),
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
async fn test_message_edit_replaces_mentions_and_broadcasts_hydrated_update() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob_id = insert_public_user(&ctx.db, "bobmentionedit", Some("Bobby Mention")).await;
    let carol_id = insert_public_user(&ctx.db, "carolmentionedit", None).await;
    let msg_id = insert_message(&ctx.db, alice.user_id, chan_id, "plain before mentions").await;
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let first_text = format!("hi <@{bob_id}> twice <@{bob_id}> and <@{carol_id}>");
    let req = test::TestRequest::put()
        .uri(&format!("/message/{msg_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": first_text}).to_string())
        .to_request();
    let edited: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(edited["text"], first_text);
    assert_eq!(edited["mentions"].as_array().unwrap().len(), 2);
    assert_eq!(edited["mentions"][0]["id"], bob_id);
    assert_eq!(edited["mentions"][0]["display_name"], "Bobby Mention");
    assert_eq!(edited["mentions"][1]["id"], carol_id);

    let mention_rows = message_mention_rows(&ctx.db, msg_id).await;
    assert_eq!(mention_rows.len(), 2);
    assert_eq!(mention_rows[0].user_id, bob_id);
    assert_eq!(mention_rows[0].position, 0);
    assert_eq!(mention_rows[1].user_id, carol_id);
    assert_eq!(mention_rows[1].position, 1);

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for first edit broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{event:?}");
    assert!(event_str.contains("kind\\\":\\\"message_updated\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{bob_id}")));
    assert!(event_str.contains("Bobby Mention"));
    assert!(event_str.contains(&format!("\\\"id\\\":{carol_id}")));

    let second_text = format!("only carol remains <@{carol_id}>");
    let req = test::TestRequest::put()
        .uri(&format!("/message/{msg_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": second_text}).to_string())
        .to_request();
    let edited: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(edited["mentions"].as_array().unwrap().len(), 1);
    assert_eq!(edited["mentions"][0]["id"], carol_id);

    let mention_rows = message_mention_rows(&ctx.db, msg_id).await;
    assert_eq!(mention_rows.len(), 1);
    assert_eq!(mention_rows[0].user_id, carol_id);
    assert_eq!(mention_rows[0].position, 0);

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for second edit broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{event:?}");
    assert!(event_str.contains("kind\\\":\\\"message_updated\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{carol_id}")));
    assert!(!event_str.contains("bobmentionedit"));

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(history[0]["mentions"].as_array().unwrap().len(), 1);
    assert_eq!(history[0]["mentions"][0]["id"], carol_id);
}

#[actix_web::test]
async fn test_message_edit_rejects_invalid_mentions_and_preserves_existing_rows() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob_id = insert_public_user(&ctx.db, "editvalidbob", None).await;
    let original_text = format!("original <@{bob_id}>");
    let msg_id = insert_message(&ctx.db, alice.user_id, chan_id, &original_text).await;
    insert_message_mention(&ctx.db, msg_id, bob_id, 0).await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    for text in [
        "unsafe zero <@0>".to_owned(),
        "missing user <@900000000000000>".to_owned(),
    ] {
        let req = test::TestRequest::put()
            .uri(&format!("/message/{msg_id}"))
            .insert_header(ContentType::json())
            .insert_header(alice.cookie_header())
            .set_payload(serde_json::json!({"text": text}).to_string())
            .to_request();
        assert_error_response(
            test::call_service(&app, req).await,
            StatusCode::BAD_REQUEST,
            "invalid_request",
        )
        .await;

        let stored = entity::message::Entity::find_by_id(msg_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.text, original_text);
        let mention_rows = message_mention_rows(&ctx.db, msg_id).await;
        assert_eq!(mention_rows.len(), 1);
        assert_eq!(mention_rows[0].user_id, bob_id);
        assert_eq!(mention_rows[0].position, 0);
    }

    let mut marker_text = String::new();
    for index in 0..=mentions::MAX_UNIQUE_MENTIONS_PER_MESSAGE {
        let user_id = insert_public_user(&ctx.db, &format!("edit_cap_{index}"), None).await;
        marker_text.push_str(&format!("<@{user_id}> "));
    }
    let req = test::TestRequest::put()
        .uri(&format!("/message/{msg_id}"))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": marker_text}).to_string())
        .to_request();
    assert_error_response(
        test::call_service(&app, req).await,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    )
    .await;

    let stored = entity::message::Entity::find_by_id(msg_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.text, original_text);
    let mention_rows = message_mention_rows(&ctx.db, msg_id).await;
    assert_eq!(mention_rows.len(), 1);
    assert_eq!(mention_rows[0].user_id, bob_id);
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
async fn test_message_hard_delete_removes_mention_rows() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob_id = insert_public_user(&ctx.db, "harddeletebob", None).await;
    let msg_id = insert_message(&ctx.db, alice.user_id, chan_id, &format!("bye <@{bob_id}>")).await;
    insert_message_mention(&ctx.db, msg_id, bob_id, 0).await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{msg_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    assert!(
        entity::message::Entity::find_by_id(msg_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(message_mention_rows(&ctx.db, msg_id).await.is_empty());
}

#[actix_web::test]
async fn test_message_tombstone_tolerates_existing_mentions_without_rendering_them() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let target_id =
        insert_public_user(&ctx.db, "secretmentiontarget", Some("Secret Mention")).await;
    let root_id = insert_message(
        &ctx.db,
        alice.user_id,
        chan_id,
        &format!("secret body <@{target_id}>"),
    )
    .await;
    insert_message_mention(&ctx.db, root_id, target_id, 0).await;
    insert_message_with_parent(
        &ctx.db,
        bob.user_id,
        chan_id,
        Some(root_id),
        1_700_000_010_000_000,
        "reply keeps root tombstoned",
    )
    .await;
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for tombstone broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{event:?}");
    assert!(event_str.contains("kind\\\":\\\"message_updated\\\""));
    assert!(event_str.contains("\\\"text\\\":\\\"\\\""));
    assert!(event_str.contains("\\\"mentions\\\":[]"));
    assert!(!event_str.contains("secretmentiontarget"));
    assert!(!event_str.contains("Secret Mention"));

    let mention_rows = message_mention_rows(&ctx.db, root_id).await;
    assert_eq!(mention_rows.len(), 1);
    assert_eq!(mention_rows[0].user_id, target_id);

    let req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let tombstone = history
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == root_id)
        .unwrap();
    assert!(tombstone["deleted_at"].as_i64().is_some());
    assert_eq!(tombstone["text"], "");
    assert_empty_mentions(tombstone);
    let serialized = serde_json::to_string(&history).unwrap();
    assert!(!serialized.contains("secretmentiontarget"));
    assert!(!serialized.contains("Secret Mention"));
}

#[actix_web::test]
async fn test_photo_message_hard_delete_removes_attachment_rows_files_and_broadcasts_delete() {
    let ctx = TestCtx::new().await;
    let chan_id = ctx.channel_id;
    let alice = ctx.register("alice", "hunter2").await;
    let msg_id = insert_message(&ctx.db, alice.user_id, chan_id, "photo caption").await;
    let private_dir = make_tmp_uploads_dir();
    let (attachment_id, full_path, thumbnail_path) =
        insert_message_attachment_with_files(&ctx.db, &private_dir, msg_id, 0).await;
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{msg_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for delete broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"message_deleted\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{msg_id}")));

    assert!(
        entity::message::Entity::find_by_id(msg_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        entity::message_attachment::Entity::find_by_id(attachment_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(!private_dir.join(full_path).exists());
    assert!(!private_dir.join(thumbnail_path).exists());

    let attachment_req = test::TestRequest::get()
        .uri(&format!("/attachments/{attachment_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, attachment_req).await.status(),
        StatusCode::NOT_FOUND
    );

    let history_req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, history_req).await).await;
    assert!(history.as_array().unwrap().is_empty());

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_photo_reply_hard_delete_removes_attachment_rows_files_and_recalculates_summary() {
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
    let photo_reply = insert_message_with_parent(
        &ctx.db,
        alice.user_id,
        chan_id,
        Some(root_id),
        1_700_000_020_000_000,
        "photo reply",
    )
    .await;
    let private_dir = make_tmp_uploads_dir();
    let (attachment_id, full_path, thumbnail_path) =
        insert_message_attachment_with_files(&ctx.db, &private_dir, photo_reply, 0).await;
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{photo_reply}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for reply delete broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"thread_reply_deleted\\\""));
    assert!(event_str.contains(&format!("\\\"reply_id\\\":{photo_reply}")));
    assert!(event_str.contains("\\\"reply_count\\\":1"));

    assert!(
        entity::message::Entity::find_by_id(photo_reply)
            .one(&ctx.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        entity::message_attachment::Entity::find_by_id(attachment_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(!private_dir.join(full_path).exists());
    assert!(!private_dir.join(thumbnail_path).exists());

    let thread_req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value =
        test::read_body_json(test::call_service(&app, thread_req).await).await;
    assert_eq!(thread["replies"].as_array().unwrap().len(), 1);
    assert_eq!(thread["replies"][0]["id"], older_reply);

    let history_req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, history_req).await).await;
    assert_eq!(history[0]["thread_summary"]["reply_count"], 1);
    assert_eq!(
        history[0]["thread_summary"]["last_reply_created_at"],
        1_700_000_010_000_000_i64
    );

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_photo_delete_cleanup_missing_and_unexpected_failures_are_best_effort() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let msg_id = insert_message(&ctx.db, alice.user_id, ctx.channel_id, "cleanup edge").await;
    let private_dir = make_tmp_uploads_dir();
    let attachment_id = insert_message_attachment(&ctx.db, msg_id, 0).await;
    let row = attachment_row(&ctx.db, attachment_id).await;
    let directory_at_full_path = private_dir.join(&row.storage_path);
    std::fs::create_dir_all(&directory_at_full_path).unwrap();
    assert!(!private_dir.join(&row.thumbnail_storage_path).exists());
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::delete()
        .uri(&format!("/message/{msg_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    assert!(directory_at_full_path.is_dir());
    assert!(
        entity::message_attachment::Entity::find_by_id(attachment_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        entity::message::Entity::find_by_id(msg_id)
            .one(&ctx.db)
            .await
            .unwrap()
            .is_none()
    );

    std::fs::remove_dir_all(&private_dir).ok();
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
