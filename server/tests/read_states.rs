#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use actix_web::{App, http::StatusCode, test};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    QueryFilter, Set, Statement,
};
use serde_json::{Value, json};

use common::TestCtx;
use hamlet::{configure_app, connect_initialized_database_url, entity, generate_id, read_state};

macro_rules! create_message {
    ($app:expr, $channel_id:expr, $session:expr, $text:expr) => {{
        let req = test::TestRequest::post()
            .uri(&format!("/message/{}", $channel_id))
            .insert_header($session.cookie_header())
            .set_json(json!({ "text": $text }))
            .to_request();
        test::call_and_read_body_json($app, req).await
    }};
}

macro_rules! create_thread_reply {
    ($app:expr, $root_message_id:expr, $session:expr, $text:expr) => {{
        let req = test::TestRequest::post()
            .uri(&format!("/thread/{}/reply", $root_message_id))
            .insert_header($session.cookie_header())
            .set_json(json!({ "text": $text }))
            .to_request();
        test::call_and_read_body_json($app, req).await
    }};
}

macro_rules! update_message {
    ($app:expr, $message_id:expr, $session:expr, $text:expr) => {{
        let req = test::TestRequest::put()
            .uri(&format!("/message/{}", $message_id))
            .insert_header($session.cookie_header())
            .set_json(json!({ "text": $text }))
            .to_request();
        test::call_and_read_body_json($app, req).await
    }};
}

macro_rules! mark_read {
    ($app:expr, $channel_id:expr, $session:expr, $message_id:expr) => {{
        let req = test::TestRequest::put()
            .uri(&format!("/channels/{}/read-state", $channel_id))
            .insert_header($session.cookie_header())
            .set_json(json!({ "last_visible_message_id": $message_id }))
            .to_request();
        test::call_and_read_body_json($app, req).await
    }};
}

#[actix_web::test]
async fn read_state_schema_has_composite_key_and_expected_indexes() {
    let ctx = TestCtx::new().await;
    let columns = ctx
        .db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA table_info('user_channel_read_state')".to_owned(),
        ))
        .await
        .unwrap();
    let mut pk_columns = columns
        .iter()
        .filter_map(|row| {
            let pk_order: i64 = row.try_get_by_index(5).unwrap();
            if pk_order == 0 {
                return None;
            }
            let name: String = row.try_get_by_index(1).unwrap();
            Some((pk_order, name))
        })
        .collect::<Vec<_>>();
    pk_columns.sort_by_key(|(order, _)| *order);
    assert_eq!(
        pk_columns,
        vec![(1, "user_id".to_owned()), (2, "channel_id".to_owned())]
    );

    let indexes = ctx
        .db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA index_list('user_channel_read_state')".to_owned(),
        ))
        .await
        .unwrap();
    let index_names = indexes
        .iter()
        .map(|row| row.try_get_by_index::<String>(1).unwrap())
        .collect::<Vec<_>>();
    assert!(
        index_names
            .iter()
            .any(|name| name == "idx_user_channel_read_state_channel_user")
    );
    assert!(
        index_names
            .iter()
            .any(|name| name == "idx_user_channel_read_state_user_updated")
    );
}

#[actix_web::test]
async fn read_state_backfill_baselines_existing_user_text_channels_to_latest_top_level_message() {
    let db = unique_db("backfill").await;
    let user = insert_user(&db, "alice").await;
    let text_channel = insert_channel(&db, "general", "text", 0).await;
    let empty_channel = insert_channel(&db, "empty", "text", 1).await;
    let voice_channel = insert_channel(&db, "voice", "voice", 2).await;
    insert_message_at(&db, user.id, text_channel.id, None, 100, 900).await;
    insert_message_at(&db, user.id, text_channel.id, None, 200, 100).await;
    insert_message_at(&db, user.id, text_channel.id, Some(900), 1_000, 300).await;

    read_state::ensure_all_read_state_baselines(&db)
        .await
        .unwrap();

    let text_state = read_state_row(&db, user.id, text_channel.id).await.unwrap();
    assert_eq!(text_state.last_read_created_at, 200);
    assert_eq!(text_state.last_read_message_id, 100);

    let empty_state = read_state_row(&db, user.id, empty_channel.id)
        .await
        .unwrap();
    assert_eq!(
        empty_state.last_read_created_at,
        read_state::EMPTY_CURSOR_CREATED_AT
    );
    assert_eq!(
        empty_state.last_read_message_id,
        read_state::EMPTY_CURSOR_MESSAGE_ID
    );

    assert!(
        read_state_row(&db, user.id, voice_channel.id)
            .await
            .is_none()
    );
}

#[actix_web::test]
async fn registered_users_and_new_text_channels_get_read_state_baselines() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "pw").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let initial = read_state_row(&ctx.db, alice.user_id, ctx.channel_id)
        .await
        .expect("registered user is baselined for existing text channel");
    assert_eq!(
        initial.last_read_message_id,
        read_state::EMPTY_CURSOR_MESSAGE_ID
    );

    let req = test::TestRequest::post()
        .uri("/channel")
        .insert_header(alice.cookie_header())
        .set_json(json!({ "name": "new-text", "type": "text" }))
        .to_request();
    let created: Value = test::call_and_read_body_json(&app, req).await;
    let channel_id = created["id"].as_i64().unwrap();

    let created_state = read_state_row(&ctx.db, alice.user_id, channel_id)
        .await
        .expect("new text channel baselines existing users");
    assert_eq!(
        created_state.last_read_created_at,
        read_state::EMPTY_CURSOR_CREATED_AT
    );
    assert_eq!(
        created_state.last_read_message_id,
        read_state::EMPTY_CURSOR_MESSAGE_ID
    );

    let req = test::TestRequest::post()
        .uri("/channel")
        .insert_header(alice.cookie_header())
        .set_json(json!({ "name": "new-voice", "type": "voice" }))
        .to_request();
    let created_voice: Value = test::call_and_read_body_json(&app, req).await;
    let voice_channel_id = created_voice["id"].as_i64().unwrap();
    assert!(
        read_state_row(&ctx.db, alice.user_id, voice_channel_id)
            .await
            .is_none()
    );
}

#[actix_web::test]
async fn read_state_snapshot_requires_auth_and_uses_tuple_order_without_unread_counts() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "pw").await;
    let bob = insert_user(&ctx.db, "bob").await;
    upsert_read_state(&ctx.db, alice.user_id, ctx.channel_id, 100, 9000).await;
    insert_message_at(&ctx.db, bob.id, ctx.channel_id, None, 101, 5).await;

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let unauth = test::TestRequest::get().uri("/read-states").to_request();
    let unauth_resp = test::call_service(&app, unauth).await;
    assert_eq!(unauth_resp.status(), StatusCode::UNAUTHORIZED);

    let req = test::TestRequest::get()
        .uri("/read-states")
        .insert_header(alice.cookie_header())
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    let summary = body.as_array().unwrap().first().unwrap();

    assert_eq!(summary["channel_id"].as_i64(), Some(ctx.channel_id));
    assert_eq!(summary["has_unread"].as_bool(), Some(true));
    assert_eq!(summary["mention_count"].as_i64(), Some(0));
    assert_eq!(summary["last_read_created_at"].as_i64(), Some(100));
    assert_eq!(summary["last_read_message_id"].as_i64(), Some(9000));
    assert!(summary["updated_at"].as_i64().is_some());
    assert!(summary.get("unread_count").is_none());
}

#[actix_web::test]
async fn mark_read_advances_monotonically_and_returns_authoritative_summary() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "pw").await;
    let bob = insert_user(&ctx.db, "bob").await;
    upsert_read_state(&ctx.db, alice.user_id, ctx.channel_id, 0, 0).await;
    insert_message_at(&ctx.db, bob.id, ctx.channel_id, None, 100, 10).await;
    insert_message_at(&ctx.db, bob.id, ctx.channel_id, None, 100, 20).await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let first: Value = mark_read!(&app, ctx.channel_id, &alice, 10);
    assert_eq!(first["last_read_created_at"].as_i64(), Some(100));
    assert_eq!(first["last_read_message_id"].as_i64(), Some(10));
    assert_eq!(first["has_unread"].as_bool(), Some(true));

    let second: Value = mark_read!(&app, ctx.channel_id, &alice, 20);
    assert_eq!(second["last_read_created_at"].as_i64(), Some(100));
    assert_eq!(second["last_read_message_id"].as_i64(), Some(20));
    assert_eq!(second["has_unread"].as_bool(), Some(false));

    let stale: Value = mark_read!(&app, ctx.channel_id, &alice, 10);
    assert_eq!(stale["last_read_created_at"].as_i64(), Some(100));
    assert_eq!(stale["last_read_message_id"].as_i64(), Some(20));
    assert_eq!(stale["has_unread"].as_bool(), Some(false));
}

#[actix_web::test]
async fn mark_read_accepts_tombstones_but_rejects_invalid_targets_and_auth() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "pw").await;
    let bob = insert_user(&ctx.db, "bob").await;
    let other_channel = insert_channel(&ctx.db, "other", "text", 1).await;
    read_state::ensure_channel_read_state_baselines(&ctx.db, other_channel.id)
        .await
        .unwrap();
    let voice = insert_channel(&ctx.db, "voice", "voice", 2).await;
    let top = insert_message_at(&ctx.db, bob.id, ctx.channel_id, None, 100, 30).await;
    let tombstoned = insert_message_at(&ctx.db, bob.id, ctx.channel_id, None, 200, 40).await;
    tombstone_message(&ctx.db, tombstoned.id).await;
    let thread_reply =
        insert_message_at(&ctx.db, bob.id, ctx.channel_id, Some(top.id), 300, 50).await;
    let cross_channel = insert_message_at(&ctx.db, bob.id, other_channel.id, None, 400, 60).await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let ok: Value = mark_read!(&app, ctx.channel_id, &alice, tombstoned.id);
    assert_eq!(ok["last_read_message_id"].as_i64(), Some(tombstoned.id));

    let unauth = test::TestRequest::put()
        .uri(&format!("/channels/{}/read-state", ctx.channel_id))
        .set_json(json!({ "last_visible_message_id": top.id }))
        .to_request();
    assert_eq!(
        test::call_service(&app, unauth).await.status(),
        StatusCode::UNAUTHORIZED
    );

    for (channel_id, message_id) in [
        (ctx.channel_id, 999_999),
        (ctx.channel_id, thread_reply.id),
        (ctx.channel_id, cross_channel.id),
        (voice.id, top.id),
    ] {
        let req = test::TestRequest::put()
            .uri(&format!("/channels/{channel_id}/read-state"))
            .insert_header(alice.cookie_header())
            .set_json(json!({ "last_visible_message_id": message_id }))
            .to_request();
        assert!(!test::call_service(&app, req).await.status().is_success());
    }
}

#[actix_web::test]
async fn mark_read_emits_user_scoped_read_state_sse_after_advancing() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "pw").await;
    let bob_session = ctx.register("bob", "pw").await;
    let bob = entity::user::Entity::find_by_id(bob_session.user_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    upsert_read_state(&ctx.db, alice.user_id, ctx.channel_id, 0, 0).await;
    insert_message_at(&ctx.db, bob.id, ctx.channel_id, None, 100, 70).await;
    let mut alice_rx_one = ctx.broadcaster.test_client_for_user(alice.user_id);
    let mut alice_rx_two = ctx.broadcaster.test_client_for_user(alice.user_id);
    let mut bob_rx = ctx.broadcaster.test_client_for_user(bob.id);
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let _: Value = mark_read!(&app, ctx.channel_id, &alice, 70);

    for rx in [&mut alice_rx_one, &mut alice_rx_two] {
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out waiting for read-state event")
            .expect("broadcast channel closed");
        let event_str = format!("{:?}", event);
        assert!(event_str.contains("kind\\\":\\\"read_state_updated\\\""));
        assert!(event_str.contains("\\\"channel_id\\\":"));
        assert!(event_str.contains("\\\"last_read_message_id\\\":70"));
    }

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), bob_rx.recv())
            .await
            .is_err()
    );

    let _: Value = mark_read!(&app, ctx.channel_id, &alice, 70);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), alice_rx_one.recv())
            .await
            .is_err()
    );
}

#[actix_web::test]
async fn sending_a_message_does_not_directly_advance_sender_cursor() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "pw").await;
    upsert_read_state(&ctx.db, alice.user_id, ctx.channel_id, 0, 0).await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let sent: Value = create_message!(&app, ctx.channel_id, &alice, "from me");
    assert!(sent["id"].as_i64().is_some());

    let state = read_state_row(&ctx.db, alice.user_id, ctx.channel_id)
        .await
        .unwrap();
    assert_eq!(state.last_read_created_at, 0);
    assert_eq!(state.last_read_message_id, 0);
}

#[actix_web::test]
async fn mention_count_counts_created_top_level_mentions_but_excludes_self_and_threads() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "pw").await;
    let bob = ctx.register("bob", "pw").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let _: Value = create_message!(
        &app,
        ctx.channel_id,
        &bob,
        &format!("hi <@{}>", alice.user_id)
    );
    let _: Value = create_message!(
        &app,
        ctx.channel_id,
        &alice,
        &format!("self <@{}>", alice.user_id)
    );
    let root: Value = create_message!(&app, ctx.channel_id, &bob, "thread root");
    let _: Value = create_thread_reply!(
        &app,
        root["id"].as_i64().unwrap(),
        &bob,
        &format!("thread <@{}>", alice.user_id)
    );

    let summaries = read_state::read_state_snapshot(&ctx.db, alice.user_id)
        .await
        .unwrap();
    let summary = summaries
        .iter()
        .find(|summary| summary.channel_id == ctx.channel_id)
        .unwrap();

    assert!(summary.has_unread);
    assert_eq!(summary.mention_count, 1);
}

#[actix_web::test]
async fn edit_created_mentions_do_not_create_badges_and_edited_away_mentions_drop() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "pw").await;
    let bob = ctx.register("bob", "pw").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let plain: Value = create_message!(&app, ctx.channel_id, &bob, "plain message");
    let _: Value = update_message!(
        &app,
        plain["id"].as_i64().unwrap(),
        &bob,
        &format!("edited in <@{}>", alice.user_id)
    );

    let summaries = read_state::read_state_snapshot(&ctx.db, alice.user_id)
        .await
        .unwrap();
    let summary = summaries
        .iter()
        .find(|summary| summary.channel_id == ctx.channel_id)
        .unwrap();
    assert!(summary.has_unread);
    assert_eq!(summary.mention_count, 0);

    let mentioned: Value = create_message!(
        &app,
        ctx.channel_id,
        &bob,
        &format!("created with <@{}>", alice.user_id)
    );
    let summaries = read_state::read_state_snapshot(&ctx.db, alice.user_id)
        .await
        .unwrap();
    let summary = summaries
        .iter()
        .find(|summary| summary.channel_id == ctx.channel_id)
        .unwrap();
    assert_eq!(summary.mention_count, 1);

    let _: Value = update_message!(
        &app,
        mentioned["id"].as_i64().unwrap(),
        &bob,
        "mention removed"
    );
    let summaries = read_state::read_state_snapshot(&ctx.db, alice.user_id)
        .await
        .unwrap();
    let summary = summaries
        .iter()
        .find(|summary| summary.channel_id == ctx.channel_id)
        .unwrap();
    assert_eq!(summary.mention_count, 0);
}

#[actix_web::test]
async fn hard_deleted_messages_disappear_from_unread_summaries() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "pw").await;
    let bob = insert_user(&ctx.db, "bob").await;
    upsert_read_state(&ctx.db, alice.user_id, ctx.channel_id, 0, 0).await;
    let message = insert_message_at(&ctx.db, bob.id, ctx.channel_id, None, 100, 80).await;

    let before = read_state::read_state_snapshot(&ctx.db, alice.user_id)
        .await
        .unwrap();
    assert!(before[0].has_unread);

    let active: entity::message::ActiveModel = message.into();
    active.delete(&ctx.db).await.unwrap();

    let after = read_state::read_state_snapshot(&ctx.db, alice.user_id)
        .await
        .unwrap();
    assert!(!after[0].has_unread);
    assert_eq!(after[0].mention_count, 0);
}

#[actix_web::test]
async fn read_state_snapshot_excludes_self_deleted_thread_and_voice_messages() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "pw").await;
    let bob = insert_user(&ctx.db, "bob").await;
    let voice = insert_channel(&ctx.db, "voice", "voice", 1).await;
    upsert_read_state(&ctx.db, alice.user_id, ctx.channel_id, 0, 0).await;
    insert_message_at(&ctx.db, alice.user_id, ctx.channel_id, None, 10, 10).await;
    let deleted = insert_message_at(&ctx.db, bob.id, ctx.channel_id, None, 20, 20).await;
    tombstone_message(&ctx.db, deleted.id).await;
    let root = insert_message_at(&ctx.db, bob.id, ctx.channel_id, None, 30, 30).await;
    upsert_read_state(
        &ctx.db,
        alice.user_id,
        ctx.channel_id,
        root.created_at,
        root.id,
    )
    .await;
    insert_message_at(&ctx.db, bob.id, ctx.channel_id, Some(root.id), 40, 40).await;
    insert_message_at(&ctx.db, bob.id, voice.id, None, 50, 50).await;

    let summaries = read_state::read_state_snapshot(&ctx.db, alice.user_id)
        .await
        .unwrap();

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].channel_id, ctx.channel_id);
    assert!(!summaries[0].has_unread);
    assert_eq!(summaries[0].mention_count, 0);
}

async fn unique_db(label: &str) -> DatabaseConnection {
    connect_initialized_database_url(&format!(
        "sqlite:file:hamlet_read_state_test_{label}_{}?mode=memory&cache=shared",
        generate_id()
    ))
    .await
    .unwrap()
}

async fn insert_user(db: &DatabaseConnection, username: &str) -> entity::user::Model {
    entity::user::ActiveModel {
        id: Set(generate_id()),
        username: Set(username.to_owned()),
        display_name: Set(None),
        email: Set(None),
        email_verified: Set(false),
        avatar_path: Set(None),
        avatar_updated_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap()
}

async fn insert_channel(
    db: &DatabaseConnection,
    name: &str,
    channel_type: &str,
    position: i64,
) -> entity::channel::Model {
    entity::channel::ActiveModel {
        id: Set(generate_id()),
        name: Set(name.to_owned()),
        position: Set(position),
        channel_type: Set(channel_type.to_owned()),
    }
    .insert(db)
    .await
    .unwrap()
}

async fn insert_message_at(
    db: &DatabaseConnection,
    user_id: i64,
    channel_id: i64,
    parent_id: Option<i64>,
    created_at: i64,
    message_id: i64,
) -> entity::message::Model {
    entity::message::ActiveModel {
        id: Set(message_id),
        user_id: Set(user_id),
        channel_id: Set(channel_id),
        parent_id: Set(parent_id),
        reply_to_message_id: Set(None),
        created_at: Set(created_at),
        deleted_at: Set(None),
        text: Set("message".to_owned()),
        suppress_embeds: Set(false),
    }
    .insert(db)
    .await
    .unwrap()
}

async fn tombstone_message(db: &DatabaseConnection, message_id: i64) {
    let message = entity::message::Entity::find_by_id(message_id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    let mut active: entity::message::ActiveModel = message.into();
    active.deleted_at = Set(Some(99));
    active.text = Set(String::new());
    active.update(db).await.unwrap();
}

async fn upsert_read_state(
    db: &DatabaseConnection,
    user_id: i64,
    channel_id: i64,
    last_read_created_at: i64,
    last_read_message_id: i64,
) {
    if let Some(existing) = read_state_row(db, user_id, channel_id).await {
        let mut active: entity::user_channel_read_state::ActiveModel = existing.into();
        active.last_read_created_at = Set(last_read_created_at);
        active.last_read_message_id = Set(last_read_message_id);
        active.updated_at = Set(123);
        active.update(db).await.unwrap();
    } else {
        entity::user_channel_read_state::ActiveModel {
            user_id: Set(user_id),
            channel_id: Set(channel_id),
            last_read_created_at: Set(last_read_created_at),
            last_read_message_id: Set(last_read_message_id),
            updated_at: Set(123),
        }
        .insert(db)
        .await
        .unwrap();
    }
}

async fn read_state_row(
    db: &DatabaseConnection,
    user_id: i64,
    channel_id: i64,
) -> Option<entity::user_channel_read_state::Model> {
    entity::user_channel_read_state::Entity::find()
        .filter(entity::user_channel_read_state::Column::UserId.eq(user_id))
        .filter(entity::user_channel_read_state::Column::ChannelId.eq(channel_id))
        .one(db)
        .await
        .unwrap()
}
