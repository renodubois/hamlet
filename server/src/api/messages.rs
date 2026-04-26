//! Message HTTP handlers + the SSE subscribe + typing notifications +
//! embed-fetch orchestration.

use std::collections::HashMap;

use actix_web::{HttpResponse, Responder, delete, get, post, put, web};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};

use crate::api::avatars::avatar_url;
use crate::auth::AuthUser;
use crate::broadcast::{
    BroadcastEvent, Broadcaster, MessageDeletedEvent, MessageEmbedsUpdatedEvent, UserTypingEvent,
};
use crate::embeds;
use crate::entity;
use crate::error::AppError;
use crate::util::generate_id;

/// Cap on how many URLs per message we actually fetch. If a message is a wall
/// of 200 links we still broadcast it instantly — we just don't try to turn
/// them all into embed cards.
const MAX_EMBEDS_PER_MESSAGE: usize = 5;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SendMessageRequest {
    pub text: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SuppressEmbedsRequest {
    pub suppress: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct MessageResponse {
    pub id: i64,
    pub user_id: i64,
    pub channel_id: i64,
    pub text: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub suppress_embeds: bool,
    pub embeds: Vec<EmbedResponse>,
}

#[derive(Clone, Debug, Serialize)]
pub struct EmbedResponse {
    pub id: i64,
    pub message_id: i64,
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub site_name: Option<String>,
    pub embed_type: String,
    pub iframe_url: Option<String>,
    pub iframe_width: Option<i32>,
    pub iframe_height: Option<i32>,
}

impl From<entity::embed::Model> for EmbedResponse {
    fn from(e: entity::embed::Model) -> Self {
        Self {
            id: e.id,
            message_id: e.message_id,
            url: e.url,
            title: e.title,
            description: e.description,
            image_url: e.image_url,
            site_name: e.site_name,
            embed_type: e.embed_type,
            iframe_url: e.iframe_url,
            iframe_width: e.iframe_width,
            iframe_height: e.iframe_height,
        }
    }
}

/// Runtime switch controlling whether message creation kicks off an outbound
/// OpenGraph fetch. Tests use `Disabled` to keep the suite hermetic;
/// `start_server` uses `Enabled`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmbedFetcher {
    Enabled,
    #[default]
    Disabled,
}

#[get("/messages/{channel_id}")]
async fn get_messages(
    db: web::Data<DatabaseConnection>,
    path: web::Path<i64>,
) -> Result<impl Responder, AppError> {
    let channel_id = path.into_inner();

    let channel = entity::channel::Entity::find_by_id(channel_id)
        .one(db.get_ref())
        .await?;
    if channel.is_none() {
        return Err(AppError::NoChannelFound);
    }

    let rows = entity::message::Entity::find()
        .filter(entity::message::Column::ChannelId.eq(channel_id))
        .find_also_related(entity::user::Entity)
        .all(db.get_ref())
        .await?;

    let message_ids: Vec<i64> = rows.iter().map(|(m, _)| m.id).collect();
    let embeds_by_message = load_embeds_for_messages(db.get_ref(), &message_ids).await?;

    let messages: Vec<MessageResponse> = rows
        .into_iter()
        .map(|(m, u)| {
            let (username, display_name, avatar_url) = match u {
                Some(u) => (
                    u.username,
                    u.display_name,
                    avatar_url(u.avatar_path.as_deref(), u.avatar_updated_at),
                ),
                None => ("[deleted]".into(), None, None),
            };
            let embeds = embeds_by_message.get(&m.id).cloned().unwrap_or_default();
            MessageResponse {
                id: m.id,
                user_id: m.user_id,
                channel_id: m.channel_id,
                text: m.text,
                username,
                display_name,
                avatar_url,
                suppress_embeds: m.suppress_embeds,
                embeds,
            }
        })
        .collect();

    Ok(web::Json(messages))
}

/// Load all embeds for a batch of message ids, grouped by message id.
async fn load_embeds_for_messages(
    db: &DatabaseConnection,
    ids: &[i64],
) -> Result<HashMap<i64, Vec<EmbedResponse>>, AppError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = entity::embed::Entity::find()
        .filter(entity::embed::Column::MessageId.is_in(ids.iter().copied()))
        .order_by_asc(entity::embed::Column::Id)
        .all(db)
        .await?;
    let mut out: HashMap<i64, Vec<EmbedResponse>> = HashMap::new();
    for row in rows {
        out.entry(row.message_id)
            .or_default()
            .push(EmbedResponse::from(row));
    }
    Ok(out)
}

#[post("/message/{channel_id}")]
async fn create_message(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    embed_fetcher: web::Data<EmbedFetcher>,
    path: web::Path<i64>,
    body: web::Json<SendMessageRequest>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let channel_id = path.into_inner();

    let channel = entity::channel::Entity::find_by_id(channel_id)
        .one(db.get_ref())
        .await?;
    if channel.is_none() {
        return Err(AppError::NoChannelFound);
    }

    let new_message = entity::message::ActiveModel {
        id: Set(generate_id()),
        user_id: Set(user.id),
        channel_id: Set(channel_id),
        text: Set(body.text.clone()),
        suppress_embeds: Set(false),
    };
    let inserted = new_message.insert(db.get_ref()).await?;

    let resp = MessageResponse {
        id: inserted.id,
        user_id: inserted.user_id,
        channel_id: inserted.channel_id,
        text: inserted.text.clone(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        avatar_url: avatar_url(user.avatar_path.as_deref(), user.avatar_updated_at),
        suppress_embeds: inserted.suppress_embeds,
        embeds: Vec::new(),
    };
    broadcaster
        .publish(&BroadcastEvent::Message(resp.clone()))
        .await?;

    // Embed fetching runs in the background so the POST returns immediately.
    spawn_embed_refresh(
        embed_fetcher.clone(),
        db.clone(),
        broadcaster.clone(),
        inserted.id,
        inserted.channel_id,
        inserted.text,
    );

    Ok(web::Json(resp))
}

#[put("/message/{message_id}")]
async fn update_message(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    embed_fetcher: web::Data<EmbedFetcher>,
    path: web::Path<i64>,
    body: web::Json<SendMessageRequest>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let message_id = path.into_inner();

    let existing = entity::message::Entity::find_by_id(message_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound)?;

    if existing.user_id != user.id {
        return Err(AppError::Forbidden);
    }

    let channel_id = existing.channel_id;
    let previous_text = existing.text.clone();
    let mut active: entity::message::ActiveModel = existing.into();
    active.text = Set(body.text.clone());
    let updated = active.update(db.get_ref()).await?;

    let existing_embeds = load_embeds_for_messages(db.get_ref(), &[updated.id])
        .await?
        .remove(&updated.id)
        .unwrap_or_default();

    let resp = MessageResponse {
        id: updated.id,
        user_id: updated.user_id,
        channel_id,
        text: updated.text.clone(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        avatar_url: avatar_url(user.avatar_path.as_deref(), user.avatar_updated_at),
        suppress_embeds: updated.suppress_embeds,
        embeds: existing_embeds,
    };
    broadcaster
        .publish(&BroadcastEvent::MessageUpdated(resp.clone()))
        .await?;

    // Only re-fetch embeds if the URL set actually changed.
    if embeds::extract_urls(&previous_text) != embeds::extract_urls(&updated.text) {
        spawn_embed_refresh(
            embed_fetcher.clone(),
            db.clone(),
            broadcaster.clone(),
            updated.id,
            channel_id,
            updated.text,
        );
    }

    Ok(web::Json(resp))
}

#[post("/message/{message_id}/suppress_embeds")]
async fn suppress_message_embeds(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
    body: web::Json<SuppressEmbedsRequest>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let message_id = path.into_inner();

    let existing = entity::message::Entity::find_by_id(message_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound)?;

    // Only the author can hide their own message's embeds — matches the rest
    // of the message mutation endpoints.
    if existing.user_id != user.id {
        return Err(AppError::Forbidden);
    }

    let channel_id = existing.channel_id;
    let mut active: entity::message::ActiveModel = existing.into();
    active.suppress_embeds = Set(body.suppress);
    let updated = active.update(db.get_ref()).await?;

    let embeds = load_embeds_for_messages(db.get_ref(), &[message_id])
        .await?
        .remove(&message_id)
        .unwrap_or_default();

    broadcaster
        .publish(&BroadcastEvent::MessageEmbedsUpdated(
            MessageEmbedsUpdatedEvent {
                id: message_id,
                channel_id,
                suppress_embeds: updated.suppress_embeds,
                embeds: embeds.clone(),
            },
        ))
        .await?;

    Ok(web::Json(MessageEmbedsUpdatedEvent {
        id: message_id,
        channel_id,
        suppress_embeds: updated.suppress_embeds,
        embeds,
    }))
}

#[delete("/message/{message_id}")]
async fn delete_message(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let message_id = path.into_inner();

    let existing = entity::message::Entity::find_by_id(message_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound)?;

    if existing.user_id != user.id {
        return Err(AppError::Forbidden);
    }

    let channel_id = existing.channel_id;
    let active: entity::message::ActiveModel = existing.into();
    active.delete(db.get_ref()).await?;

    broadcaster
        .publish(&BroadcastEvent::MessageDeleted(MessageDeletedEvent {
            id: message_id,
            channel_id,
        }))
        .await?;

    Ok(HttpResponse::NoContent().finish())
}

#[get("/messages/subscribe")]
async fn subscribe(broadcaster: web::Data<Broadcaster>) -> impl Responder {
    broadcaster.subscribe().await
}

#[post("/typing/{channel_id}")]
async fn post_typing(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let channel_id = path.into_inner();

    let channel = entity::channel::Entity::find_by_id(channel_id)
        .one(db.get_ref())
        .await?;
    if channel.is_none() {
        return Err(AppError::NoChannelFound);
    }

    broadcaster
        .publish(&BroadcastEvent::UserTyping(UserTypingEvent {
            channel_id,
            user_id: user.id,
            username: user.username.clone(),
        }))
        .await?;

    Ok(HttpResponse::NoContent().finish())
}

/// Spawn the embed-refresh task for a single message. Returns immediately —
/// the task handles its own errors by logging them. When disabled, this is a
/// no-op so no network traffic leaks out during tests.
fn spawn_embed_refresh(
    fetcher: web::Data<EmbedFetcher>,
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    message_id: i64,
    channel_id: i64,
    text: String,
) {
    if matches!(**fetcher, EmbedFetcher::Disabled) {
        return;
    }
    actix_web::rt::spawn(async move {
        let urls = embeds::extract_urls(&text);
        let mut fetched: Vec<embeds::FetchedEmbed> = Vec::new();
        for url in urls.into_iter().take(MAX_EMBEDS_PER_MESSAGE) {
            match embeds::fetch_embed(&url).await {
                Ok(e) => fetched.push(e),
                Err(err) => {
                    tracing::warn!(url, ?err, "embed fetch failed");
                }
            }
        }
        if let Err(err) =
            apply_fetched_embeds(db.get_ref(), &broadcaster, message_id, channel_id, fetched).await
        {
            tracing::warn!(message_id, ?err, "embed apply failed");
        }
    });
}

/// Replace the embed rows for `message_id` with `fetched` and broadcast a
/// MessageEmbedsUpdated event. Silently skips if the message was deleted
/// between the fetch finishing and this write landing.
async fn apply_fetched_embeds(
    db: &DatabaseConnection,
    broadcaster: &Broadcaster,
    message_id: i64,
    channel_id: i64,
    fetched: Vec<embeds::FetchedEmbed>,
) -> Result<(), AppError> {
    entity::embed::Entity::delete_many()
        .filter(entity::embed::Column::MessageId.eq(message_id))
        .exec(db)
        .await?;

    let Some(msg) = entity::message::Entity::find_by_id(message_id)
        .one(db)
        .await?
    else {
        return Ok(());
    };

    let mut inserted: Vec<EmbedResponse> = Vec::new();
    for f in fetched {
        let model = entity::embed::ActiveModel {
            id: Set(generate_id()),
            message_id: Set(message_id),
            url: Set(f.url),
            title: Set(f.title),
            description: Set(f.description),
            image_url: Set(f.image_url),
            site_name: Set(f.site_name),
            embed_type: Set(f.embed_type.as_str().to_owned()),
            iframe_url: Set(f.iframe_url),
            iframe_width: Set(f.iframe_width),
            iframe_height: Set(f.iframe_height),
        };
        let saved = model.insert(db).await?;
        inserted.push(EmbedResponse::from(saved));
    }

    broadcaster
        .publish(&BroadcastEvent::MessageEmbedsUpdated(
            MessageEmbedsUpdatedEvent {
                id: message_id,
                channel_id,
                suppress_embeds: msg.suppress_embeds,
                embeds: inserted,
            },
        ))
        .await?;
    Ok(())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    // `subscribe` must be registered before `get_messages` so actix-web's
    // router doesn't match `/messages/subscribe` as a channel ID.
    cfg.service(subscribe)
        .service(get_messages)
        .service(create_message)
        .service(update_message)
        .service(delete_message)
        .service(suppress_message_embeds)
        .service(post_typing);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::time::Duration;

    use actix_web::http::header::ContentType;
    use actix_web::{App, http::StatusCode, test};
    use sea_orm::Database;

    use super::*;
    use crate::auth;
    use crate::startup::{AppDeps, configure_app};
    use crate::voice::{VoiceConfig, VoiceState};

    async fn setup_db() -> (DatabaseConnection, i64) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let url = format!("sqlite:file:hamlet_messages_test_{n}?mode=memory&cache=shared");
        let db = Database::connect(&url).await.unwrap();
        db.get_schema_registry("hamlet::entity::*")
            .sync(&db)
            .await
            .unwrap();

        let chan_id = generate_id();
        entity::channel::ActiveModel {
            id: Set(chan_id),
            name: Set("general".to_owned()),
            position: Set(0),
            channel_type: Set(crate::api::channels::CHANNEL_TYPE_TEXT.to_owned()),
        }
        .insert(&db)
        .await
        .unwrap();

        (db, chan_id)
    }

    fn session_cookie_header(token: &str) -> (String, String) {
        (
            "Cookie".to_owned(),
            format!("{}={}", auth::SESSION_COOKIE, token),
        )
    }

    fn deps(db: DatabaseConnection, broadcaster: std::sync::Arc<Broadcaster>) -> AppDeps {
        AppDeps {
            db: web::Data::new(db),
            broadcaster: web::Data::from(broadcaster),
            voice_cfg: web::Data::new(None::<VoiceConfig>),
            voice_state: web::Data::new(VoiceState::new()),
            embed_fetcher: web::Data::new(EmbedFetcher::Disabled),
        }
    }

    #[actix_web::test]
    async fn test_message_create_broadcasts_to_clients() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let app_deps = deps(db, broadcaster);
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri(&format!("/message/{}", chan_id))
            .insert_header(ContentType::json())
            .insert_header((name, value))
            .set_payload(
                serde_json::to_string(&SendMessageRequest {
                    text: "hello".into(),
                })
                .unwrap(),
            )
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — broadcast was never sent")
            .expect("channel closed");

        let event_str = format!("{:?}", event);
        assert!(event_str.contains("hello"));
        assert!(event_str.contains("alice"));
        assert!(event_str.contains("kind\\\":\\\"message\\\""));
    }

    #[actix_web::test]
    async fn test_post_typing_broadcasts_to_clients() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let app_deps = deps(db, broadcaster);
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri(&format!("/typing/{}", chan_id))
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — broadcast was never sent")
            .expect("channel closed");

        let event_str = format!("{:?}", event);
        assert!(event_str.contains("kind\\\":\\\"user_typing\\\""));
        assert!(event_str.contains("alice"));
        assert!(event_str.contains(&format!("\\\"channel_id\\\":{}", chan_id)));
    }

    #[actix_web::test]
    async fn test_post_typing_rejects_unknown_channel() {
        let (db, _chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let broadcaster = Broadcaster::new();
        let app_deps = deps(db, broadcaster);
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri("/typing/99999999999999")
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_message_delete_broadcasts_to_clients() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let msg_id = generate_id();
        entity::message::ActiveModel {
            id: Set(msg_id),
            user_id: Set(user.id),
            channel_id: Set(chan_id),
            text: Set("bye".into()),
            suppress_embeds: Set(false),
        }
        .insert(&db)
        .await
        .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let app_deps = deps(db, broadcaster);
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::delete()
            .uri(&format!("/message/{}", msg_id))
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — broadcast was never sent")
            .expect("channel closed");

        let event_str = format!("{:?}", event);
        assert!(event_str.contains("kind\\\":\\\"message_deleted\\\""));
        assert!(event_str.contains(&msg_id.to_string()));
    }

    #[actix_web::test]
    async fn test_create_channel_broadcasts_to_clients() {
        let (db, _) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let app_deps = deps(db, broadcaster);
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri("/channel")
            .insert_header(ContentType::json())
            .insert_header((name, value))
            .set_payload(serde_json::json!({"name": "random"}).to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — broadcast was never sent")
            .expect("channel closed");

        let event_str = format!("{:?}", event);
        assert!(event_str.contains("kind\\\":\\\"channel_created\\\""));
        assert!(event_str.contains("random"));
    }

    #[actix_web::test]
    async fn test_reorder_channels_broadcasts_to_clients() {
        let (db, general_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let other_id = generate_id();
        entity::channel::ActiveModel {
            id: Set(other_id),
            name: Set("other".to_owned()),
            position: Set(1),
            channel_type: Set(crate::api::channels::CHANNEL_TYPE_TEXT.to_owned()),
        }
        .insert(&db)
        .await
        .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let app_deps = deps(db, broadcaster);
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::put()
            .uri("/channels/order")
            .insert_header(ContentType::json())
            .insert_header((name, value))
            .set_payload(serde_json::json!({"ids": [other_id, general_id]}).to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — broadcast was never sent")
            .expect("channel closed");
        let event_str = format!("{:?}", event);
        assert!(event_str.contains("kind\\\":\\\"channels_reordered\\\""));
    }
}
