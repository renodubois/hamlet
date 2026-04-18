#![allow(dead_code, clippy::unwrap_used)]

use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, Set};

use hamlet::{auth, entity, generate_id};

pub async fn setup_db() -> (DatabaseConnection, i64) {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.get_schema_registry("hamlet::entity::*")
        .sync(&db)
        .await
        .unwrap();

    let chan_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(chan_id),
        name: Set("general".to_owned()),
        position: Set(0),
    }
    .insert(&db)
    .await
    .unwrap();

    (db, chan_id)
}

pub fn session_cookie_header(token: &str) -> (String, String) {
    (
        "Cookie".to_owned(),
        format!("{}={}", auth::SESSION_COOKIE, token),
    )
}
