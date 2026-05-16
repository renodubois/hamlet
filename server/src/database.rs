//! Database connection helpers.
//!
//! The default development database is SQLite in-memory with a shared cache.
//! SQLite keeps that database alive only while at least one connection to the
//! named in-memory database remains open, so the SQLx pool must keep a
//! sentinel connection around for the lifetime of the process.

use std::time::Duration;

use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

use crate::Config;

pub async fn connect_database(config: &Config) -> Result<DatabaseConnection, DbErr> {
    Database::connect(connection_options(&config.database_url)).await
}

fn connection_options(database_url: &str) -> ConnectOptions {
    let mut options = ConnectOptions::new(database_url.to_owned());

    if is_in_memory_sqlite(database_url) {
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
    use super::*;

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
}
