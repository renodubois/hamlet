//! Voice HTTP handlers — token minting, participant listing, speaking
//! relay, and the LiveKit webhook receiver.
//!
//! The participant cache, room name parsing, and `VoiceConfig` shape live
//! in `crate::voice`; this module is the HTTP surface on top.

use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::{Deserialize, Serialize};

use crate::api::avatars::avatar_url;
use crate::auth::AuthUser;
use crate::broadcast::{
    BroadcastEvent, Broadcaster, VoiceParticipantLeftEvent, VoiceParticipantSpeakingEvent,
};
use crate::entity;
use crate::error::AppError;
use crate::voice::{VoiceConfig, VoiceParticipant, VoiceState, parse_channel_id, room_name};

const CHANNEL_TYPE_VOICE: &str = "voice";

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
pub struct VoiceTokenResponse {
    pub url: String,
    pub token: String,
    pub room: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VoiceSpeakingRequest {
    pub channel_id: i64,
    pub speaking: bool,
}

#[post("/voice/token/{channel_id}")]
async fn mint_voice_token(
    db: web::Data<DatabaseConnection>,
    voice_cfg: web::Data<Option<VoiceConfig>>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let channel_id = path.into_inner();
    let cfg = voice_cfg
        .as_ref()
        .as_ref()
        .ok_or(AppError::ServiceUnavailable)?;

    let channel = entity::channel::Entity::find_by_id(channel_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NoChannelFound)?;
    if channel.channel_type != CHANNEL_TYPE_VOICE {
        return Err(AppError::InvalidRequest);
    }

    let room = room_name(channel_id);
    let grants = livekit_api::access_token::VideoGrants {
        room_join: true,
        room: room.clone(),
        can_publish: true,
        can_subscribe: true,
        can_publish_data: true,
        ..Default::default()
    };
    let token = livekit_api::access_token::AccessToken::with_api_key(&cfg.api_key, &cfg.api_secret)
        .with_identity(&user.id.to_string())
        .with_name(&user.username)
        .with_grants(grants)
        .to_jwt()
        .map_err(|e| AppError::Internal(format!("livekit token: {e}")))?;

    Ok(web::Json(VoiceTokenResponse {
        url: cfg.url.clone(),
        token,
        room,
    }))
}

#[get("/voice/participants/{channel_id}")]
async fn list_voice_participants(
    voice_state: web::Data<VoiceState>,
    path: web::Path<i64>,
) -> Result<impl Responder, AppError> {
    let channel_id = path.into_inner();
    Ok(web::Json(voice_state.participants(channel_id)))
}

/// Broadcast a user's speaking-state transition to every SSE subscriber.
///
/// The client is the source of truth here — LiveKit does emit speaking
/// events server-side but not via webhooks, so participants forward their
/// own local transitions. We gate on membership so a stray POST from a
/// user who isn't actually in the channel can't spoof a ring.
#[post("/voice/speaking")]
async fn post_voice_speaking(
    voice_state: web::Data<VoiceState>,
    broadcaster: web::Data<Broadcaster>,
    body: web::Json<VoiceSpeakingRequest>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let is_member = voice_state
        .participants(body.channel_id)
        .iter()
        .any(|p| p.user_id == user.id);
    if !is_member {
        return Err(AppError::Forbidden);
    }
    broadcaster
        .publish(&BroadcastEvent::VoiceParticipantSpeakingChanged(
            VoiceParticipantSpeakingEvent {
                channel_id: body.channel_id,
                user_id: user.id,
                speaking: body.speaking,
            },
        ))
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

/// Apply a single parsed LiveKit webhook event to in-memory state and
/// broadcast the resulting SSE payload. Split out of the HTTP handler so
/// it can be exercised by unit tests without a signed-JWT round-trip.
pub async fn apply_voice_webhook(
    db: &DatabaseConnection,
    voice_state: &VoiceState,
    broadcaster: &Broadcaster,
    event: &livekit_protocol::WebhookEvent,
) -> Result<(), AppError> {
    let Some(room) = event.room.as_ref() else {
        return Ok(());
    };
    let Some(channel_id) = parse_channel_id(&room.name) else {
        return Ok(());
    };
    let Some(participant) = event.participant.as_ref() else {
        return Ok(());
    };
    let Ok(user_id) = participant.identity.parse::<i64>() else {
        return Ok(());
    };

    match event.event.as_str() {
        "participant_joined" => {
            let Some(user) = entity::user::Entity::find_by_id(user_id).one(db).await? else {
                return Ok(());
            };
            let p = VoiceParticipant {
                user_id,
                channel_id,
                username: user.username,
                avatar_url: avatar_url(user.avatar_path.as_deref(), user.avatar_updated_at),
            };
            if voice_state.add_participant(p.clone()) {
                broadcaster
                    .publish(&BroadcastEvent::VoiceParticipantJoined(p))
                    .await?;
            }
        }
        "participant_left" | "participant_connection_aborted"
            if voice_state
                .remove_participant(channel_id, user_id)
                .is_some() =>
        {
            broadcaster
                .publish(&BroadcastEvent::VoiceParticipantLeft(
                    VoiceParticipantLeftEvent {
                        channel_id,
                        user_id,
                    },
                ))
                .await?;
        }
        _ => {}
    }
    Ok(())
}

#[post("/livekit/webhook")]
async fn receive_voice_webhook(
    db: web::Data<DatabaseConnection>,
    voice_state: web::Data<VoiceState>,
    broadcaster: web::Data<Broadcaster>,
    voice_cfg: web::Data<Option<VoiceConfig>>,
    req: HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse, AppError> {
    let cfg = voice_cfg
        .as_ref()
        .as_ref()
        .ok_or(AppError::ServiceUnavailable)?;
    let auth = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;
    let body_str = std::str::from_utf8(&body).map_err(|_| AppError::InvalidRequest)?;

    let verifier =
        livekit_api::access_token::TokenVerifier::with_api_key(&cfg.api_key, &cfg.api_secret);
    let receiver = livekit_api::webhooks::WebhookReceiver::new(verifier);
    let event = receiver
        .receive(body_str, auth)
        .map_err(|_| AppError::Unauthorized)?;

    apply_voice_webhook(
        db.get_ref(),
        voice_state.get_ref(),
        broadcaster.get_ref(),
        &event,
    )
    .await?;
    Ok(HttpResponse::Ok().finish())
}

/// Auth-gated voice handlers. The LiveKit webhook authenticates via signed
/// JWT in the body, not a session cookie, and is registered separately.
pub fn configure_authed(cfg: &mut web::ServiceConfig) {
    cfg.service(mint_voice_token)
        .service(list_voice_participants)
        .service(post_voice_speaking);
}

pub fn configure_public_webhook(cfg: &mut web::ServiceConfig) {
    cfg.service(receive_voice_webhook);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::time::Duration;

    use actix_web::{App, test, web};
    use sea_orm::{ActiveModelTrait, Database, Set};

    use super::*;
    use crate::api::messages::EmbedFetcher;
    use crate::auth;
    use crate::broadcast::Broadcaster;
    use crate::startup::{AppDeps, configure_app};
    use crate::util::generate_id;

    async fn setup_db() -> (DatabaseConnection, i64) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let url = format!("sqlite:file:hamlet_voice_test_{n}?mode=memory&cache=shared");
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

    fn make_webhook_event(
        event: &str,
        channel_id: i64,
        user_id: i64,
        username: &str,
    ) -> livekit_protocol::WebhookEvent {
        livekit_protocol::WebhookEvent {
            event: event.to_string(),
            room: Some(livekit_protocol::Room {
                name: room_name(channel_id),
                ..Default::default()
            }),
            participant: Some(livekit_protocol::ParticipantInfo {
                identity: user_id.to_string(),
                name: username.to_string(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[actix_web::test]
    async fn test_voice_webhook_join_updates_state_and_broadcasts() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();

        let event = make_webhook_event("participant_joined", chan_id, user.id, &user.username);
        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        let participants = voice_state.participants(chan_id);
        assert_eq!(participants.len(), 1);
        assert_eq!(participants[0].user_id, user.id);
        assert_eq!(participants[0].username, "alice");

        let broadcast = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let s = format!("{:?}", broadcast);
        assert!(s.contains("voice_participant_joined"));
        assert!(s.contains("alice"));
    }

    #[actix_web::test]
    async fn test_voice_webhook_leave_removes_state() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let voice_state = VoiceState::new();
        voice_state.add_participant(VoiceParticipant {
            user_id: user.id,
            channel_id: chan_id,
            username: user.username.clone(),
            avatar_url: None,
        });

        let event = make_webhook_event("participant_left", chan_id, user.id, &user.username);
        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        assert!(voice_state.participants(chan_id).is_empty());
    }

    #[actix_web::test]
    async fn test_voice_webhook_ignores_unknown_room() {
        let (db, _chan) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();

        // A room that doesn't match our `channel-{id}` scheme must be ignored.
        let event = livekit_protocol::WebhookEvent {
            event: "participant_joined".into(),
            room: Some(livekit_protocol::Room {
                name: "some-other-tenant".into(),
                ..Default::default()
            }),
            participant: Some(livekit_protocol::ParticipantInfo {
                identity: user.id.to_string(),
                name: user.username.clone(),
                ..Default::default()
            }),
            ..Default::default()
        };

        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        assert!(
            tokio::time::timeout(Duration::from_millis(100), rx.recv())
                .await
                .is_err(),
            "no broadcast should be sent for an unrelated room"
        );
    }

    #[actix_web::test]
    async fn test_mint_voice_token_requires_voice_channel() {
        let (db, text_chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let voice_cfg = VoiceConfig {
            url: "ws://localhost:7880".to_string(),
            api_key: "devkey".to_string(),
            api_secret: "devsecretdevsecretdevsecretdevsecret".to_string(),
        };
        let app_deps = AppDeps {
            db: web::Data::new(db),
            broadcaster: web::Data::from(Broadcaster::new()),
            voice_cfg: web::Data::new(Some(voice_cfg)),
            voice_state: web::Data::new(VoiceState::new()),
            embed_fetcher: web::Data::new(EmbedFetcher::Disabled),
        };
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri(&format!("/voice/token/{}", text_chan_id))
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "text channel should be rejected");
    }

    #[actix_web::test]
    async fn test_mint_voice_token_returns_jwt_for_voice_channel() {
        let (db, _text) = setup_db().await;
        let voice_chan_id = generate_id();
        entity::channel::ActiveModel {
            id: Set(voice_chan_id),
            name: Set("lounge".into()),
            position: Set(1),
            channel_type: Set(CHANNEL_TYPE_VOICE.into()),
        }
        .insert(&db)
        .await
        .unwrap();

        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let voice_cfg = VoiceConfig {
            url: "ws://localhost:7880".to_string(),
            api_key: "devkey".to_string(),
            api_secret: "devsecretdevsecretdevsecretdevsecret".to_string(),
        };
        let app_deps = AppDeps {
            db: web::Data::new(db),
            broadcaster: web::Data::from(Broadcaster::new()),
            voice_cfg: web::Data::new(Some(voice_cfg.clone())),
            voice_state: web::Data::new(VoiceState::new()),
            embed_fetcher: web::Data::new(EmbedFetcher::Disabled),
        };
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri(&format!("/voice/token/{}", voice_chan_id))
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body: VoiceTokenResponse = test::read_body_json(resp).await;
        assert_eq!(body.url, voice_cfg.url);
        assert_eq!(body.room, room_name(voice_chan_id));

        let verifier = livekit_api::access_token::TokenVerifier::with_api_key(
            &voice_cfg.api_key,
            &voice_cfg.api_secret,
        );
        let claims = verifier.verify(&body.token).expect("token must verify");
        assert_eq!(claims.sub, user.id.to_string());
        assert_eq!(claims.name, "alice");
        assert_eq!(claims.video.room, room_name(voice_chan_id));
        assert!(claims.video.room_join);
    }

    #[actix_web::test]
    async fn test_mint_voice_token_returns_503_when_unconfigured() {
        let (db, _) = setup_db().await;
        let voice_chan_id = generate_id();
        entity::channel::ActiveModel {
            id: Set(voice_chan_id),
            name: Set("lounge".into()),
            position: Set(1),
            channel_type: Set(CHANNEL_TYPE_VOICE.into()),
        }
        .insert(&db)
        .await
        .unwrap();

        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();
        let app_deps = AppDeps {
            db: web::Data::new(db),
            broadcaster: web::Data::from(Broadcaster::new()),
            voice_cfg: web::Data::new(None),
            voice_state: web::Data::new(VoiceState::new()),
            embed_fetcher: web::Data::new(EmbedFetcher::Disabled),
        };
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri(&format!("/voice/token/{}", voice_chan_id))
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 503);
    }

    #[actix_web::test]
    async fn test_post_voice_speaking_broadcasts_for_members() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        voice_state.add_participant(VoiceParticipant {
            user_id: user.id,
            channel_id: chan_id,
            username: user.username.clone(),
            avatar_url: None,
        });

        let app_deps = AppDeps {
            db: web::Data::new(db),
            broadcaster: web::Data::from(broadcaster),
            voice_cfg: web::Data::new(None),
            voice_state: web::Data::new(voice_state),
            embed_fetcher: web::Data::new(EmbedFetcher::Disabled),
        };
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri("/voice/speaking")
            .insert_header((name, value))
            .set_json(serde_json::json!({ "channel_id": chan_id, "speaking": true }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 204);

        let broadcast = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let s = format!("{:?}", broadcast);
        assert!(s.contains("voice_participant_speaking_changed"));
        assert!(s.contains(&user.id.to_string()));
    }

    #[actix_web::test]
    async fn test_post_voice_speaking_rejects_non_members() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let app_deps = AppDeps {
            db: web::Data::new(db),
            broadcaster: web::Data::from(Broadcaster::new()),
            voice_cfg: web::Data::new(None),
            voice_state: web::Data::new(VoiceState::new()),
            embed_fetcher: web::Data::new(EmbedFetcher::Disabled),
        };
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri("/voice/speaking")
            .insert_header((name, value))
            .set_json(serde_json::json!({ "channel_id": chan_id, "speaking": true }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
    }
}
