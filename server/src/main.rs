use hamlet::{broadcast, seed_development_data, start_server};
use sea_orm::{Database, DatabaseConnection};

#[actix_web::main]
#[allow(clippy::unwrap_used)]
async fn main() -> std::io::Result<()> {
    // TODO(reno): Using in-memory for now, will swap to real DB later
    let db: DatabaseConnection = Database::connect("sqlite::memory:").await.unwrap();
    let broadcaster = broadcast::Broadcaster::create();

    // TODO(reno): Probably gate this on debug builds. Does schema syncing, and some test data
    // inserting. Production should just trust that the database is set up correctly.
    seed_development_data(&db).await;

    start_server(db, broadcaster).await
}
