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
    BroadcastEvent, Broadcaster, CameraVideoStoppedEvent, ScreenShareStoppedEvent,
    VoiceParticipantLeftEvent, VoiceParticipantSpeakingEvent, VoiceParticipantStatusEvent,
};
use crate::entity;
use crate::error::AppError;
use crate::util::now_unix_secs;
use crate::voice::{
    ACTIVE_MEDIA_SOURCE_CAMERA, ACTIVE_MEDIA_SOURCE_SCREEN_SHARE, AddScreenShareStreamResult,
    ScreenShareStream, VoiceConfig, VoiceParticipant, VoiceState, VoiceStatus, parse_channel_id,
    room_name,
};

const CHANNEL_TYPE_VOICE: &str = "voice";
const LIVEKIT_TRACK_SOURCE_MICROPHONE: &str = "microphone";
const LIVEKIT_TRACK_SOURCE_CAMERA: &str = ACTIVE_MEDIA_SOURCE_CAMERA;
const LIVEKIT_TRACK_SOURCE_SCREEN_SHARE: &str = ACTIVE_MEDIA_SOURCE_SCREEN_SHARE;

#[derive(Clone, Debug, Deserialize)]
pub struct ScreenShareStreamsQuery {
    pub channel_id: Option<i64>,
}

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

#[derive(Clone, Debug, Deserialize)]
pub struct VoiceStatusRequest {
    pub muted: bool,
    pub deafened: bool,
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
        can_publish_data: false,
        can_publish_sources: vec![
            LIVEKIT_TRACK_SOURCE_MICROPHONE.to_owned(),
            LIVEKIT_TRACK_SOURCE_CAMERA.to_owned(),
            LIVEKIT_TRACK_SOURCE_SCREEN_SHARE.to_owned(),
        ],
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

#[get("/voice/screen-shares")]
async fn list_screen_share_streams(
    voice_state: web::Data<VoiceState>,
    query: web::Query<ScreenShareStreamsQuery>,
) -> Result<impl Responder, AppError> {
    Ok(web::Json(
        voice_state.screen_share_streams(query.channel_id),
    ))
}

#[get("/voice/cameras")]
async fn list_camera_streams(
    voice_state: web::Data<VoiceState>,
    query: web::Query<ScreenShareStreamsQuery>,
) -> Result<impl Responder, AppError> {
    Ok(web::Json(voice_state.camera_streams(query.channel_id)))
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

/// Store the caller's current mute/deafen controls. This is accepted even when
/// the caller is not in a LiveKit room so pre-call mute/deafen is reflected in
/// the subsequent participant_joined event. If the user is currently present in
/// any voice channel, changed bits are also broadcast via SSE.
#[post("/voice/status")]
async fn post_voice_status(
    voice_state: web::Data<VoiceState>,
    broadcaster: web::Data<Broadcaster>,
    body: web::Json<VoiceStatusRequest>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let changed = voice_state.set_user_status(
        user.id,
        VoiceStatus {
            muted: body.muted,
            deafened: body.deafened,
        },
    );
    for participant in changed {
        broadcaster
            .publish(&BroadcastEvent::VoiceParticipantStatusChanged(
                VoiceParticipantStatusEvent {
                    channel_id: participant.channel_id,
                    user_id: participant.user_id,
                    muted: participant.muted,
                    deafened: participant.deafened,
                },
            ))
            .await?;
    }
    Ok(HttpResponse::NoContent().finish())
}

fn webhook_event_timestamp(event: &livekit_protocol::WebhookEvent) -> i64 {
    if event.created_at > 0 {
        event.created_at
    } else {
        now_unix_secs()
    }
}

fn active_media_source(track: &livekit_protocol::TrackInfo) -> Option<&'static str> {
    let source = livekit_protocol::TrackSource::try_from(track.source).ok()?;
    match source {
        livekit_protocol::TrackSource::Camera => Some(LIVEKIT_TRACK_SOURCE_CAMERA),
        livekit_protocol::TrackSource::ScreenShare => Some(LIVEKIT_TRACK_SOURCE_SCREEN_SHARE),
        _ => None,
    }
}

async fn active_media_stream_from_webhook(
    db: &DatabaseConnection,
    event: &livekit_protocol::WebhookEvent,
    channel_id: i64,
    user_id: i64,
) -> Result<Option<ScreenShareStream>, AppError> {
    let Some(participant) = event.participant.as_ref() else {
        return Ok(None);
    };
    let Some(track) = event.track.as_ref() else {
        return Ok(None);
    };
    let Some(source) = active_media_source(track) else {
        return Ok(None);
    };
    if track.sid.is_empty() {
        return Ok(None);
    }

    let Some(channel) = entity::channel::Entity::find_by_id(channel_id)
        .one(db)
        .await?
    else {
        return Ok(None);
    };
    if channel.channel_type != CHANNEL_TYPE_VOICE {
        return Ok(None);
    }

    let Some(user) = entity::user::Entity::find_by_id(user_id).one(db).await? else {
        return Ok(None);
    };

    Ok(Some(ScreenShareStream {
        channel_id,
        sharer_user_id: user_id,
        username: user.username,
        display_name: user.display_name,
        avatar_url: avatar_url(user.avatar_path.as_deref(), user.avatar_updated_at),
        participant_identity: participant.identity.clone(),
        track_sid: track.sid.clone(),
        track_name: track.name.clone(),
        source: source.to_owned(),
        started_at: webhook_event_timestamp(event),
    }))
}

async fn publish_media_started(
    broadcaster: &Broadcaster,
    stream: ScreenShareStream,
) -> Result<(), AppError> {
    match stream.source.as_str() {
        LIVEKIT_TRACK_SOURCE_CAMERA => {
            broadcaster
                .publish(&BroadcastEvent::CameraVideoStarted(stream))
                .await
        }
        LIVEKIT_TRACK_SOURCE_SCREEN_SHARE => {
            broadcaster
                .publish(&BroadcastEvent::ScreenShareStarted(stream))
                .await
        }
        _ => Ok(()),
    }
}

async fn publish_media_stopped(
    broadcaster: &Broadcaster,
    stream: ScreenShareStream,
) -> Result<(), AppError> {
    match stream.source.as_str() {
        LIVEKIT_TRACK_SOURCE_CAMERA => {
            broadcaster
                .publish(&BroadcastEvent::CameraVideoStopped(
                    CameraVideoStoppedEvent {
                        channel_id: stream.channel_id,
                        sharer_user_id: stream.sharer_user_id,
                        participant_identity: stream.participant_identity,
                        track_sid: stream.track_sid,
                    },
                ))
                .await
        }
        LIVEKIT_TRACK_SOURCE_SCREEN_SHARE => {
            broadcaster
                .publish(&BroadcastEvent::ScreenShareStopped(
                    ScreenShareStoppedEvent {
                        channel_id: stream.channel_id,
                        sharer_user_id: stream.sharer_user_id,
                        participant_identity: stream.participant_identity,
                        track_sid: stream.track_sid,
                    },
                ))
                .await
        }
        _ => Ok(()),
    }
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
            voice_state.mark_participant_connected(
                channel_id,
                user_id,
                &participant.identity,
                webhook_event_timestamp(event),
            );
            let status = voice_state.user_status(user_id);
            let p = VoiceParticipant {
                user_id,
                channel_id,
                username: user.username,
                avatar_url: avatar_url(user.avatar_path.as_deref(), user.avatar_updated_at),
                muted: status.muted,
                deafened: status.deafened,
            };
            if voice_state.add_participant(p.clone()) {
                broadcaster
                    .publish(&BroadcastEvent::VoiceParticipantJoined(p))
                    .await?;
            }
        }
        "participant_left" | "participant_connection_aborted" => {
            let stopped_streams = voice_state.remove_media_streams_for_participant(
                channel_id,
                user_id,
                &participant.identity,
                webhook_event_timestamp(event),
            );
            for stream in stopped_streams {
                publish_media_stopped(broadcaster, stream).await?;
            }
            if voice_state
                .remove_participant(channel_id, user_id)
                .is_some()
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
        }
        "track_published" => {
            let Some(stream) =
                active_media_stream_from_webhook(db, event, channel_id, user_id).await?
            else {
                return Ok(());
            };
            let result = match stream.source.as_str() {
                LIVEKIT_TRACK_SOURCE_CAMERA => voice_state.add_camera_stream(stream.clone()),
                LIVEKIT_TRACK_SOURCE_SCREEN_SHARE => {
                    voice_state.add_screen_share_stream(stream.clone())
                }
                _ => AddScreenShareStreamResult::Unchanged,
            };
            match result {
                AddScreenShareStreamResult::Added => {
                    publish_media_started(broadcaster, stream).await?;
                }
                AddScreenShareStreamResult::Replaced(previous) => {
                    publish_media_stopped(broadcaster, previous).await?;
                    publish_media_started(broadcaster, stream).await?;
                }
                AddScreenShareStreamResult::Unchanged => {}
            }
        }
        "track_unpublished" => {
            let Some(track) = event.track.as_ref() else {
                return Ok(());
            };
            let Some(source) = active_media_source(track) else {
                return Ok(());
            };
            if track.sid.is_empty() {
                return Ok(());
            }
            let stopped = match source {
                LIVEKIT_TRACK_SOURCE_CAMERA => voice_state.remove_camera_stream(
                    channel_id,
                    user_id,
                    &participant.identity,
                    &track.sid,
                ),
                LIVEKIT_TRACK_SOURCE_SCREEN_SHARE => voice_state.remove_screen_share_stream(
                    channel_id,
                    user_id,
                    &participant.identity,
                    &track.sid,
                ),
                _ => None,
            };
            if let Some(stream) = stopped {
                publish_media_stopped(broadcaster, stream).await?;
            }
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
        .service(list_screen_share_streams)
        .service(list_camera_streams)
        .service(post_voice_speaking)
        .service(post_voice_status);
}

pub fn configure_public_webhook(cfg: &mut web::ServiceConfig) {
    cfg.service(receive_voice_webhook);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::time::Duration;

    use actix_web::{App, test, web};
    use sea_orm::{ActiveModelTrait, Set};

    use super::*;
    use crate::api::messages::EmbedFetcher;
    use crate::auth;
    use crate::broadcast::Broadcaster;
    use crate::database::connect_initialized_database_url;
    use crate::startup::{AppDeps, configure_app};
    use crate::util::generate_id;

    async fn setup_db() -> (DatabaseConnection, i64) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let url = format!("sqlite:file:hamlet_voice_test_{n}?mode=memory&cache=shared");
        let db = connect_initialized_database_url(&url).await.unwrap();

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

    async fn insert_voice_channel(db: &DatabaseConnection, name: &str) -> i64 {
        let channel_id = generate_id();
        entity::channel::ActiveModel {
            id: Set(channel_id),
            name: Set(name.to_owned()),
            position: Set(1),
            channel_type: Set(CHANNEL_TYPE_VOICE.to_owned()),
        }
        .insert(db)
        .await
        .unwrap();
        channel_id
    }

    fn make_track_webhook_event(
        event: &str,
        channel_id: i64,
        user_id: i64,
        username: &str,
        source: livekit_protocol::TrackSource,
        track_sid: &str,
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
            track: Some(livekit_protocol::TrackInfo {
                sid: track_sid.to_owned(),
                name: match source {
                    livekit_protocol::TrackSource::Camera => "camera".to_owned(),
                    livekit_protocol::TrackSource::ScreenShare => "screen".to_owned(),
                    _ => "track".to_owned(),
                },
                source: source as i32,
                ..Default::default()
            }),
            created_at: 1_700_000_000,
            ..Default::default()
        }
    }

    async fn expect_no_broadcast(rx: &mut tokio::sync::mpsc::Receiver<actix_sse::Event>) {
        assert!(
            tokio::time::timeout(Duration::from_millis(100), rx.recv())
                .await
                .is_err(),
            "no broadcast should be sent"
        );
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
        assert!(!participants[0].muted);
        assert!(!participants[0].deafened);

        let broadcast = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let s = format!("{:?}", broadcast);
        assert!(s.contains("voice_participant_joined"));
        assert!(s.contains("alice"));
    }

    #[actix_web::test]
    async fn test_voice_webhook_join_uses_pre_call_status() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        voice_state.set_user_status(
            user.id,
            VoiceStatus {
                muted: true,
                deafened: true,
            },
        );

        let event = make_webhook_event("participant_joined", chan_id, user.id, &user.username);
        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        let participants = voice_state.participants(chan_id);
        assert_eq!(participants.len(), 1);
        assert!(participants[0].muted);
        assert!(participants[0].deafened);

        let broadcast = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let s = format!("{:?}", broadcast);
        assert!(s.contains("voice_participant_joined"));
        assert!(s.contains("muted"));
        assert!(s.contains("deafened"));
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
            muted: false,
            deafened: false,
        });

        let event = make_webhook_event("participant_left", chan_id, user.id, &user.username);
        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        assert!(voice_state.participants(chan_id).is_empty());
    }

    #[actix_web::test]
    async fn test_voice_webhook_participant_leave_removes_media_streams_and_broadcasts_stop() {
        let (db, _text_chan_id) = setup_db().await;
        let voice_chan_id = insert_voice_channel(&db, "lounge").await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        voice_state.add_participant(VoiceParticipant {
            user_id: user.id,
            channel_id: voice_chan_id,
            username: user.username.clone(),
            avatar_url: None,
            muted: false,
            deafened: false,
        });
        voice_state.add_screen_share_stream(ScreenShareStream {
            channel_id: voice_chan_id,
            sharer_user_id: user.id,
            username: user.username.clone(),
            display_name: None,
            avatar_url: None,
            participant_identity: user.id.to_string(),
            track_sid: "TR_screen".to_owned(),
            track_name: "screen".to_owned(),
            source: LIVEKIT_TRACK_SOURCE_SCREEN_SHARE.to_owned(),
            started_at: 10,
        });
        voice_state.add_camera_stream(ScreenShareStream {
            channel_id: voice_chan_id,
            sharer_user_id: user.id,
            username: user.username.clone(),
            display_name: None,
            avatar_url: None,
            participant_identity: user.id.to_string(),
            track_sid: "TR_camera".to_owned(),
            track_name: "camera".to_owned(),
            source: LIVEKIT_TRACK_SOURCE_CAMERA.to_owned(),
            started_at: 11,
        });

        let mut event =
            make_webhook_event("participant_left", voice_chan_id, user.id, &user.username);
        event.created_at = 20;
        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        assert!(voice_state.participants(voice_chan_id).is_empty());
        assert!(
            voice_state
                .screen_share_streams(Some(voice_chan_id))
                .is_empty()
        );
        assert!(voice_state.camera_streams(Some(voice_chan_id)).is_empty());

        let stopped_screen = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let stopped_screen = format!("{:?}", stopped_screen);
        assert!(stopped_screen.contains("screen_share_stopped"));
        assert!(stopped_screen.contains("TR_screen"));

        let stopped_camera = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let stopped_camera = format!("{:?}", stopped_camera);
        assert!(stopped_camera.contains("camera_video_stopped"));
        assert!(stopped_camera.contains("TR_camera"));

        let left = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let left = format!("{:?}", left);
        assert!(left.contains("voice_participant_left"));
        assert!(left.contains(&user.id.to_string()));
    }

    #[actix_web::test]
    async fn test_voice_webhook_connection_abort_removes_media_without_participant_cache() {
        let (db, _text_chan_id) = setup_db().await;
        let voice_chan_id = insert_voice_channel(&db, "lounge").await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        voice_state.add_screen_share_stream(ScreenShareStream {
            channel_id: voice_chan_id,
            sharer_user_id: user.id,
            username: user.username.clone(),
            display_name: None,
            avatar_url: None,
            participant_identity: user.id.to_string(),
            track_sid: "TR_abort_screen".to_owned(),
            track_name: "screen".to_owned(),
            source: LIVEKIT_TRACK_SOURCE_SCREEN_SHARE.to_owned(),
            started_at: 10,
        });
        voice_state.add_camera_stream(ScreenShareStream {
            channel_id: voice_chan_id,
            sharer_user_id: user.id,
            username: user.username.clone(),
            display_name: None,
            avatar_url: None,
            participant_identity: user.id.to_string(),
            track_sid: "TR_abort_camera".to_owned(),
            track_name: "camera".to_owned(),
            source: LIVEKIT_TRACK_SOURCE_CAMERA.to_owned(),
            started_at: 11,
        });

        let mut event = make_webhook_event(
            "participant_connection_aborted",
            voice_chan_id,
            user.id,
            &user.username,
        );
        event.created_at = 20;
        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        assert!(
            voice_state
                .screen_share_streams(Some(voice_chan_id))
                .is_empty()
        );
        assert!(voice_state.camera_streams(Some(voice_chan_id)).is_empty());
        let stopped_screen = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let stopped_screen = format!("{:?}", stopped_screen);
        assert!(stopped_screen.contains("screen_share_stopped"));
        assert!(stopped_screen.contains("TR_abort_screen"));
        let stopped_camera = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let stopped_camera = format!("{:?}", stopped_camera);
        assert!(stopped_camera.contains("camera_video_stopped"));
        assert!(stopped_camera.contains("TR_abort_camera"));
        expect_no_broadcast(&mut rx).await;
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
    async fn test_list_screen_share_streams_requires_auth_and_starts_empty() {
        let (db, _chan_id) = setup_db().await;
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
            emoji_storage: web::Data::new(crate::api::emoji::EmojiStorage {
                dir: std::env::temp_dir(),
            }),
        };
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let req = test::TestRequest::get()
            .uri("/voice/screen-shares")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::get()
            .uri("/voice/screen-shares")
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body, serde_json::json!([]));
    }

    #[actix_web::test]
    async fn test_list_camera_streams_requires_auth_starts_empty_and_filters_by_channel() {
        let (db, _chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();
        let voice_state = web::Data::new(VoiceState::new());

        let app_deps = AppDeps {
            db: web::Data::new(db),
            broadcaster: web::Data::from(Broadcaster::new()),
            voice_cfg: web::Data::new(None),
            voice_state: voice_state.clone(),
            embed_fetcher: web::Data::new(EmbedFetcher::Disabled),
            emoji_storage: web::Data::new(crate::api::emoji::EmojiStorage {
                dir: std::env::temp_dir(),
            }),
        };
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let req = test::TestRequest::get().uri("/voice/cameras").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::get()
            .uri("/voice/cameras")
            .insert_header((name.clone(), value.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body, serde_json::json!([]));

        voice_state.add_camera_stream(ScreenShareStream {
            channel_id: 42,
            sharer_user_id: user.id,
            username: user.username.clone(),
            display_name: None,
            avatar_url: None,
            participant_identity: user.id.to_string(),
            track_sid: "TR_camera_42".to_owned(),
            track_name: "camera".to_owned(),
            source: LIVEKIT_TRACK_SOURCE_CAMERA.to_owned(),
            started_at: 1,
        });
        voice_state.add_camera_stream(ScreenShareStream {
            channel_id: 99,
            sharer_user_id: user.id,
            username: user.username.clone(),
            display_name: None,
            avatar_url: None,
            participant_identity: user.id.to_string(),
            track_sid: "TR_camera_99".to_owned(),
            track_name: "camera".to_owned(),
            source: LIVEKIT_TRACK_SOURCE_CAMERA.to_owned(),
            started_at: 2,
        });

        let req = test::TestRequest::get()
            .uri("/voice/cameras?channel_id=42")
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["track_sid"], "TR_camera_42");
        assert_eq!(body[0]["source"], LIVEKIT_TRACK_SOURCE_CAMERA);
    }

    #[actix_web::test]
    async fn test_camera_track_publish_adds_stream_and_broadcasts() {
        let (db, _text_chan_id) = setup_db().await;
        let voice_chan_id = insert_voice_channel(&db, "lounge").await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        let event = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::Camera,
            "TR_camera_1",
        );

        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        let streams = voice_state.camera_streams(Some(voice_chan_id));
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].channel_id, voice_chan_id);
        assert_eq!(streams[0].sharer_user_id, user.id);
        assert_eq!(streams[0].username, "alice");
        assert_eq!(streams[0].participant_identity, user.id.to_string());
        assert_eq!(streams[0].track_sid, "TR_camera_1");
        assert_eq!(streams[0].track_name, "camera");
        assert_eq!(streams[0].source, LIVEKIT_TRACK_SOURCE_CAMERA);
        assert_eq!(streams[0].started_at, 1_700_000_000);

        let broadcast = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let s = format!("{:?}", broadcast);
        assert!(s.contains("camera_video_started"));
        assert!(s.contains("TR_camera_1"));
        assert!(s.contains("alice"));
    }

    #[actix_web::test]
    async fn test_screen_share_track_publish_adds_stream_and_broadcasts() {
        let (db, _text_chan_id) = setup_db().await;
        let voice_chan_id = insert_voice_channel(&db, "lounge").await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        let event = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_screen_1",
        );

        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        let streams = voice_state.screen_share_streams(Some(voice_chan_id));
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].channel_id, voice_chan_id);
        assert_eq!(streams[0].sharer_user_id, user.id);
        assert_eq!(streams[0].username, "alice");
        assert_eq!(streams[0].participant_identity, user.id.to_string());
        assert_eq!(streams[0].track_sid, "TR_screen_1");
        assert_eq!(streams[0].track_name, "screen");
        assert_eq!(streams[0].source, LIVEKIT_TRACK_SOURCE_SCREEN_SHARE);
        assert_eq!(streams[0].started_at, 1_700_000_000);

        let broadcast = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let s = format!("{:?}", broadcast);
        assert!(s.contains("screen_share_started"));
        assert!(s.contains("TR_screen_1"));
        assert!(s.contains("alice"));
    }

    #[actix_web::test]
    async fn test_screen_share_track_publish_replaces_existing_stream_for_same_user() {
        let (db, _text_chan_id) = setup_db().await;
        let voice_chan_id = insert_voice_channel(&db, "lounge").await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        let first = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_screen_1",
        );
        let second = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_screen_2",
        );

        apply_voice_webhook(&db, &voice_state, &broadcaster, &first)
            .await
            .unwrap();
        apply_voice_webhook(&db, &voice_state, &broadcaster, &second)
            .await
            .unwrap();

        let streams = voice_state.screen_share_streams(Some(voice_chan_id));
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].track_sid, "TR_screen_2");

        let started_first = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let stopped_first = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let started_second = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        let started_first = format!("{:?}", started_first);
        assert!(started_first.contains("screen_share_started"));
        assert!(started_first.contains("TR_screen_1"));
        let stopped_first = format!("{:?}", stopped_first);
        assert!(stopped_first.contains("screen_share_stopped"));
        assert!(stopped_first.contains("TR_screen_1"));
        let started_second = format!("{:?}", started_second);
        assert!(started_second.contains("screen_share_started"));
        assert!(started_second.contains("TR_screen_2"));
    }

    #[actix_web::test]
    async fn test_camera_replacement_and_screen_share_coexistence_are_source_aware() {
        let (db, _text_chan_id) = setup_db().await;
        let voice_chan_id = insert_voice_channel(&db, "lounge").await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        let screen = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_screen",
        );
        let first_camera = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::Camera,
            "TR_camera_1",
        );
        let second_camera = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::Camera,
            "TR_camera_2",
        );

        apply_voice_webhook(&db, &voice_state, &broadcaster, &screen)
            .await
            .unwrap();
        apply_voice_webhook(&db, &voice_state, &broadcaster, &first_camera)
            .await
            .unwrap();
        apply_voice_webhook(&db, &voice_state, &broadcaster, &second_camera)
            .await
            .unwrap();

        let cameras = voice_state.camera_streams(Some(voice_chan_id));
        assert_eq!(cameras.len(), 1);
        assert_eq!(cameras[0].track_sid, "TR_camera_2");
        let screens = voice_state.screen_share_streams(Some(voice_chan_id));
        assert_eq!(screens.len(), 1);
        assert_eq!(screens[0].track_sid, "TR_screen");

        let first = format!(
            "{:?}",
            tokio::time::timeout(Duration::from_secs(1), rx.recv())
                .await
                .expect("timed out")
                .expect("channel closed")
        );
        let second = format!(
            "{:?}",
            tokio::time::timeout(Duration::from_secs(1), rx.recv())
                .await
                .expect("timed out")
                .expect("channel closed")
        );
        let third = format!(
            "{:?}",
            tokio::time::timeout(Duration::from_secs(1), rx.recv())
                .await
                .expect("timed out")
                .expect("channel closed")
        );
        let fourth = format!(
            "{:?}",
            tokio::time::timeout(Duration::from_secs(1), rx.recv())
                .await
                .expect("timed out")
                .expect("channel closed")
        );
        assert!(first.contains("screen_share_started"));
        assert!(first.contains("TR_screen"));
        assert!(second.contains("camera_video_started"));
        assert!(second.contains("TR_camera_1"));
        assert!(third.contains("camera_video_stopped"));
        assert!(third.contains("TR_camera_1"));
        assert!(fourth.contains("camera_video_started"));
        assert!(fourth.contains("TR_camera_2"));
    }

    #[actix_web::test]
    async fn test_camera_track_unpublish_removes_exact_stream_and_broadcasts_stop() {
        let (db, _text_chan_id) = setup_db().await;
        let voice_chan_id = insert_voice_channel(&db, "lounge").await;
        let alice = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let bob = auth::register_user(&db, "bob", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        voice_state.add_camera_stream(ScreenShareStream {
            channel_id: voice_chan_id,
            sharer_user_id: alice.id,
            username: alice.username.clone(),
            display_name: None,
            avatar_url: None,
            participant_identity: alice.id.to_string(),
            track_sid: "TR_keep".to_owned(),
            track_name: "camera-a".to_owned(),
            source: LIVEKIT_TRACK_SOURCE_CAMERA.to_owned(),
            started_at: 1,
        });
        voice_state.add_camera_stream(ScreenShareStream {
            channel_id: voice_chan_id,
            sharer_user_id: bob.id,
            username: bob.username.clone(),
            display_name: None,
            avatar_url: None,
            participant_identity: bob.id.to_string(),
            track_sid: "TR_stop".to_owned(),
            track_name: "camera-b".to_owned(),
            source: LIVEKIT_TRACK_SOURCE_CAMERA.to_owned(),
            started_at: 2,
        });
        voice_state.add_screen_share_stream(ScreenShareStream {
            channel_id: voice_chan_id,
            sharer_user_id: bob.id,
            username: bob.username.clone(),
            display_name: None,
            avatar_url: None,
            participant_identity: bob.id.to_string(),
            track_sid: "TR_bob_screen".to_owned(),
            track_name: "screen".to_owned(),
            source: LIVEKIT_TRACK_SOURCE_SCREEN_SHARE.to_owned(),
            started_at: 3,
        });

        let event = make_track_webhook_event(
            "track_unpublished",
            voice_chan_id,
            bob.id,
            &bob.username,
            livekit_protocol::TrackSource::Camera,
            "TR_stop",
        );
        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        let cameras = voice_state.camera_streams(Some(voice_chan_id));
        assert_eq!(cameras.len(), 1);
        assert_eq!(cameras[0].track_sid, "TR_keep");
        assert_eq!(cameras[0].sharer_user_id, alice.id);
        let screens = voice_state.screen_share_streams(Some(voice_chan_id));
        assert_eq!(screens.len(), 1);
        assert_eq!(screens[0].track_sid, "TR_bob_screen");

        let broadcast = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let s = format!("{:?}", broadcast);
        assert!(s.contains("camera_video_stopped"));
        assert!(s.contains("TR_stop"));
        assert!(s.contains(&bob.id.to_string()));
    }

    #[actix_web::test]
    async fn test_screen_share_track_unpublish_removes_exact_stream_and_broadcasts_stop() {
        let (db, _text_chan_id) = setup_db().await;
        let voice_chan_id = insert_voice_channel(&db, "lounge").await;
        let alice = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let bob = auth::register_user(&db, "bob", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        voice_state.add_screen_share_stream(ScreenShareStream {
            channel_id: voice_chan_id,
            sharer_user_id: alice.id,
            username: alice.username.clone(),
            display_name: None,
            avatar_url: None,
            participant_identity: alice.id.to_string(),
            track_sid: "TR_keep".to_owned(),
            track_name: "screen-a".to_owned(),
            source: LIVEKIT_TRACK_SOURCE_SCREEN_SHARE.to_owned(),
            started_at: 1,
        });
        voice_state.add_screen_share_stream(ScreenShareStream {
            channel_id: voice_chan_id,
            sharer_user_id: bob.id,
            username: bob.username.clone(),
            display_name: None,
            avatar_url: None,
            participant_identity: bob.id.to_string(),
            track_sid: "TR_stop".to_owned(),
            track_name: "screen-b".to_owned(),
            source: LIVEKIT_TRACK_SOURCE_SCREEN_SHARE.to_owned(),
            started_at: 2,
        });

        let event = make_track_webhook_event(
            "track_unpublished",
            voice_chan_id,
            bob.id,
            &bob.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_stop",
        );
        apply_voice_webhook(&db, &voice_state, &broadcaster, &event)
            .await
            .unwrap();

        let streams = voice_state.screen_share_streams(Some(voice_chan_id));
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].track_sid, "TR_keep");
        assert_eq!(streams[0].sharer_user_id, alice.id);

        let broadcast = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let s = format!("{:?}", broadcast);
        assert!(s.contains("screen_share_stopped"));
        assert!(s.contains("TR_stop"));
        assert!(s.contains(&bob.id.to_string()));
    }

    #[actix_web::test]
    async fn test_camera_track_webhooks_are_idempotent_and_out_of_order_safe() {
        let (db, _text_chan_id) = setup_db().await;
        let voice_chan_id = insert_voice_channel(&db, "lounge").await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        let published = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::Camera,
            "TR_once",
        );

        apply_voice_webhook(&db, &voice_state, &broadcaster, &published)
            .await
            .unwrap();
        apply_voice_webhook(&db, &voice_state, &broadcaster, &published)
            .await
            .unwrap();
        assert_eq!(voice_state.camera_streams(Some(voice_chan_id)).len(), 1);
        let first = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert!(format!("{:?}", first).contains("camera_video_started"));
        expect_no_broadcast(&mut rx).await;

        let unpublished = make_track_webhook_event(
            "track_unpublished",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::Camera,
            "TR_once",
        );
        apply_voice_webhook(&db, &voice_state, &broadcaster, &unpublished)
            .await
            .unwrap();
        apply_voice_webhook(&db, &voice_state, &broadcaster, &unpublished)
            .await
            .unwrap();
        assert!(voice_state.camera_streams(Some(voice_chan_id)).is_empty());
        let first = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert!(format!("{:?}", first).contains("camera_video_stopped"));
        expect_no_broadcast(&mut rx).await;

        let unpublished_first = make_track_webhook_event(
            "track_unpublished",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::Camera,
            "TR_out_of_order",
        );
        let published_late = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::Camera,
            "TR_out_of_order",
        );
        apply_voice_webhook(&db, &voice_state, &broadcaster, &unpublished_first)
            .await
            .unwrap();
        apply_voice_webhook(&db, &voice_state, &broadcaster, &published_late)
            .await
            .unwrap();
        assert!(voice_state.camera_streams(Some(voice_chan_id)).is_empty());
        expect_no_broadcast(&mut rx).await;
    }

    #[actix_web::test]
    async fn test_screen_share_track_webhooks_are_idempotent() {
        let (db, _text_chan_id) = setup_db().await;
        let voice_chan_id = insert_voice_channel(&db, "lounge").await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        let published = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_once",
        );

        apply_voice_webhook(&db, &voice_state, &broadcaster, &published)
            .await
            .unwrap();
        apply_voice_webhook(&db, &voice_state, &broadcaster, &published)
            .await
            .unwrap();
        assert_eq!(
            voice_state.screen_share_streams(Some(voice_chan_id)).len(),
            1
        );
        let first = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert!(format!("{:?}", first).contains("screen_share_started"));
        expect_no_broadcast(&mut rx).await;

        let unpublished = make_track_webhook_event(
            "track_unpublished",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_once",
        );
        apply_voice_webhook(&db, &voice_state, &broadcaster, &unpublished)
            .await
            .unwrap();
        apply_voice_webhook(&db, &voice_state, &broadcaster, &unpublished)
            .await
            .unwrap();
        assert!(
            voice_state
                .screen_share_streams(Some(voice_chan_id))
                .is_empty()
        );
        let first = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert!(format!("{:?}", first).contains("screen_share_stopped"));
        expect_no_broadcast(&mut rx).await;
    }

    #[actix_web::test]
    async fn test_screen_share_track_unpublish_before_publish_does_not_resurrect_stream() {
        let (db, _text_chan_id) = setup_db().await;
        let voice_chan_id = insert_voice_channel(&db, "lounge").await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        let unpublished = make_track_webhook_event(
            "track_unpublished",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_out_of_order",
        );
        let published = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_out_of_order",
        );

        apply_voice_webhook(&db, &voice_state, &broadcaster, &unpublished)
            .await
            .unwrap();
        apply_voice_webhook(&db, &voice_state, &broadcaster, &published)
            .await
            .unwrap();

        assert!(
            voice_state
                .screen_share_streams(Some(voice_chan_id))
                .is_empty()
        );
        expect_no_broadcast(&mut rx).await;
    }

    #[actix_web::test]
    async fn test_participant_abort_before_delayed_publish_does_not_resurrect_stream() {
        let (db, _text_chan_id) = setup_db().await;
        let voice_chan_id = insert_voice_channel(&db, "lounge").await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();
        let mut aborted = make_webhook_event(
            "participant_connection_aborted",
            voice_chan_id,
            user.id,
            &user.username,
        );
        aborted.created_at = 30;
        let mut delayed_screen_publish = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_delayed_screen_after_abort",
        );
        delayed_screen_publish.created_at = 20;
        let mut delayed_camera_publish = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::Camera,
            "TR_delayed_camera_after_abort",
        );
        delayed_camera_publish.created_at = 21;

        apply_voice_webhook(&db, &voice_state, &broadcaster, &aborted)
            .await
            .unwrap();
        apply_voice_webhook(&db, &voice_state, &broadcaster, &delayed_screen_publish)
            .await
            .unwrap();
        apply_voice_webhook(&db, &voice_state, &broadcaster, &delayed_camera_publish)
            .await
            .unwrap();

        assert!(
            voice_state
                .screen_share_streams(Some(voice_chan_id))
                .is_empty()
        );
        assert!(voice_state.camera_streams(Some(voice_chan_id)).is_empty());
        expect_no_broadcast(&mut rx).await;
    }

    #[actix_web::test]
    async fn test_screen_share_track_webhooks_ignore_invalid_inputs() {
        let (db, text_chan_id) = setup_db().await;
        let voice_chan_id = insert_voice_channel(&db, "lounge").await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = VoiceState::new();

        let unsupported = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShareAudio,
            "TR_audio",
        );
        apply_voice_webhook(&db, &voice_state, &broadcaster, &unsupported)
            .await
            .unwrap();

        let microphone = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::Microphone,
            "TR_microphone",
        );
        apply_voice_webhook(&db, &voice_state, &broadcaster, &microphone)
            .await
            .unwrap();

        let non_voice = make_track_webhook_event(
            "track_published",
            text_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_text",
        );
        apply_voice_webhook(&db, &voice_state, &broadcaster, &non_voice)
            .await
            .unwrap();

        let missing_channel = make_track_webhook_event(
            "track_published",
            generate_id(),
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_missing_channel",
        );
        apply_voice_webhook(&db, &voice_state, &broadcaster, &missing_channel)
            .await
            .unwrap();

        let missing_user = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            generate_id(),
            "missing",
            livekit_protocol::TrackSource::ScreenShare,
            "TR_missing_user",
        );
        apply_voice_webhook(&db, &voice_state, &broadcaster, &missing_user)
            .await
            .unwrap();

        let mut malformed_identity = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_bad_identity",
        );
        malformed_identity.participant.as_mut().unwrap().identity = "not-a-user-id".to_owned();
        apply_voice_webhook(&db, &voice_state, &broadcaster, &malformed_identity)
            .await
            .unwrap();

        let mut unknown_room = make_track_webhook_event(
            "track_published",
            voice_chan_id,
            user.id,
            &user.username,
            livekit_protocol::TrackSource::ScreenShare,
            "TR_unknown_room",
        );
        unknown_room.room.as_mut().unwrap().name = "other-room".to_owned();
        apply_voice_webhook(&db, &voice_state, &broadcaster, &unknown_room)
            .await
            .unwrap();

        let missing_track = make_webhook_event("track_published", voice_chan_id, user.id, "alice");
        apply_voice_webhook(&db, &voice_state, &broadcaster, &missing_track)
            .await
            .unwrap();

        assert!(voice_state.screen_share_streams(None).is_empty());
        assert!(voice_state.camera_streams(None).is_empty());
        expect_no_broadcast(&mut rx).await;
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
            emoji_storage: web::Data::new(crate::api::emoji::EmojiStorage {
                dir: std::env::temp_dir(),
            }),
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
            emoji_storage: web::Data::new(crate::api::emoji::EmojiStorage {
                dir: std::env::temp_dir(),
            }),
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
        assert!(claims.video.can_subscribe);
        assert!(claims.video.can_publish);
        assert!(!claims.video.can_publish_data);
        assert_eq!(
            claims.video.can_publish_sources,
            vec![
                LIVEKIT_TRACK_SOURCE_MICROPHONE.to_owned(),
                LIVEKIT_TRACK_SOURCE_CAMERA.to_owned(),
                LIVEKIT_TRACK_SOURCE_SCREEN_SHARE.to_owned(),
            ]
        );
        assert!(
            claims
                .video
                .can_publish_sources
                .iter()
                .any(|source| source == "camera")
        );
        assert!(
            !claims
                .video
                .can_publish_sources
                .iter()
                .any(|source| source == "screen_share_audio")
        );
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
            emoji_storage: web::Data::new(crate::api::emoji::EmojiStorage {
                dir: std::env::temp_dir(),
            }),
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
    async fn test_post_voice_status_updates_members_and_broadcasts() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = web::Data::new(VoiceState::new());
        voice_state.add_participant(VoiceParticipant {
            user_id: user.id,
            channel_id: chan_id,
            username: user.username.clone(),
            avatar_url: None,
            muted: false,
            deafened: false,
        });

        let app_deps = AppDeps {
            db: web::Data::new(db),
            broadcaster: web::Data::from(broadcaster),
            voice_cfg: web::Data::new(None),
            voice_state: voice_state.clone(),
            embed_fetcher: web::Data::new(EmbedFetcher::Disabled),
            emoji_storage: web::Data::new(crate::api::emoji::EmojiStorage {
                dir: std::env::temp_dir(),
            }),
        };
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri("/voice/status")
            .insert_header((name, value))
            .set_json(serde_json::json!({ "muted": true, "deafened": false }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 204);

        let participants = voice_state.participants(chan_id);
        assert_eq!(participants.len(), 1);
        assert!(participants[0].muted);
        assert!(!participants[0].deafened);

        let broadcast = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let s = format!("{:?}", broadcast);
        assert!(s.contains("voice_participant_status_changed"));
        assert!(s.contains(&user.id.to_string()));
        assert!(s.contains("muted"));
    }

    #[actix_web::test]
    async fn test_post_voice_status_stores_pre_call_without_broadcast() {
        let (db, _chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let broadcaster = Broadcaster::new();
        let mut rx = broadcaster.test_client();
        let voice_state = web::Data::new(VoiceState::new());
        let app_deps = AppDeps {
            db: web::Data::new(db),
            broadcaster: web::Data::from(broadcaster),
            voice_cfg: web::Data::new(None),
            voice_state: voice_state.clone(),
            embed_fetcher: web::Data::new(EmbedFetcher::Disabled),
            emoji_storage: web::Data::new(crate::api::emoji::EmojiStorage {
                dir: std::env::temp_dir(),
            }),
        };
        let app =
            test::init_service(App::new().configure(|cfg| configure_app(cfg, app_deps.clone())))
                .await;

        let (name, value) = session_cookie_header(&session.token);
        let req = test::TestRequest::post()
            .uri("/voice/status")
            .insert_header((name, value))
            .set_json(serde_json::json!({ "muted": true, "deafened": true }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 204);
        assert_eq!(
            voice_state.user_status(user.id),
            VoiceStatus {
                muted: true,
                deafened: true,
            }
        );
        expect_no_broadcast(&mut rx).await;
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
            muted: false,
            deafened: false,
        });

        let app_deps = AppDeps {
            db: web::Data::new(db),
            broadcaster: web::Data::from(broadcaster),
            voice_cfg: web::Data::new(None),
            voice_state: web::Data::new(voice_state),
            embed_fetcher: web::Data::new(EmbedFetcher::Disabled),
            emoji_storage: web::Data::new(crate::api::emoji::EmojiStorage {
                dir: std::env::temp_dir(),
            }),
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
            emoji_storage: web::Data::new(crate::api::emoji::EmojiStorage {
                dir: std::env::temp_dir(),
            }),
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
