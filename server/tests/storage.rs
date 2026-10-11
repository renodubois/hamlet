use hamlet::connect_to_database;
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement, TransactionTrait};
use sea_orm_migration::MigratorTrait;

#[actix_web::test]
async fn initialization_adds_nullable_message_deletion_without_losing_retained_rows() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("existing.db").display()
    );
    let old_db = Database::connect(&url).await.unwrap();
    hamlet_migration::Migrator::up(&old_db, Some(4))
        .await
        .unwrap();
    for sql in [
        "INSERT INTO users VALUES (100000000000001, 'Alice', 'alice', 'retained password hash')",
        "INSERT INTO sessions VALUES ('retained digest', 100000000000001, '2099-01-01T00:00:00Z')",
        "INSERT INTO channels VALUES (100000000000002, 'Active', 'active', 'text', NULL), (100000000000003, 'Removed', 'removed', 'text', '2020-01-02T00:00:00Z')",
        "INSERT INTO messages VALUES (100000000000004, 100000000000002, 100000000000001, 'old active message', '1999-01-01T00:00:00.000000000Z'), (100000000000005, 100000000000003, 100000000000001, 'old retained message', '1999-01-02T00:00:00.000000000Z')",
    ] {
        old_db.execute_unprepared(sql).await.unwrap();
    }
    old_db.close().await.unwrap();
    let state = connect_to_database(&url).await.unwrap();
    let rows = state.db.query_all_raw(Statement::from_string(DbBackend::Sqlite,
        "SELECT id, channel_id, author_id, text, created_at, deleted_at FROM messages ORDER BY id")).await.unwrap();
    assert_eq!(rows.len(), 2);
    for (row, id, channel, text, created) in [
        (
            &rows[0],
            100_000_000_000_004_i64,
            100_000_000_000_002_i64,
            "old active message",
            "1999-01-01T00:00:00.000000000Z",
        ),
        (
            &rows[1],
            100_000_000_000_005_i64,
            100_000_000_000_003_i64,
            "old retained message",
            "1999-01-02T00:00:00.000000000Z",
        ),
    ] {
        assert_eq!(row.try_get::<i64>("", "id").unwrap(), id);
        assert_eq!(row.try_get::<i64>("", "channel_id").unwrap(), channel);
        assert_eq!(
            row.try_get::<i64>("", "author_id").unwrap(),
            100_000_000_000_001
        );
        assert_eq!(row.try_get::<String>("", "text").unwrap(), text);
        assert_eq!(row.try_get::<String>("", "created_at").unwrap(), created);
        assert_eq!(
            row.try_get::<Option<String>>("", "deleted_at").unwrap(),
            None
        );
    }
    // Initialization is repeatable, including after messages have been marked.
    state
        .db
        .execute_unprepared(
            "UPDATE messages SET deleted_at = '2026-01-01T00:00:00Z' WHERE id = 100000000000004",
        )
        .await
        .unwrap();
    state.db.close().await.unwrap();
    let restarted = connect_to_database(&url).await.unwrap();
    let row = restarted
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT text, deleted_at FROM messages WHERE id = 100000000000004",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<String>("", "text").unwrap(),
        "old active message"
    );
    assert_eq!(
        row.try_get::<String>("", "deleted_at").unwrap(),
        "2026-01-01T00:00:00Z"
    );
    let row = restarted
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS n FROM sessions WHERE token_digest = 'retained digest'",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
}

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
