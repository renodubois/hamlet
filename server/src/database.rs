//! Database connection helpers and baseline schema initialization.
//!
//! Runtime defaults use a file-backed SQLite database. Explicit in-memory
//! SQLite URLs remain supported for tests and clean-room runs; SQLite keeps
//! those databases alive only while at least one connection to the named
//! in-memory database remains open, so the SQLx pool must keep a sentinel
//! connection around for the lifetime of the process.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sea_orm::sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, DbErr, Statement,
    TransactionTrait,
};
use thiserror::Error;

use crate::Config;

const SCHEMA_REGISTRY: &str = "hamlet::entity::*";
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug)]
struct SqliteIndexDefinition {
    name: &'static str,
    sql: &'static str,
}

const SQLITE_MANAGED_INDEXES: &[SqliteIndexDefinition] = &[
    SqliteIndexDefinition {
        name: "ux_credential_provider_external_id",
        sql: r#"CREATE UNIQUE INDEX IF NOT EXISTS "ux_credential_provider_external_id"
ON "credential" ("provider", "external_id")"#,
    },
    SqliteIndexDefinition {
        name: "ux_message_reaction_message_user_emoji_key",
        sql: r#"CREATE UNIQUE INDEX IF NOT EXISTS "ux_message_reaction_message_user_emoji_key"
ON "message_reaction" ("message_id", "user_id", "emoji_key")"#,
    },
    SqliteIndexDefinition {
        name: "ux_emoji_active_normalized_name",
        sql: r#"CREATE UNIQUE INDEX IF NOT EXISTS "ux_emoji_active_normalized_name"
ON "emoji" ("normalized_name")
WHERE "deleted_at" IS NULL"#,
    },
    SqliteIndexDefinition {
        name: "idx_message_channel_parent_created_id",
        sql: r#"CREATE INDEX IF NOT EXISTS "idx_message_channel_parent_created_id"
ON "message" ("channel_id", "parent_id", "created_at", "id")"#,
    },
    SqliteIndexDefinition {
        name: "idx_message_parent_created_id",
        sql: r#"CREATE INDEX IF NOT EXISTS "idx_message_parent_created_id"
ON "message" ("parent_id", "created_at", "id")"#,
    },
    SqliteIndexDefinition {
        name: "idx_message_reply_to_deleted_id",
        sql: r#"CREATE INDEX IF NOT EXISTS "idx_message_reply_to_deleted_id"
ON "message" ("reply_to_message_id", "deleted_at", "id")"#,
    },
    SqliteIndexDefinition {
        name: "idx_session_token_expires_user",
        sql: r#"CREATE INDEX IF NOT EXISTS "idx_session_token_expires_user"
ON "session" ("token", "expires_at", "user_id")"#,
    },
    SqliteIndexDefinition {
        name: "idx_message_attachment_message_position_id",
        sql: r#"CREATE INDEX IF NOT EXISTS "idx_message_attachment_message_position_id"
ON "message_attachment" ("message_id", "position", "id")"#,
    },
    SqliteIndexDefinition {
        name: "idx_embed_message_id_id",
        sql: r#"CREATE INDEX IF NOT EXISTS "idx_embed_message_id_id"
ON "embed" ("message_id", "id")"#,
    },
    SqliteIndexDefinition {
        name: "idx_message_reaction_message_created_id",
        sql: r#"CREATE INDEX IF NOT EXISTS "idx_message_reaction_message_created_id"
ON "message_reaction" ("message_id", "created_at", "id")"#,
    },
];

#[derive(Debug, Error)]
pub enum DatabaseSetupError {
    #[error("unsupported database URL {url:?}; only sqlite: URLs are supported")]
    UnsupportedUrl { url: String },
    #[error("invalid SQLite database URL {url:?}: {source}")]
    InvalidSqliteUrl {
        url: String,
        #[source]
        source: sea_orm::sqlx::Error,
    },
    #[error("unsupported SQLite file URI {filename:?} in database URL {url:?}")]
    UnsupportedSqliteFileUri { url: String, filename: String },
    #[error("could not create parent directory {path:?} for SQLite database URL {url:?}: {source}")]
    CreateParentDirectory {
        url: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not connect to SQLite database URL {url:?}: {source}")]
    Connect {
        url: String,
        #[source]
        source: DbErr,
    },
    #[error("could not initialize SQLite schema: {source}")]
    Initialize {
        #[source]
        source: DbErr,
    },
    #[error("could not apply SQLite schema migration step {step:?}: {source}")]
    Migration {
        step: &'static str,
        #[source]
        source: DbErr,
    },
    #[error("could not apply SQLite schema migration: {message}")]
    MigrationInvariant { message: String },
}

impl From<DatabaseSetupError> for std::io::Error {
    fn from(error: DatabaseSetupError) -> Self {
        std::io::Error::other(error.to_string())
    }
}

pub async fn connect_database(config: &Config) -> Result<DatabaseConnection, DatabaseSetupError> {
    connect_initialized_database_url(&config.database_url).await
}

pub async fn connect_initialized_database_url(
    database_url: &str,
) -> Result<DatabaseConnection, DatabaseSetupError> {
    let db = connect_database_url(database_url).await?;
    initialize_database(&db).await?;
    Ok(db)
}

pub(crate) async fn connect_database_url(
    database_url: &str,
) -> Result<DatabaseConnection, DatabaseSetupError> {
    let sqlite_options = parse_sqlite_options(database_url)?;
    prepare_sqlite_database_path(database_url, &sqlite_options)?;

    Database::connect(connection_options(database_url))
        .await
        .map_err(|source| DatabaseSetupError::Connect {
            url: database_url.to_owned(),
            source,
        })
}

pub(crate) async fn initialize_database(db: &DatabaseConnection) -> Result<(), DatabaseSetupError> {
    db.get_schema_registry(SCHEMA_REGISTRY)
        .sync(db)
        .await
        .map_err(|source| DatabaseSetupError::Initialize { source })?;
    apply_sqlite_integrity_migrations(db).await
}

async fn apply_sqlite_integrity_migrations(
    db: &DatabaseConnection,
) -> Result<(), DatabaseSetupError> {
    let txn = db
        .begin()
        .await
        .map_err(|source| migration_error("begin integrity/index migration", source))?;

    reject_duplicate_credentials(&txn).await?;
    reject_duplicate_active_emoji_names(&txn).await?;
    deduplicate_message_reactions(&txn).await?;

    for index in SQLITE_MANAGED_INDEXES {
        execute_sqlite_statement(&txn, index.name, index.sql).await?;
    }

    txn.commit()
        .await
        .map_err(|source| migration_error("commit integrity/index migration", source))
}

async fn reject_duplicate_credentials<C>(db: &C) -> Result<(), DatabaseSetupError>
where
    C: ConnectionTrait,
{
    let rows = query_sqlite_rows(
        db,
        "detect duplicate credentials",
        r#"SELECT "provider", "external_id", COUNT(*) AS "duplicate_count"
FROM "credential"
GROUP BY "provider", "external_id"
HAVING COUNT(*) > 1
ORDER BY "provider", "external_id"
LIMIT 10"#,
    )
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    let mut examples = Vec::with_capacity(rows.len());
    for row in rows {
        let provider: String = row
            .try_get("", "provider")
            .map_err(|source| migration_error("read duplicate credential provider", source))?;
        let external_id: String = row
            .try_get("", "external_id")
            .map_err(|source| migration_error("read duplicate credential external_id", source))?;
        let count: i64 = row
            .try_get("", "duplicate_count")
            .map_err(|source| migration_error("read duplicate credential count", source))?;
        examples.push(format!(
            "provider={provider:?}, external_id={external_id:?} has {count} rows"
        ));
    }

    Err(DatabaseSetupError::MigrationInvariant {
        message: format!(
            "duplicate credentials for provider/external_id prevent adding the unique constraint: {}",
            examples.join("; ")
        ),
    })
}

async fn reject_duplicate_active_emoji_names<C>(db: &C) -> Result<(), DatabaseSetupError>
where
    C: ConnectionTrait,
{
    let rows = query_sqlite_rows(
        db,
        "detect duplicate active custom emoji names",
        r#"SELECT "normalized_name", COUNT(*) AS "duplicate_count"
FROM "emoji"
WHERE "deleted_at" IS NULL
GROUP BY "normalized_name"
HAVING COUNT(*) > 1
ORDER BY "normalized_name"
LIMIT 10"#,
    )
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    let mut examples = Vec::with_capacity(rows.len());
    for row in rows {
        let normalized_name: String = row
            .try_get("", "normalized_name")
            .map_err(|source| migration_error("read duplicate active emoji name", source))?;
        let count: i64 = row
            .try_get("", "duplicate_count")
            .map_err(|source| migration_error("read duplicate active emoji count", source))?;
        examples.push(format!(
            "normalized_name={normalized_name:?} has {count} active rows"
        ));
    }

    Err(DatabaseSetupError::MigrationInvariant {
        message: format!(
            "duplicate active custom emoji names prevent adding the partial unique constraint: {}",
            examples.join("; ")
        ),
    })
}

async fn deduplicate_message_reactions<C>(db: &C) -> Result<(), DatabaseSetupError>
where
    C: ConnectionTrait,
{
    execute_sqlite_statement(
        db,
        "deduplicate message reactions",
        r#"DELETE FROM "message_reaction"
WHERE EXISTS (
    SELECT 1
    FROM "message_reaction" AS "keeper"
    WHERE "keeper"."message_id" = "message_reaction"."message_id"
      AND "keeper"."user_id" = "message_reaction"."user_id"
      AND "keeper"."emoji_key" = "message_reaction"."emoji_key"
      AND (
          "keeper"."created_at" < "message_reaction"."created_at"
          OR (
              "keeper"."created_at" = "message_reaction"."created_at"
              AND "keeper"."id" < "message_reaction"."id"
          )
      )
)"#,
    )
    .await
}

async fn execute_sqlite_statement<C>(
    db: &C,
    step: &'static str,
    sql: &'static str,
) -> Result<(), DatabaseSetupError>
where
    C: ConnectionTrait,
{
    db.execute_raw(Statement::from_string(DbBackend::Sqlite, sql.to_owned()))
        .await
        .map(|_| ())
        .map_err(|source| migration_error(step, source))
}

async fn query_sqlite_rows<C>(
    db: &C,
    step: &'static str,
    sql: &'static str,
) -> Result<Vec<sea_orm::QueryResult>, DatabaseSetupError>
where
    C: ConnectionTrait,
{
    db.query_all_raw(Statement::from_string(DbBackend::Sqlite, sql.to_owned()))
        .await
        .map_err(|source| migration_error(step, source))
}

fn migration_error(step: &'static str, source: DbErr) -> DatabaseSetupError {
    DatabaseSetupError::Migration { step, source }
}

fn parse_sqlite_options(database_url: &str) -> Result<SqliteConnectOptions, DatabaseSetupError> {
    if !database_url.to_ascii_lowercase().starts_with("sqlite:") {
        return Err(DatabaseSetupError::UnsupportedUrl {
            url: database_url.to_owned(),
        });
    }

    database_url
        .parse::<SqliteConnectOptions>()
        .map_err(|source| DatabaseSetupError::InvalidSqliteUrl {
            url: database_url.to_owned(),
            source,
        })
}

fn prepare_sqlite_database_path(
    database_url: &str,
    sqlite_options: &SqliteConnectOptions,
) -> Result<(), DatabaseSetupError> {
    if is_in_memory_sqlite(database_url) {
        return Ok(());
    }

    let database_path = sqlite_file_path(database_url, sqlite_options.get_filename())?;
    let Some(parent) = database_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };

    std::fs::create_dir_all(parent).map_err(|source| DatabaseSetupError::CreateParentDirectory {
        url: database_url.to_owned(),
        path: parent.to_path_buf(),
        source,
    })
}

fn sqlite_file_path(database_url: &str, filename: &Path) -> Result<PathBuf, DatabaseSetupError> {
    let filename = filename.to_string_lossy();

    if let Some(rest) = filename.strip_prefix("file://") {
        if !rest.starts_with('/') {
            return Err(DatabaseSetupError::UnsupportedSqliteFileUri {
                url: database_url.to_owned(),
                filename: filename.into_owned(),
            });
        }

        return Ok(PathBuf::from(rest));
    }

    if let Some(rest) = filename.strip_prefix("file:") {
        return Ok(PathBuf::from(rest));
    }

    Ok(PathBuf::from(filename.into_owned()))
}

fn connection_options(database_url: &str) -> ConnectOptions {
    let mut options = ConnectOptions::new(database_url.to_owned());
    let database_is_in_memory = is_in_memory_sqlite(database_url);

    if database_url.to_ascii_lowercase().starts_with("sqlite:") {
        options.map_sqlx_sqlite_opts(move |sqlite_options| {
            let sqlite_options = sqlite_options
                .busy_timeout(SQLITE_BUSY_TIMEOUT)
                .foreign_keys(true);

            if database_is_in_memory {
                sqlite_options
            } else {
                sqlite_options
                    .create_if_missing(true)
                    .journal_mode(SqliteJournalMode::Wal)
                    .synchronous(SqliteSynchronous::Normal)
            }
        });
    }

    if database_is_in_memory {
        // SQLx's default pool can reap every idle connection. For SQLite
        // in-memory databases that destroys the database itself, so later
        // requests see a fresh empty DB (`no such table: ...`). Keep one
        // connection alive and disable age/idle reaping for that sentinel.
        options
            .min_connections(1)
            .idle_timeout(None::<Duration>)
            .max_lifetime(None::<Duration>);
    }

    options
}

fn is_in_memory_sqlite(database_url: &str) -> bool {
    let lower = database_url.to_ascii_lowercase();
    lower.starts_with("sqlite:") && (lower.contains(":memory:") || lower.contains("mode=memory"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use sea_orm::{
        ActiveModelTrait, ConnectionTrait, DbBackend, EntityTrait, PaginatorTrait, QueryResult,
        Set, Statement,
    };

    use super::*;
    use crate::entity;
    use crate::util::{generate_id, now_unix_micros};

    #[test]
    fn shared_memory_sqlite_keeps_a_sentinel_connection_alive() {
        let options = connection_options("sqlite:file::memory:?cache=shared");

        assert_eq!(options.get_min_connections(), Some(1));
        assert_eq!(options.get_idle_timeout(), Some(None));
        assert_eq!(options.get_max_lifetime(), Some(None));
    }

    #[test]
    fn named_memory_sqlite_keeps_a_sentinel_connection_alive() {
        let options = connection_options("sqlite:file:hamlet_api_test?mode=memory&cache=shared");

        assert_eq!(options.get_min_connections(), Some(1));
        assert_eq!(options.get_idle_timeout(), Some(None));
        assert_eq!(options.get_max_lifetime(), Some(None));
    }

    #[test]
    fn file_backed_sqlite_uses_pool_defaults() {
        let options = connection_options("sqlite://hamlet.db?mode=rwc");

        assert_eq!(options.get_min_connections(), None);
        assert_eq!(options.get_idle_timeout(), None);
        assert_eq!(options.get_max_lifetime(), None);
    }

    #[actix_web::test]
    async fn file_backed_sqlite_prepares_parent_dirs_and_pragmas() {
        let root = tmp_database_dir();
        let db_path = root.join("nested").join("hamlet.db");
        let database_url = sqlite_url(&db_path);

        let db = connect_database_url(&database_url).await.unwrap();

        assert!(db_path.parent().unwrap().is_dir());
        assert_eq!(pragma_i64(&db, "foreign_keys").await, 1);
        assert_eq!(pragma_i64(&db, "busy_timeout").await, 5_000);
        assert_eq!(pragma_string(&db, "journal_mode").await, "wal");

        db.close().await.unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[actix_web::test]
    async fn initialization_is_idempotent_and_file_backed_rows_survive_reconnect() {
        let root = tmp_database_dir();
        let db_path = root.join("persistent").join("hamlet.db");
        let database_url = sqlite_url(&db_path);

        let db = connect_initialized_database_url(&database_url)
            .await
            .unwrap();
        assert_managed_indexes_exist(&db).await;
        let ids = insert_representative_rows(&db).await;
        initialize_database(&db).await.unwrap();
        assert_managed_indexes_exist(&db).await;
        assert_representative_rows(&db, &ids).await;
        db.close().await.unwrap();

        let reconnected = connect_database_url(&database_url).await.unwrap();
        initialize_database(&reconnected).await.unwrap();
        assert_managed_indexes_exist(&reconnected).await;
        assert_representative_rows(&reconnected, &ids).await;
        reconnected.close().await.unwrap();

        let _ = std::fs::remove_dir_all(root);
    }

    #[actix_web::test]
    async fn fresh_database_gets_integrity_constraints_and_read_indexes() {
        let url = memory_sqlite_url("fresh_indexes");
        let db = connect_initialized_database_url(&url).await.unwrap();

        assert_managed_indexes_exist(&db).await;
    }

    #[actix_web::test]
    async fn managed_constraints_reject_new_duplicates_but_allow_deleted_emoji_name_reuse() {
        let url = memory_sqlite_url("managed_constraints");
        let db = connect_initialized_database_url(&url).await.unwrap();
        let ids = insert_representative_rows(&db).await;

        let duplicate_credential = entity::credential::ActiveModel {
            id: Set(ids.credential_id + 1),
            user_id: Set(ids.user_id),
            provider: Set("password".to_owned()),
            external_id: Set("persisted".to_owned()),
            secret: Set(Some("other-hash".to_owned())),
        }
        .insert(&db)
        .await;
        assert!(duplicate_credential.is_err());

        let duplicate_reaction = entity::message_reaction::ActiveModel {
            id: Set(ids.reaction_id + 1),
            message_id: Set(ids.message_id),
            user_id: Set(ids.user_id),
            emoji_kind: Set("native".to_owned()),
            emoji: Set("💾".to_owned()),
            emoji_key: Set("native:💾".to_owned()),
            created_at: Set(0),
        }
        .insert(&db)
        .await;
        assert!(duplicate_reaction.is_err());

        let duplicate_active_emoji = entity::emoji::ActiveModel {
            id: Set(ids.emoji_id + 1),
            image_path: Set("emoji/active-duplicate.webp".to_owned()),
            name: Set("Persisted".to_owned()),
            normalized_name: Set("persisted".to_owned()),
            animated: Set(false),
            created_by_user_id: Set(ids.user_id),
            created_at: Set(0),
            updated_at: Set(0),
            deleted_at: Set(None),
        }
        .insert(&db)
        .await;
        assert!(duplicate_active_emoji.is_err());

        let deleted_duplicate_emoji = entity::emoji::ActiveModel {
            id: Set(ids.emoji_id + 2),
            image_path: Set("emoji/deleted-duplicate.webp".to_owned()),
            name: Set("Persisted".to_owned()),
            normalized_name: Set("persisted".to_owned()),
            animated: Set(false),
            created_by_user_id: Set(ids.user_id),
            created_at: Set(0),
            updated_at: Set(0),
            deleted_at: Set(Some(1)),
        }
        .insert(&db)
        .await;
        assert!(deleted_duplicate_emoji.is_ok());
    }

    #[actix_web::test]
    async fn duplicate_reactions_are_deduplicated_by_oldest_before_indexing() {
        let url = memory_sqlite_url("reaction_dedup");
        let db = connect_database_url(&url).await.unwrap();
        sync_baseline_schema(&db).await;
        let ids = insert_representative_rows(&db).await;
        let older_reaction_id = ids.reaction_id + 1;
        let same_timestamp_reaction_id = ids.reaction_id + 2;

        entity::message_reaction::ActiveModel {
            id: Set(older_reaction_id),
            message_id: Set(ids.message_id),
            user_id: Set(ids.user_id),
            emoji_kind: Set("native".to_owned()),
            emoji: Set("💾".to_owned()),
            emoji_key: Set("native:💾".to_owned()),
            created_at: Set(0),
        }
        .insert(&db)
        .await
        .unwrap();
        entity::message_reaction::ActiveModel {
            id: Set(same_timestamp_reaction_id),
            message_id: Set(ids.message_id),
            user_id: Set(ids.user_id),
            emoji_kind: Set("native".to_owned()),
            emoji: Set("💾".to_owned()),
            emoji_key: Set("native:💾".to_owned()),
            created_at: Set(0),
        }
        .insert(&db)
        .await
        .unwrap();

        initialize_database(&db).await.unwrap();
        assert_managed_indexes_exist(&db).await;

        let reactions = entity::message_reaction::Entity::find()
            .all(&db)
            .await
            .unwrap();
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].id, older_reaction_id);

        let duplicate_after_index = entity::message_reaction::ActiveModel {
            id: Set(same_timestamp_reaction_id + 1),
            message_id: Set(ids.message_id),
            user_id: Set(ids.user_id),
            emoji_kind: Set("native".to_owned()),
            emoji: Set("💾".to_owned()),
            emoji_key: Set("native:💾".to_owned()),
            created_at: Set(1),
        }
        .insert(&db)
        .await;
        assert!(duplicate_after_index.is_err());
    }

    #[actix_web::test]
    async fn duplicate_credentials_fail_with_clear_migration_error() {
        let url = memory_sqlite_url("duplicate_credentials");
        let db = connect_database_url(&url).await.unwrap();
        sync_baseline_schema(&db).await;
        let ids = insert_representative_rows(&db).await;

        entity::credential::ActiveModel {
            id: Set(ids.credential_id + 1),
            user_id: Set(ids.user_id),
            provider: Set("password".to_owned()),
            external_id: Set("persisted".to_owned()),
            secret: Set(Some("other-hash".to_owned())),
        }
        .insert(&db)
        .await
        .unwrap();

        let err = initialize_database(&db).await.unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, DatabaseSetupError::MigrationInvariant { .. }));
        assert!(message.contains("duplicate credentials"));
        assert!(message.contains("provider=\"password\""));
        assert!(message.contains("external_id=\"persisted\""));
    }

    #[actix_web::test]
    async fn duplicate_active_emoji_names_fail_with_clear_migration_error() {
        let url = memory_sqlite_url("duplicate_active_emoji");
        let db = connect_database_url(&url).await.unwrap();
        sync_baseline_schema(&db).await;
        let ids = insert_representative_rows(&db).await;

        entity::emoji::ActiveModel {
            id: Set(ids.emoji_id + 1),
            image_path: Set("emoji/active-duplicate.webp".to_owned()),
            name: Set("Persisted".to_owned()),
            normalized_name: Set("persisted".to_owned()),
            animated: Set(false),
            created_by_user_id: Set(ids.user_id),
            created_at: Set(0),
            updated_at: Set(0),
            deleted_at: Set(None),
        }
        .insert(&db)
        .await
        .unwrap();

        let err = initialize_database(&db).await.unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, DatabaseSetupError::MigrationInvariant { .. }));
        assert!(message.contains("duplicate active custom emoji names"));
        assert!(message.contains("normalized_name=\"persisted\""));
    }

    #[actix_web::test]
    async fn deleted_emoji_records_do_not_block_name_reuse_when_indexing() {
        let url = memory_sqlite_url("deleted_emoji_reuse");
        let db = connect_database_url(&url).await.unwrap();
        sync_baseline_schema(&db).await;
        let ids = insert_representative_rows(&db).await;

        entity::emoji::ActiveModel {
            id: Set(ids.emoji_id + 1),
            image_path: Set("emoji/deleted-duplicate.webp".to_owned()),
            name: Set("Persisted".to_owned()),
            normalized_name: Set("persisted".to_owned()),
            animated: Set(false),
            created_by_user_id: Set(ids.user_id),
            created_at: Set(0),
            updated_at: Set(0),
            deleted_at: Set(Some(1)),
        }
        .insert(&db)
        .await
        .unwrap();

        initialize_database(&db).await.unwrap();
        assert_managed_indexes_exist(&db).await;
    }

    #[actix_web::test]
    async fn unsupported_database_scheme_returns_typed_error() {
        let err = connect_database_url("postgres://localhost/hamlet")
            .await
            .unwrap_err();

        assert!(matches!(err, DatabaseSetupError::UnsupportedUrl { .. }));
        assert!(err.to_string().contains("only sqlite: URLs are supported"));
    }

    #[actix_web::test]
    async fn invalid_sqlite_url_returns_typed_error() {
        let err = connect_database_url("sqlite://hamlet.db?unknown=true")
            .await
            .unwrap_err();

        assert!(matches!(err, DatabaseSetupError::InvalidSqliteUrl { .. }));
        assert!(err.to_string().contains("unknown query parameter"));
    }

    #[actix_web::test]
    async fn unsupported_sqlite_file_uri_returns_typed_error() {
        let err = connect_database_url("sqlite:file://example.com/tmp/hamlet.db?mode=rwc")
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            DatabaseSetupError::UnsupportedSqliteFileUri { .. }
        ));
    }

    #[actix_web::test]
    async fn parent_directory_setup_failure_returns_typed_error() {
        let root = tmp_database_dir();
        std::fs::create_dir_all(&root).unwrap();
        let blocking_file = root.join("not-a-directory");
        std::fs::write(&blocking_file, b"not a directory").unwrap();
        let db_path = blocking_file.join("child").join("hamlet.db");
        let database_url = sqlite_url(&db_path);

        let err = connect_database_url(&database_url).await.unwrap_err();

        assert!(matches!(
            err,
            DatabaseSetupError::CreateParentDirectory { .. }
        ));
        assert!(
            err.to_string()
                .contains("could not create parent directory")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    struct RepresentativeIds {
        user_id: i64,
        credential_id: i64,
        session_token: String,
        channel_id: i64,
        message_id: i64,
        embed_id: i64,
        attachment_id: i64,
        reaction_id: i64,
        emoji_id: i64,
    }

    async fn insert_representative_rows(db: &DatabaseConnection) -> RepresentativeIds {
        let ids = RepresentativeIds {
            user_id: generate_id(),
            credential_id: generate_id(),
            session_token: format!("session-{}", generate_id()),
            channel_id: generate_id(),
            message_id: generate_id(),
            embed_id: generate_id(),
            attachment_id: generate_id(),
            reaction_id: generate_id(),
            emoji_id: generate_id(),
        };
        let now = now_unix_micros();

        entity::user::ActiveModel {
            id: Set(ids.user_id),
            username: Set("persisted".to_owned()),
            display_name: Set(Some("Persisted User".to_owned())),
            email: Set(Some("persisted@example.test".to_owned())),
            email_verified: Set(true),
            avatar_path: Set(Some("avatars/persisted.webp".to_owned())),
            avatar_updated_at: Set(Some(now)),
        }
        .insert(db)
        .await
        .unwrap();

        entity::credential::ActiveModel {
            id: Set(ids.credential_id),
            user_id: Set(ids.user_id),
            provider: Set("password".to_owned()),
            external_id: Set("persisted".to_owned()),
            secret: Set(Some("hashed-password".to_owned())),
        }
        .insert(db)
        .await
        .unwrap();

        entity::session::ActiveModel {
            token: Set(ids.session_token.clone()),
            user_id: Set(ids.user_id),
            created_at: Set(now),
            expires_at: Set(now + 60_000_000),
        }
        .insert(db)
        .await
        .unwrap();

        entity::channel::ActiveModel {
            id: Set(ids.channel_id),
            name: Set("persistent-general".to_owned()),
            position: Set(0),
            channel_type: Set("text".to_owned()),
        }
        .insert(db)
        .await
        .unwrap();

        entity::message::ActiveModel {
            id: Set(ids.message_id),
            user_id: Set(ids.user_id),
            channel_id: Set(ids.channel_id),
            parent_id: Set(None),
            reply_to_message_id: Set(None),
            created_at: Set(now),
            deleted_at: Set(None),
            text: Set("persistent hello".to_owned()),
            suppress_embeds: Set(false),
        }
        .insert(db)
        .await
        .unwrap();

        entity::embed::ActiveModel {
            id: Set(ids.embed_id),
            message_id: Set(ids.message_id),
            url: Set("https://example.test/hamlet".to_owned()),
            title: Set(Some("Hamlet".to_owned())),
            description: Set(Some("Persistent embed".to_owned())),
            image_url: Set(Some("https://example.test/image.png".to_owned())),
            site_name: Set(Some("Example".to_owned())),
            embed_type: Set("link".to_owned()),
            iframe_url: Set(None),
            iframe_width: Set(None),
            iframe_height: Set(None),
        }
        .insert(db)
        .await
        .unwrap();

        entity::message_attachment::ActiveModel {
            id: Set(ids.attachment_id),
            message_id: Set(ids.message_id),
            position: Set(0),
            content_type: Set("image/png".to_owned()),
            byte_size: Set(1024),
            width: Set(640),
            height: Set(480),
            storage_path: Set("messages/full.png".to_owned()),
            thumbnail_content_type: Set("image/webp".to_owned()),
            thumbnail_byte_size: Set(256),
            thumbnail_width: Set(160),
            thumbnail_height: Set(120),
            thumbnail_storage_path: Set("messages/thumb.webp".to_owned()),
            created_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();

        entity::message_reaction::ActiveModel {
            id: Set(ids.reaction_id),
            message_id: Set(ids.message_id),
            user_id: Set(ids.user_id),
            emoji_kind: Set("native".to_owned()),
            emoji: Set("💾".to_owned()),
            emoji_key: Set("native:💾".to_owned()),
            created_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();

        entity::emoji::ActiveModel {
            id: Set(ids.emoji_id),
            image_path: Set("emoji/persisted.webp".to_owned()),
            name: Set("persisted".to_owned()),
            normalized_name: Set("persisted".to_owned()),
            animated: Set(false),
            created_by_user_id: Set(ids.user_id),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
        }
        .insert(db)
        .await
        .unwrap();

        ids
    }

    async fn assert_representative_rows(db: &DatabaseConnection, ids: &RepresentativeIds) {
        let message = entity::message::Entity::find_by_id(ids.message_id)
            .one(db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(message.text, "persistent hello");

        assert_eq!(entity::user::Entity::find().count(db).await.unwrap(), 1);
        assert_eq!(
            entity::credential::Entity::find().count(db).await.unwrap(),
            1
        );
        assert_eq!(entity::session::Entity::find().count(db).await.unwrap(), 1);
        assert_eq!(entity::channel::Entity::find().count(db).await.unwrap(), 1);
        assert_eq!(entity::message::Entity::find().count(db).await.unwrap(), 1);
        assert_eq!(entity::embed::Entity::find().count(db).await.unwrap(), 1);
        assert_eq!(
            entity::message_attachment::Entity::find()
                .count(db)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            entity::message_reaction::Entity::find()
                .count(db)
                .await
                .unwrap(),
            1
        );
        assert_eq!(entity::emoji::Entity::find().count(db).await.unwrap(), 1);
    }

    async fn sync_baseline_schema(db: &DatabaseConnection) {
        db.get_schema_registry(SCHEMA_REGISTRY)
            .sync(db)
            .await
            .unwrap();
    }

    async fn assert_managed_indexes_exist(db: &DatabaseConnection) {
        let names = sqlite_index_names(db).await;
        for index in SQLITE_MANAGED_INDEXES {
            assert!(
                names.iter().any(|name| name == index.name),
                "missing managed SQLite index {} in {:?}",
                index.name,
                names
            );
        }
    }

    async fn sqlite_index_names(db: &DatabaseConnection) -> Vec<String> {
        db.query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name FROM sqlite_master WHERE type = 'index' ORDER BY name".to_owned(),
        ))
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.try_get_by_index(0).unwrap())
        .collect()
    }

    async fn pragma_i64(db: &DatabaseConnection, name: &str) -> i64 {
        pragma_row(db, name).await.try_get_by_index(0).unwrap()
    }

    async fn pragma_string(db: &DatabaseConnection, name: &str) -> String {
        pragma_row(db, name).await.try_get_by_index(0).unwrap()
    }

    async fn pragma_row(db: &DatabaseConnection, name: &str) -> QueryResult {
        db.query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            format!("PRAGMA {name}"),
        ))
        .await
        .unwrap()
        .unwrap()
    }

    fn sqlite_url(path: &Path) -> String {
        format!("sqlite://{}?mode=rwc", path.display())
    }

    fn memory_sqlite_url(label: &str) -> String {
        format!(
            "sqlite:file:hamlet_database_test_{label}_{}?mode=memory&cache=shared",
            generate_id()
        )
    }

    fn tmp_database_dir() -> PathBuf {
        std::env::temp_dir().join(format!("hamlet-database-test-{}", generate_id()))
    }
}
