use hamlet::{Config, broadcast, connect_database, seed_development_data, start_server, telemetry};
use sea_orm::DatabaseConnection;

#[actix_web::main]
#[allow(clippy::unwrap_used)]
async fn main() -> std::io::Result<()> {
    let config = Config::from_env();
    telemetry::init(&config.log_filter);

    // `connect_database` keeps a sentinel connection alive for SQLite
    // in-memory URLs. Without that, SQLx can reap every idle connection and
    // the next request would see a brand-new empty database.
    let db: DatabaseConnection = connect_database(&config).await.unwrap();
    let broadcaster = broadcast::Broadcaster::create();

    if config.seed_dev_data {
        // TODO(reno): gate this on debug builds once the DB is persistent.
        // Production should just trust that the database is set up correctly.
        seed_development_data(&db, &config.uploads_dir).await;
    }

    start_server(config, db, broadcaster).await
}
