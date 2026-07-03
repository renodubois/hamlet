use hamlet::{
    Config, bootstrap_default_channels, broadcast, connect_database, seed_development_data,
    start_server, telemetry,
};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = Config::from_env();
    let _telemetry_guard = telemetry::init(&config.log_filter, config.sentry_dsn.as_deref());

    // `connect_database` prepares SQLite connection settings, keeps a sentinel
    // connection alive for in-memory URLs, and applies the baseline schema.
    tracing::info!(database_url = %config.database_url, "connecting and initializing database");
    let db = connect_database(&config).await.map_err(|error| {
        tracing::error!(%error, "database setup failed");
        std::io::Error::from(error)
    })?;

    if config.bootstrap_default_channels {
        let outcome = bootstrap_default_channels(&db).await.map_err(|error| {
            tracing::error!(%error, "default channel bootstrap failed");
            std::io::Error::other(error.to_string())
        })?;
        tracing::info!(?outcome, "default channel bootstrap complete");
    } else {
        tracing::info!("default channel bootstrap disabled");
    }

    if config.seed_dev_data {
        tracing::info!("seeding development fixtures");
        seed_development_data(&db, &config.uploads_dir)
            .await
            .map_err(|error| {
                tracing::error!(%error, "development seed failed");
                std::io::Error::other(error.to_string())
            })?;
    } else {
        tracing::info!("development fixture seed disabled");
    }

    let broadcaster = broadcast::Broadcaster::create();
    start_server(config, db, broadcaster).await
}
