use actix_web::{App, HttpServer, web};
use hamlet::{connect_to_database, routes};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let url = match std::env::var("HAMLET_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            std::fs::create_dir_all("data")?;
            "sqlite://data/hamlet.db?mode=rwc".into()
        }
    };
    let bind = std::env::var("HAMLET_BIND").unwrap_or_else(|_| "127.0.0.1:3001".into());
    let state = connect_to_database(&url)
        .await
        .map_err(std::io::Error::other)?;
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(routes)
    })
    .bind(bind)?
    .run()
    .await
}
