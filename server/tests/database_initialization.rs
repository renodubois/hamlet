#![allow(clippy::unwrap_used)]

mod common;

use hamlet::{auth, entity, generate_id, now_unix_micros};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, PaginatorTrait, Set};

#[actix_web::test]
async fn common_test_database_builder_runs_runtime_initialization() {
    let (db, channel_id) = common::setup_db().await;
    let user = auth::register_user(&db, "initialized_fixture_user", "hunter2", None)
        .await
        .unwrap();

    assert_password_credential_is_database_unique(&db, user.id, &user.username).await;

    let message_id = common::insert_message(
        &db,
        user.id,
        channel_id,
        "message fixture with initialized constraints",
    )
    .await;
    assert_reaction_is_database_unique(&db, user.id, message_id).await;
    assert_active_emoji_names_are_database_unique(&db, user.id).await;
}

#[actix_web::test]
async fn named_in_memory_test_databases_are_unique_and_isolated() {
    let (first_db, _first_channel_id) = common::setup_db().await;
    let (second_db, _second_channel_id) = common::setup_db().await;

    assert_eq!(channel_count(&first_db).await, 1);
    assert_eq!(channel_count(&second_db).await, 1);

    let extra_channel_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(extra_channel_id),
        name: Set("first-db-only".to_owned()),
        position: Set(99),
        channel_type: Set("text".to_owned()),
    }
    .insert(&first_db)
    .await
    .unwrap();

    assert_eq!(channel_count(&first_db).await, 2);
    assert_eq!(channel_count(&second_db).await, 1);
    assert!(
        entity::channel::Entity::find_by_id(extra_channel_id)
            .one(&second_db)
            .await
            .unwrap()
            .is_none(),
        "a channel inserted into one named in-memory test database leaked into another"
    );
}

#[actix_web::test]
async fn test_context_app_deps_reuse_the_initialized_database() {
    let ctx = common::TestCtx::new().await;
    let user = ctx.register("app_deps_fixture_user", "hunter2").await;
    let _deps = ctx.deps();

    assert_password_credential_is_database_unique(&ctx.db, user.user_id, &user.username).await;
}

async fn assert_password_credential_is_database_unique(
    db: &DatabaseConnection,
    user_id: i64,
    username: &str,
) {
    let duplicate = entity::credential::ActiveModel {
        id: Set(generate_id()),
        user_id: Set(user_id),
        provider: Set(auth::PASSWORD_PROVIDER.to_owned()),
        external_id: Set(username.to_owned()),
        secret: Set(Some("different-hash".to_owned())),
    }
    .insert(db)
    .await;

    assert!(
        duplicate.is_err(),
        "test fixtures must go through database initialization so duplicate credentials are rejected"
    );
}

async fn assert_reaction_is_database_unique(
    db: &DatabaseConnection,
    user_id: i64,
    message_id: i64,
) {
    let now = now_unix_micros();
    entity::message_reaction::ActiveModel {
        id: Set(generate_id()),
        message_id: Set(message_id),
        user_id: Set(user_id),
        emoji_kind: Set("native".to_owned()),
        emoji: Set("✅".to_owned()),
        emoji_key: Set("native:✅".to_owned()),
        created_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap();

    let duplicate = entity::message_reaction::ActiveModel {
        id: Set(generate_id()),
        message_id: Set(message_id),
        user_id: Set(user_id),
        emoji_kind: Set("native".to_owned()),
        emoji: Set("✅".to_owned()),
        emoji_key: Set("native:✅".to_owned()),
        created_at: Set(now + 1),
    }
    .insert(db)
    .await;

    assert!(
        duplicate.is_err(),
        "test fixtures must go through database initialization so duplicate reactions are rejected"
    );
}

async fn assert_active_emoji_names_are_database_unique(db: &DatabaseConnection, user_id: i64) {
    let now = now_unix_micros();
    entity::emoji::ActiveModel {
        id: Set(generate_id()),
        image_path: Set("emoji/initialized-fixture.webp".to_owned()),
        name: Set("InitializedFixture".to_owned()),
        normalized_name: Set("initializedfixture".to_owned()),
        animated: Set(false),
        created_by_user_id: Set(user_id),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();

    let duplicate_active = entity::emoji::ActiveModel {
        id: Set(generate_id()),
        image_path: Set("emoji/initialized-fixture-duplicate.webp".to_owned()),
        name: Set("InitializedFixture".to_owned()),
        normalized_name: Set("initializedfixture".to_owned()),
        animated: Set(false),
        created_by_user_id: Set(user_id),
        created_at: Set(now + 1),
        updated_at: Set(now + 1),
        deleted_at: Set(None),
    }
    .insert(db)
    .await;

    assert!(
        duplicate_active.is_err(),
        "test fixtures must go through database initialization so duplicate active emoji names are rejected"
    );

    let deleted_duplicate = entity::emoji::ActiveModel {
        id: Set(generate_id()),
        image_path: Set("emoji/initialized-fixture-deleted.webp".to_owned()),
        name: Set("InitializedFixture".to_owned()),
        normalized_name: Set("initializedfixture".to_owned()),
        animated: Set(false),
        created_by_user_id: Set(user_id),
        created_at: Set(now + 2),
        updated_at: Set(now + 2),
        deleted_at: Set(Some(now + 2)),
    }
    .insert(db)
    .await;

    assert!(
        deleted_duplicate.is_ok(),
        "deleted emoji rows should not block name reuse in initialized fixtures"
    );
}

async fn channel_count(db: &DatabaseConnection) -> u64 {
    entity::channel::Entity::find().count(db).await.unwrap()
}
