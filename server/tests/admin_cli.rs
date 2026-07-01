#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};

use hamlet::{
    admin_cli::{AdminCliError, create_user_in_database},
    auth, connect_initialized_database_url, entity, generate_id,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

#[actix_web::test]
async fn create_user_in_database_creates_login_compatible_hashed_account() {
    let (database_url, db, channel_id) = setup_admin_cli_db().await;

    let created = create_user_in_database(&database_url, " alice ", "hunter2")
        .await
        .unwrap();

    assert_eq!(created.username, "alice");

    let credential = entity::credential::Entity::find()
        .filter(entity::credential::Column::Provider.eq(auth::PASSWORD_PROVIDER))
        .filter(entity::credential::Column::ExternalId.eq("alice"))
        .one(&db)
        .await
        .unwrap()
        .expect("password credential should exist");
    let secret = credential
        .secret
        .expect("password credential stores a secret");
    assert_ne!(secret, "hunter2");
    assert!(secret.starts_with("$argon2"));
    assert!(auth::verify_password("hunter2", &secret));

    let logged_in = auth::authenticate_password(&db, "alice", "hunter2")
        .await
        .unwrap();
    assert_eq!(logged_in.id, created.id);

    let read_state = entity::user_channel_read_state::Entity::find()
        .filter(entity::user_channel_read_state::Column::UserId.eq(created.id))
        .filter(entity::user_channel_read_state::Column::ChannelId.eq(channel_id))
        .one(&db)
        .await
        .unwrap();
    assert!(
        read_state.is_some(),
        "admin-created users should get read-state baselines through the shared registration path"
    );
}

#[actix_web::test]
async fn create_user_in_database_reports_duplicate_username() {
    let (database_url, _db, _channel_id) = setup_admin_cli_db().await;
    create_user_in_database(&database_url, "alice", "hunter2")
        .await
        .unwrap();

    let err = create_user_in_database(&database_url, "alice", "different")
        .await
        .expect_err("duplicate username should fail");

    match err {
        AdminCliError::UsernameTaken(username) => assert_eq!(username, "alice"),
        other => panic!("expected duplicate username error, got {other:?}"),
    }
}

async fn setup_admin_cli_db() -> (String, sea_orm::DatabaseConnection, i64) {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let database_url = format!("sqlite:file:hamlet_admin_cli_test_{n}?mode=memory&cache=shared");
    let db = connect_initialized_database_url(&database_url)
        .await
        .unwrap();

    let channel_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(channel_id),
        name: Set("general".to_owned()),
        position: Set(0),
        channel_type: Set("text".to_owned()),
    }
    .insert(&db)
    .await
    .unwrap();

    (database_url, db, channel_id)
}
