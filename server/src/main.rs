use hamlet::{Config, broadcast, seed_development_data, start_server, telemetry};
use sea_orm::{Database, DatabaseConnection};

#[actix_web::main]
#[allow(clippy::unwrap_used)]
async fn main() -> std::io::Result<()> {
    let config = Config::from_env();
    telemetry::init(&config.log_filter);

    // `file::memory:?cache=shared` so every connection in the sqlx pool sees
    // the same DB — plain `sqlite::memory:` gives each connection its own
    // empty database, so seeded tables would only exist on whichever
    // connection happened to run seeding.
    let db: DatabaseConnection = Database::connect(&config.database_url).await.unwrap();
    let broadcaster = broadcast::Broadcaster::create();

    if config.seed_dev_data {
        // TODO(reno): gate this on debug builds once the DB is persistent.
        // Production should just trust that the database is set up correctly.
        seed_development_data(&db, &config.uploads_dir).await;
    }

    start_server(config, db, broadcaster).await
}
