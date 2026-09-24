use sea_orm_migration::prelude::*;

pub struct Migrator;
#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(Initial), Box::new(Channels)]
    }
}

// Preserve the already-shipped migration version name ("lib") when moving
// schema ownership out of the HTTP crate. Existing test databases can upgrade.
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
