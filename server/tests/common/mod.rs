#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::Arc;

use actix_web::web;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};

use hamlet::voice::{VoiceConfig, VoiceState};
use hamlet::{
    AppDeps, AvatarStorage, EmbedFetcher, EmojiStorage, auth, broadcast::Broadcaster,
    connect_initialized_database_url, entity, generate_id, now_unix_micros,
};

/// Bag of state every integration test needs: the DB, a seeded text channel,
/// a broadcaster, and (optionally) an avatar uploads dir. Build app instances
/// off of `ctx.deps()` so each test stays one or two lines of boilerplate.
pub struct TestCtx {
    pub db: DatabaseConnection,
    pub channel_id: i64,
    pub broadcaster: Arc<Broadcaster>,
    pub voice_cfg: Option<VoiceConfig>,
    pub voice_state: Arc<VoiceState>,
    pub uploads_dir: Option<PathBuf>,
}

impl TestCtx {
    /// Standard test fixture: in-memory DB + one seeded text channel +
    /// a quiet broadcaster (no ping loop). Embed fetcher is `Disabled`.
    pub async fn new() -> Self {
        let (db, channel_id) = setup_db().await;
        Self {
            db,
            channel_id,
            broadcaster: Broadcaster::new(),
            voice_cfg: None,
            voice_state: Arc::new(VoiceState::new()),
            uploads_dir: None,
        }
    }

    /// Variant that also creates a unique uploads dir on disk so avatar
    /// upload/delete tests don't trip over each other. Caller is responsible
    /// for removing the directory at the end of the test.
    pub async fn with_avatar_storage() -> Self {
        let mut ctx = Self::new().await;
        ctx.uploads_dir = Some(make_tmp_uploads_dir());
        ctx
    }

    pub fn with_voice_cfg(mut self, cfg: VoiceConfig) -> Self {
        self.voice_cfg = Some(cfg);
        self
    }

    pub fn with_voice_state(mut self, state: VoiceState) -> Self {
        self.voice_state = Arc::new(state);
        self
    }

    /// Construct the deps bag needed by `configure_app`.
    pub fn deps(&self) -> AppDeps {
        AppDeps {
            db: web::Data::new(self.db.clone()),
            broadcaster: web::Data::from(self.broadcaster.clone()),
            voice_cfg: web::Data::new(self.voice_cfg.clone()),
            voice_state: web::Data::from(self.voice_state.clone()),
            embed_fetcher: web::Data::new(EmbedFetcher::Disabled),
            emoji_storage: web::Data::new(EmojiStorage {
                dir: self.uploads_dir.clone().unwrap_or_else(std::env::temp_dir),
            }),
        }
    }

    /// `web::Data` for `AvatarStorage`. Panics if `with_avatar_storage`
    /// wasn't called.
    pub fn avatar_storage(&self) -> web::Data<AvatarStorage> {
        let dir = self
            .uploads_dir
            .clone()
            .expect("call TestCtx::with_avatar_storage to enable avatar tests");
        web::Data::new(AvatarStorage { dir })
    }

    /// Register a fresh user + session. Returns a cookie helper.
    pub async fn register(&self, username: &str, password: &str) -> AuthSession {
        let user = auth::register_user(&self.db, username, password, None)
            .await
            .unwrap();
        let session = auth::create_session(&self.db, user.id).await.unwrap();
        AuthSession {
            user_id: user.id,
            username: username.to_owned(),
            token: session.token,
        }
    }
}

#[derive(Clone)]
pub struct AuthSession {
    pub user_id: i64,
    pub username: String,
    pub token: String,
}

impl AuthSession {
    /// Tuple suitable for `TestRequest::insert_header` so callers don't
    /// have to know the cookie wire format.
    pub fn cookie_header(&self) -> (String, String) {
        (
            "Cookie".to_owned(),
            format!("{}={}", auth::SESSION_COOKIE, self.token),
        )
    }
}

pub async fn setup_db() -> (DatabaseConnection, i64) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let url = format!("sqlite:file:hamlet_api_test_{n}?mode=memory&cache=shared");
    let db = connect_initialized_database_url(&url).await.unwrap();

    let chan_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(chan_id),
        name: Set("general".to_owned()),
        position: Set(0),
        channel_type: Set("text".to_owned()),
    }
    .insert(&db)
    .await
    .unwrap();

    (db, chan_id)
}

pub fn make_tmp_uploads_dir() -> PathBuf {
    let id = generate_id();
    let dir = std::env::temp_dir().join(format!("hamlet-test-uploads-{id}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Insert a message directly into the DB without going through the HTTP
/// layer. Useful for tests that need a target row to mutate.
pub async fn insert_message(
    db: &DatabaseConnection,
    user_id: i64,
    channel_id: i64,
    text: &str,
) -> i64 {
    let id = generate_id();
    entity::message::ActiveModel {
        id: Set(id),
        user_id: Set(user_id),
        channel_id: Set(channel_id),
        parent_id: Set(None),
        reply_to_message_id: Set(None),
        created_at: Set(now_unix_micros()),
        deleted_at: Set(None),
        text: Set(text.to_owned()),
        suppress_embeds: Set(false),
    }
    .insert(db)
    .await
    .unwrap();
    id
}
