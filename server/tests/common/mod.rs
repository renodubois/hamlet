#![allow(dead_code, clippy::unwrap_used)]

use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, Set};

use hamlet::{auth, entity, generate_id};

pub async fn setup_db() -> (DatabaseConnection, i64) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let url = format!("sqlite:file:hamlet_api_test_{n}?mode=memory&cache=shared");
    let db = Database::connect(&url).await.unwrap();
    db.get_schema_registry("hamlet::entity::*")
        .sync(&db)
        .await
        .unwrap();

    let chan_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(chan_id),
        name: Set("general".to_owned()),
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
