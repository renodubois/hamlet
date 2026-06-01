//! App wiring: the `AppDeps` bag and the `configure_app` / `start_server`
//! pair that build an Actix `App` from it.

use std::sync::Arc;

use actix_cors::Cors;
use actix_web::{App, HttpServer, middleware::from_fn, web, web::Data};
use sea_orm::DatabaseConnection;
use tracing_actix_web::TracingLogger;

use crate::api;
use crate::api::avatars::AvatarStorage;
use crate::api::emoji::EmojiStorage;
use crate::api::messages::EmbedFetcher;
use crate::broadcast::Broadcaster;
use crate::config::Config;
use crate::middleware;
use crate::voice::{VoiceConfig, VoiceState};

/// Bag of `web::Data` that every sub-router needs. Constructed once in
/// `start_server` (or in tests) and cloned into the closure that builds
/// each `App` instance.
#[derive(Clone)]
pub struct AppDeps {
    pub db: Data<DatabaseConnection>,
    pub broadcaster: Data<Broadcaster>,
    pub voice_cfg: Data<Option<VoiceConfig>>,
    pub voice_state: Data<VoiceState>,
    pub embed_fetcher: Data<EmbedFetcher>,
    pub emoji_storage: Data<EmojiStorage>,
}

/// Default-flavoured deps for tests/hosts that don't need voice. Voice
/// config is `None` (so `/voice/*` returns 503), voice state is empty,
/// and embed fetching is disabled.
pub fn deps_for_tests(db: DatabaseConnection, broadcaster: Arc<Broadcaster>) -> AppDeps {
    AppDeps {
        db: Data::new(db),
        broadcaster: Data::from(broadcaster),
        voice_cfg: Data::new(None::<VoiceConfig>),
        voice_state: Data::new(VoiceState::new()),
        embed_fetcher: Data::new(EmbedFetcher::Disabled),
        emoji_storage: Data::new(EmojiStorage {
            dir: std::env::temp_dir(),
        }),
    }
}

/// Register every route on `cfg`. Public routes (register/login/logout +
/// LiveKit webhook) live outside the auth scope; everything else is wrapped
/// in `require_auth`.
pub fn configure_app(cfg: &mut web::ServiceConfig, deps: AppDeps) {
    cfg.app_data(deps.db.clone())
        .app_data(deps.broadcaster.clone())
        .app_data(deps.voice_cfg.clone())
        .app_data(deps.voice_state.clone())
        .app_data(deps.embed_fetcher.clone())
        .app_data(deps.emoji_storage.clone())
        // Public auth surface
        .configure(api::auth::configure_public)
        // LiveKit webhooks authenticate via signed JWT in the body, not a
        // session cookie, so they live outside the require_auth scope.
        .configure(api::voice::configure_public_webhook)
        // Everything else requires a session cookie
        .service(
            web::scope("")
                .wrap(from_fn(middleware::require_auth))
                .configure(api::messages::configure)
                .configure(api::channels::configure)
                .configure(api::voice::configure_authed)
                .configure(api::auth::configure_authed)
                .configure(api::avatars::configure)
                .configure(api::emoji::configure),
        );
}

/// Bind, configure, and run the HTTP server. Invoked by `main` after
/// loading `Config`, the DB, and the broadcaster.
pub async fn start_server(
    config: Config,
    db: DatabaseConnection,
    broadcaster: Arc<Broadcaster>,
) -> std::io::Result<()> {
    let avatars_dir = config.uploads_dir.join(crate::api::avatars::AVATARS_SUBDIR);
    let emojis_dir = config.uploads_dir.join(crate::api::emoji::EMOJIS_SUBDIR);
    std::fs::create_dir_all(&avatars_dir)?;
    std::fs::create_dir_all(&emojis_dir)?;
    let avatar_storage = Data::new(AvatarStorage {
        dir: config.uploads_dir.clone(),
    });
    let emoji_storage = Data::new(EmojiStorage {
        dir: config.uploads_dir.clone(),
    });

    if config.voice.is_none() {
        tracing::warn!(
            "LIVEKIT_URL / LIVEKIT_API_KEY / LIVEKIT_API_SECRET not set — voice endpoints will return 503"
        );
    }

    let deps = AppDeps {
        db: Data::new(db),
        broadcaster: Data::from(broadcaster),
        voice_cfg: Data::new(config.voice.clone()),
        voice_state: Data::new(VoiceState::new()),
        embed_fetcher: Data::new(if config.embed_fetcher_enabled {
            EmbedFetcher::Enabled
        } else {
            EmbedFetcher::Disabled
        }),
        emoji_storage: emoji_storage.clone(),
    };

    let bind_addr = config.bind_addr.clone();
    let uploads_dir = config.uploads_dir.clone();
    tracing::info!(addr = %bind_addr, "starting server");

    HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin_fn(|origin, _| {
                // TODO(reno): this is my lazy way to get CORS working for local envs.
                // Worth tightening to an explicit allowlist before any production deploy.
                origin.as_bytes().starts_with(b"http://localhost")
                    || origin.as_bytes().starts_with(b"http://127.0.0.1")
            })
            // NOTE(reno): These are dangerous - probably worth reconsidering if keeping CORS
            // in production mode.
            .allow_any_method()
            .allow_any_header()
            .supports_credentials();

        App::new()
            .wrap(TracingLogger::default())
            .wrap(cors)
            .app_data(avatar_storage.clone())
            .app_data(emoji_storage.clone())
            .service(actix_files::Files::new("/uploads", uploads_dir.clone()))
            .configure(|cfg| configure_app(cfg, deps.clone()))
    })
    .bind(&bind_addr)?
    .run()
    .await
}
