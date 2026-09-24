use actix_web::{App, HttpServer, web};
use hamlet_rewrite::{connect, routes};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let url = std::env::var("HAMLET_REWRITE_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://hamlet-rewrite.db?mode=rwc".into());
    let bind = std::env::var("HAMLET_REWRITE_BIND").unwrap_or_else(|_| "127.0.0.1:8081".into());
    let state = connect(&url).await.map_err(std::io::Error::other)?;
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(routes)
    })
    .bind(bind)?
    .run()
    .await
}
