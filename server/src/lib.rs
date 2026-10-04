use actix_web::{http::StatusCode, middleware::from_fn, web};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use http::{
    auth::bearer,
    error::{bad_request, problem},
    request_id::request_id,
};
use rand::RngCore;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use sha2::{Digest, Sha256};
use std::time::Duration as StdDuration;

mod auth;
mod bootstrap;
mod channels;
pub mod contract;
mod http;
pub mod live_updates;
mod messages;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub events: live_updates::EventHub,
}

pub async fn connect_to_database(url: &str) -> Result<AppState, String> {
    if !url.starts_with("sqlite:") {
        return Err("server requires a SQLite URL".into());
    }
    let mut opts = ConnectOptions::new(url);
    let file_backed = !url.contains(":memory:");
    opts.max_connections(if file_backed { 5 } else { 1 })
        .min_connections(1)
        .connect_timeout(StdDuration::from_secs(5))
        // TODO(reno): Maybe have this be toggled via env var?
        // I believe this is letting us see every query ran, which can be noisy but useful
        .sqlx_logging(false)
        .map_sqlx_sqlite_opts(move |options| {
            let options = options
                .foreign_keys(true)
                .busy_timeout(StdDuration::from_secs(5));
            if file_backed {
                options.journal_mode(sea_orm::sqlx::sqlite::SqliteJournalMode::Wal)
            } else {
                options
            }
        });
    let db = Database::connect(opts)
        .await
        .map_err(|e| format!("database connection failed: {e}"))?;

    // Run SeaORM migrations
    use sea_orm_migration::MigratorTrait;
    hamlet_migration::Migrator::up(&db, None)
        .await
        .map_err(|e| format!("migration failed: {e}"))?;

    bootstrap::bootstrap(&db)
        .await
        .map_err(|e| format!("channel bootstrap failed: {e}"))?;

    // Construct once before the worker factory; AppState clones share this hub.
    Ok(AppState {
        db,
        events: live_updates::EventHub::default(),
    })
}

fn digest(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
fn new_id() -> i64 {
    100_000_000_000_000 + (rand::random::<u64>() % 900_000_000_000_000) as i64
}
fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
pub fn routes(cfg: &mut web::ServiceConfig) {
    cfg.app_data(web::JsonConfig::default().error_handler(|_, _| {
        actix_web::error::InternalError::from_response("json", bad_request()).into()
    }));
    cfg.service(
        web::scope("")
            .wrap(from_fn(request_id))
            .service(
                web::scope("/api/v1")
                    .service(
                        web::resource("/auth/signup")
                            .route(web::post().to(auth::handlers::signup))
                            .default_service(web::to(|| async {
                                problem(
                                    StatusCode::METHOD_NOT_ALLOWED,
                                    "method_not_allowed",
                                    "Method not allowed",
                                )
                            })),
                    )
                    .service(
                        web::resource("/auth/login")
                            .route(web::post().to(auth::handlers::login))
                            .default_service(web::to(|| async {
                                problem(
                                    StatusCode::METHOD_NOT_ALLOWED,
                                    "method_not_allowed",
                                    "Method not allowed",
                                )
                            })),
                    )
                    .service(
                        web::scope("")
                            .wrap(from_fn(bearer))
                            .service(
                                web::resource("/auth/logout")
                                    .route(web::post().to(auth::handlers::logout))
                                    .default_service(web::to(|| async {
                                        problem(
                                            StatusCode::METHOD_NOT_ALLOWED,
                                            "method_not_allowed",
                                            "Method not allowed",
                                        )
                                    })),
                            )
                            .service(
                                web::resource("/me")
                                    .route(web::get().to(auth::handlers::me))
                                    .default_service(web::to(|| async {
                                        problem(
                                            StatusCode::METHOD_NOT_ALLOWED,
                                            "method_not_allowed",
                                            "Method not allowed",
                                        )
                                    })),
                            )
                            .configure(channels::routes)
                            .configure(messages::routes)
                            .configure(live_updates::routes),
                    )
                    .default_service(web::to(|| async {
                        problem(StatusCode::NOT_FOUND, "not_found", "Not found")
                    })),
            )
            .default_service(web::to(|| async {
                problem(StatusCode::NOT_FOUND, "not_found", "Not found")
            })),
    );
}
