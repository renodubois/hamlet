use hamlet::connect_to_database;
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};

#[actix_web::test]
async fn migrations_and_pool_configuration_survive_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("hamlet.db").display()
    );
    let app = connect_to_database(&url).await.unwrap();
    let mut connections = Vec::new();
    for _ in 0..5 {
        connections.push(app.db.begin().await.unwrap());
    }
    for tx in &connections {
        let fk = tx
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_keys",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fk.try_get::<i64>("", "foreign_keys").unwrap(), 1);
        let timeout = tx
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA busy_timeout",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(timeout.try_get::<i64>("", "timeout").unwrap(), 5000);
    }
    let result = connections[0].execute_raw(Statement::from_string(DbBackend::Sqlite,
        "INSERT INTO sessions (token_digest, user_id, expires_at) VALUES ('test', 999, '2020-01-01T00:00:00Z')")).await;
    assert!(result.is_err());
    drop(connections);
    let journal = app
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA journal_mode",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        journal.try_get::<String>("", "journal_mode").unwrap(),
        "wal"
    );
    connect_to_database(&url).await.unwrap();
}
