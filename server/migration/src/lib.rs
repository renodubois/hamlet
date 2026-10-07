//! Ordered SQLite schema migrations for users, sessions, channels and message history.

use sea_orm_migration::prelude::*;

pub struct Migrator;
#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(Initial),
            Box::new(Channels),
            Box::new(Messages),
            Box::new(ChannelDeletion),
        ]
    }
}

#[derive(DeriveMigrationName)]
struct Initial;
#[async_trait::async_trait]
impl MigrationTrait for Initial {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in [
            "CREATE TABLE users (id INTEGER PRIMARY KEY, username TEXT NOT NULL, username_key TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL)",
            "CREATE TABLE sessions (token_digest TEXT PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id), expires_at TEXT NOT NULL)",
        ] {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE sessions; DROP TABLE users")
            .await?;
        Ok(())
    }
}

struct Channels;
impl MigrationName for Channels {
    fn name(&self) -> &str {
        "m20260923_000002_channels"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Channels {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "CREATE TABLE channels (id INTEGER PRIMARY KEY, name TEXT NOT NULL, name_key TEXT NOT NULL UNIQUE, type TEXT NOT NULL CHECK (type = 'text'))"
        ).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE channels")
            .await?;
        Ok(())
    }
}

// Rebuild both related tables with foreign keys enabled. Copy before dropping
// either original table; SQLite retargets the new message FK on channel rename.
struct ChannelDeletion;
impl MigrationName for ChannelDeletion {
    fn name(&self) -> &str {
        "m20261007_000004_channel_deletion"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ChannelDeletion {
    fn use_transaction(&self) -> Option<bool> {
        Some(true)
    }
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in [
            "CREATE TABLE channels_new (id INTEGER PRIMARY KEY, name TEXT NOT NULL, name_key TEXT NOT NULL, type TEXT NOT NULL CHECK (type = 'text'), deleted_at TEXT)",
            "INSERT INTO channels_new (id, name, name_key, type) SELECT id, name, name_key, type FROM channels",
            "CREATE TABLE messages_new (id INTEGER PRIMARY KEY, channel_id INTEGER NOT NULL REFERENCES channels_new(id), author_id INTEGER NOT NULL REFERENCES users(id), text TEXT NOT NULL, created_at TEXT NOT NULL)",
            "INSERT INTO messages_new SELECT id, channel_id, author_id, text, created_at FROM messages",
            "DROP TABLE messages",
            "DROP TABLE channels",
            "ALTER TABLE channels_new RENAME TO channels",
            "ALTER TABLE messages_new RENAME TO messages",
            "CREATE UNIQUE INDEX channels_active_name ON channels(name_key) WHERE deleted_at IS NULL",
            "CREATE INDEX messages_history ON messages (channel_id, created_at DESC, id DESC)",
        ] {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Active-name reuse makes a lossless downgrade to all-row uniqueness
        // impossible in general. Never silently discard retained conversations.
        Err(DbErr::Custom(
            "channel deletion migration cannot be downgraded losslessly".into(),
        ))
    }
}

struct Messages;
impl MigrationName for Messages {
    fn name(&self) -> &str {
        "m20260923_000003_messages"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Messages {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in [
            "CREATE TABLE messages (id INTEGER PRIMARY KEY, channel_id INTEGER NOT NULL REFERENCES channels(id), author_id INTEGER NOT NULL REFERENCES users(id), text TEXT NOT NULL, created_at TEXT NOT NULL)",
            "CREATE INDEX messages_history ON messages (channel_id, created_at DESC, id DESC)",
        ] {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE messages")
            .await?;
        Ok(())
    }
}
