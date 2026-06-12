//! Message HTTP handlers + the SSE subscribe + typing notifications +
//! embed-fetch orchestration.

use std::collections::HashMap;
use std::path::Path;

use actix_multipart::Multipart;
use actix_web::http::header::CONTENT_TYPE;
use actix_web::{HttpResponse, Responder, delete, get, guard, post, put, web};
use futures_util::TryStreamExt;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};

use crate::api::attachments::{AttachmentStorage, resolve_storage_path};
use crate::api::avatars::avatar_url;
use crate::api::channels::{CHANNEL_TYPE_TEXT, ChannelResponse};
use crate::auth::AuthUser;
use crate::broadcast::{
    BroadcastEvent, Broadcaster, MessageDeletedEvent, MessageEmbedsUpdatedEvent,
    MessageReactionsUpdatedEvent, ThreadReplyCreatedEvent, ThreadReplyDeletedEvent,
    UserTypingEvent,
};
use crate::embeds;
use crate::entity;
use crate::error::AppError;
use crate::photos::{
    MAX_MESSAGE_PHOTOS, PHOTO_MAX_BYTES, ProcessedPhoto, STORED_PHOTO_CONTENT_TYPE, UploadedPhoto,
    process_uploaded_photos,
};
use crate::reactions::{
    ReactionRequest, ReactionSummary, add_reaction, load_reaction_summaries, remove_reaction,
};
use crate::util::{generate_id, now_unix_micros};

/// Cap on how many URLs per message we actually fetch. If a message is a wall
/// of 200 links we still broadcast it instantly — we just don't try to turn
/// them all into embed cards.
const MAX_EMBEDS_PER_MESSAGE: usize = 5;
const DEFAULT_THREAD_REPLY_LIMIT: u64 = 50;
const MAX_THREAD_REPLY_LIMIT: u64 = 100;
const MULTIPART_TEXT_MAX_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SendMessageRequest {
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ThreadResponse {
    pub root: MessageResponse,
    pub replies: Vec<MessageResponse>,
    pub has_more_replies: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ParticipatedThreadPreview {
    pub channel: ChannelResponse,
    pub root: MessageResponse,
    pub reply_count: u64,
    pub last_reply_created_at: i64,
    pub recent_replies: Vec<MessageResponse>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ThreadPageQuery {
    pub limit: Option<u64>,
    pub before_created_at: Option<i64>,
    pub before_id: Option<i64>,
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
    pub parent_id: Option<i64>,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
    pub text: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub suppress_embeds: bool,
    pub attachments: Vec<AttachmentResponse>,
    pub embeds: Vec<EmbedResponse>,
    pub reactions: Vec<ReactionSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_summary: Option<ThreadSummary>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ThreadSummary {
    pub reply_count: u64,
    pub last_reply_created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AttachmentResponse {
    pub id: i64,
    pub message_id: i64,
    pub position: i32,
    pub content_type: String,
    pub byte_size: i64,
    pub width: i32,
    pub height: i32,
    pub url: String,
    pub thumbnail_url: String,
    pub thumbnail_content_type: String,
    pub thumbnail_byte_size: i64,
    pub thumbnail_width: i32,
    pub thumbnail_height: i32,
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

impl From<entity::message_attachment::Model> for AttachmentResponse {
    fn from(attachment: entity::message_attachment::Model) -> Self {
        Self {
            id: attachment.id,
            message_id: attachment.message_id,
            position: attachment.position,
            content_type: attachment.content_type,
            byte_size: attachment.byte_size,
            width: attachment.width,
            height: attachment.height,
            url: attachment_url(attachment.id),
            thumbnail_url: attachment_thumbnail_url(attachment.id),
            thumbnail_content_type: attachment.thumbnail_content_type,
            thumbnail_byte_size: attachment.thumbnail_byte_size,
            thumbnail_width: attachment.thumbnail_width,
            thumbnail_height: attachment.thumbnail_height,
        }
    }
}

fn attachment_url(id: i64) -> String {
    format!("/attachments/{id}")
}

fn attachment_thumbnail_url(id: i64) -> String {
    format!("/attachments/{id}/thumbnail")
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
    user: AuthUser,
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
        .filter(entity::message::Column::ParentId.is_null())
        .order_by_asc(entity::message::Column::CreatedAt)
        .order_by_asc(entity::message::Column::Id)
        .find_also_related(entity::user::Entity)
        .all(db.get_ref())
        .await?;

    let message_ids: Vec<i64> = rows.iter().map(|(m, _)| m.id).collect();
    let embeds_by_message = load_embeds_for_messages(db.get_ref(), &message_ids).await?;
    let attachments_by_message = load_attachments_for_messages(db.get_ref(), &message_ids).await?;
    let reactions_by_message = load_reaction_summaries(db.get_ref(), &message_ids, user.id).await?;
    let summaries_by_message = load_thread_summaries_for_roots(db.get_ref(), &message_ids).await?;

    let messages: Vec<MessageResponse> = rows
        .into_iter()
        .map(|(m, u)| {
            let embeds = embeds_by_message.get(&m.id).cloned().unwrap_or_default();
            let attachments = attachments_by_message
                .get(&m.id)
                .cloned()
                .unwrap_or_default();
            let reactions = reactions_by_message.get(&m.id).cloned().unwrap_or_default();
            let mut response = message_response_from_model(m, u, embeds, attachments, reactions);
            response.thread_summary = summaries_by_message.get(&response.id).cloned();
            response
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

/// Load all attachments for a batch of message ids, grouped by message id.
async fn load_attachments_for_messages(
    db: &DatabaseConnection,
    ids: &[i64],
) -> Result<HashMap<i64, Vec<AttachmentResponse>>, AppError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = entity::message_attachment::Entity::find()
        .filter(entity::message_attachment::Column::MessageId.is_in(ids.iter().copied()))
        .order_by_asc(entity::message_attachment::Column::Position)
        .order_by_asc(entity::message_attachment::Column::Id)
        .all(db)
        .await?;
    let mut out: HashMap<i64, Vec<AttachmentResponse>> = HashMap::new();
    for row in rows {
        out.entry(row.message_id)
            .or_default()
            .push(AttachmentResponse::from(row));
    }
    Ok(out)
}

/// Compute compact thread summaries for root messages that currently have at
/// least one persisted reply.
async fn load_thread_summaries_for_roots(
    db: &DatabaseConnection,
    root_ids: &[i64],
) -> Result<HashMap<i64, ThreadSummary>, AppError> {
    if root_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let replies = entity::message::Entity::find()
        .filter(entity::message::Column::ParentId.is_in(root_ids.iter().copied()))
        .all(db)
        .await?;

    let mut summaries: HashMap<i64, ThreadSummary> = HashMap::new();
    for reply in replies {
        let Some(root_id) = reply.parent_id else {
            continue;
        };
        let summary = summaries.entry(root_id).or_insert(ThreadSummary {
            reply_count: 0,
            last_reply_created_at: reply.created_at,
        });
        summary.reply_count += 1;
        summary.last_reply_created_at = summary.last_reply_created_at.max(reply.created_at);
    }

    Ok(summaries)
}

#[derive(Debug)]
struct MultipartMessageCreate {
    text: String,
    photos: Vec<UploadedPhoto>,
}

#[derive(Debug)]
struct PendingAttachmentInsert {
    id: i64,
    position: i32,
    content_type: String,
    byte_size: i64,
    width: i32,
    height: i32,
    storage_path: String,
    thumbnail_content_type: String,
    thumbnail_byte_size: i64,
    thumbnail_width: i32,
    thumbnail_height: i32,
    thumbnail_storage_path: String,
}

#[derive(Clone, Copy, Debug)]
enum MessageCreateTarget {
    TopLevel { channel_id: i64 },
    ThreadReply { root_message_id: i64 },
}

#[post("/message/{channel_id}", guard = "message_create_is_json")]
async fn create_message_json(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    embed_fetcher: web::Data<EmbedFetcher>,
    path: web::Path<i64>,
    body: web::Json<SendMessageRequest>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let channel_id = path.into_inner();
    let message_id = generate_id();
    let (inserted, attachments) = insert_message_rows(
        db.get_ref(),
        message_id,
        user.id,
        MessageCreateTarget::TopLevel { channel_id },
        body.text.clone(),
        Vec::new(),
    )
    .await?;
    let resp = created_message_response(&inserted, &user, attachments);
    publish_created_message(db, broadcaster, embed_fetcher, inserted, resp).await
}

#[post("/message/{channel_id}", guard = "message_create_is_multipart")]
async fn create_message_multipart(
    db: web::Data<DatabaseConnection>,
    storage: web::Data<AttachmentStorage>,
    broadcaster: web::Data<Broadcaster>,
    embed_fetcher: web::Data<EmbedFetcher>,
    path: web::Path<i64>,
    payload: Multipart,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let channel_id = path.into_inner();
    create_multipart_message(
        db,
        storage,
        broadcaster,
        embed_fetcher,
        MessageCreateTarget::TopLevel { channel_id },
        payload,
        user,
    )
    .await
}

#[post("/message/{channel_id}")]
async fn create_message_unsupported_content_type() -> Result<HttpResponse, AppError> {
    Err(AppError::InvalidRequest)
}

async fn create_multipart_message(
    db: web::Data<DatabaseConnection>,
    storage: web::Data<AttachmentStorage>,
    broadcaster: web::Data<Broadcaster>,
    embed_fetcher: web::Data<EmbedFetcher>,
    target: MessageCreateTarget,
    payload: Multipart,
    user: AuthUser,
) -> Result<web::Json<MessageResponse>, AppError> {
    let multipart = read_multipart_message_create(payload).await?;
    if multipart.text.trim().is_empty() && multipart.photos.is_empty() {
        return Err(AppError::MessageContentRequired);
    }

    let message_id = generate_id();
    let processed = process_uploaded_photos(multipart.photos).await?;
    let pending = write_processed_attachments(&storage.dir, message_id, processed).await?;
    let pending_paths = attachment_paths(&pending);
    let rows = insert_message_rows(
        db.get_ref(),
        message_id,
        user.id,
        target,
        multipart.text,
        pending,
    )
    .await;
    let (inserted, attachments) = match rows {
        Ok(rows) => rows,
        Err(err) => {
            cleanup_attachment_files(&storage.dir, &pending_paths).await;
            return Err(err);
        }
    };
    let resp = created_message_response(&inserted, &user, attachments);
    publish_created_message(db, broadcaster, embed_fetcher, inserted, resp).await
}

fn message_create_is_json(ctx: &guard::GuardContext<'_>) -> bool {
    request_content_type(ctx).is_some_and(|content_type| content_type == "application/json")
}

fn message_create_is_multipart(ctx: &guard::GuardContext<'_>) -> bool {
    request_content_type(ctx).is_some_and(|content_type| content_type == "multipart/form-data")
}

fn request_content_type(ctx: &guard::GuardContext<'_>) -> Option<String> {
    ctx.head()
        .headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .map(str::to_ascii_lowercase)
}

async fn read_multipart_message_create(
    mut payload: Multipart,
) -> Result<MultipartMessageCreate, AppError> {
    let mut text = String::new();
    let mut photos = Vec::new();

    while let Some(field) = payload
        .try_next()
        .await
        .map_err(|_| AppError::InvalidRequest)?
    {
        let field_name = field
            .content_disposition()
            .and_then(|d| d.get_name())
            .unwrap_or_default()
            .to_owned();

        match field_name.as_str() {
            "text" => text = read_multipart_text_field(field).await?,
            "photo" | "photos" => {
                if photos.len() >= MAX_MESSAGE_PHOTOS {
                    return Err(AppError::TooManyAttachments);
                }
                let content_type = field
                    .content_type()
                    .map(|m| m.essence_str().to_ascii_lowercase())
                    .unwrap_or_default();
                let bytes = read_multipart_photo_field(field).await?;
                photos.push(UploadedPhoto {
                    content_type,
                    bytes,
                });
            }
            _ => drain_multipart_field(field).await?,
        }
    }

    Ok(MultipartMessageCreate { text, photos })
}

async fn read_multipart_text_field(mut field: actix_multipart::Field) -> Result<String, AppError> {
    let mut buf = Vec::new();
    while let Some(chunk) = field
        .try_next()
        .await
        .map_err(|_| AppError::InvalidRequest)?
    {
        if chunk.len() > MULTIPART_TEXT_MAX_BYTES.saturating_sub(buf.len()) {
            return Err(AppError::InvalidRequest);
        }
        buf.extend_from_slice(&chunk);
    }
    String::from_utf8(buf).map_err(|_| AppError::InvalidRequest)
}

async fn read_multipart_photo_field(
    mut field: actix_multipart::Field,
) -> Result<Vec<u8>, AppError> {
    let mut buf = Vec::new();
    while let Some(chunk) = field
        .try_next()
        .await
        .map_err(|_| AppError::InvalidRequest)?
    {
        if chunk.len() > PHOTO_MAX_BYTES.saturating_sub(buf.len()) {
            return Err(AppError::PayloadTooLarge);
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

async fn drain_multipart_field(mut field: actix_multipart::Field) -> Result<(), AppError> {
    let mut total = 0usize;
    while let Some(chunk) = field
        .try_next()
        .await
        .map_err(|_| AppError::InvalidRequest)?
    {
        if chunk.len() > PHOTO_MAX_BYTES.saturating_sub(total) {
            return Err(AppError::PayloadTooLarge);
        }
        total += chunk.len();
    }
    Ok(())
}

async fn write_processed_attachments(
    storage_root: &Path,
    message_id: i64,
    photos: Vec<ProcessedPhoto>,
) -> Result<Vec<PendingAttachmentInsert>, AppError> {
    let mut pending = Vec::with_capacity(photos.len());
    let mut written_paths = Vec::new();

    for (position, photo) in photos.into_iter().enumerate() {
        let attachment_id = generate_id();
        let full_path = format!("messages/{message_id}/{attachment_id}/full.webp");
        let thumbnail_path = format!("messages/{message_id}/{attachment_id}/thumbnail.webp");

        if let Err(err) = write_attachment_file(storage_root, &full_path, &photo.full_bytes).await {
            cleanup_attachment_files(storage_root, &written_paths).await;
            return Err(err);
        }
        written_paths.push(full_path.clone());

        if let Err(err) =
            write_attachment_file(storage_root, &thumbnail_path, &photo.thumbnail_bytes).await
        {
            cleanup_attachment_files(storage_root, &written_paths).await;
            return Err(err);
        }
        written_paths.push(thumbnail_path.clone());

        pending.push(PendingAttachmentInsert {
            id: attachment_id,
            position: position as i32,
            content_type: STORED_PHOTO_CONTENT_TYPE.to_owned(),
            byte_size: photo.full_bytes.len() as i64,
            width: photo.full_width,
            height: photo.full_height,
            storage_path: full_path,
            thumbnail_content_type: STORED_PHOTO_CONTENT_TYPE.to_owned(),
            thumbnail_byte_size: photo.thumbnail_bytes.len() as i64,
            thumbnail_width: photo.thumbnail_width,
            thumbnail_height: photo.thumbnail_height,
            thumbnail_storage_path: thumbnail_path,
        });
    }

    Ok(pending)
}

async fn write_attachment_file(
    storage_root: &Path,
    relative_path: &str,
    bytes: &[u8],
) -> Result<(), AppError> {
    let final_path = storage_root.join(relative_path);
    let Some(parent) = final_path.parent() else {
        return Err(AppError::Internal(
            "attachment path has no parent".to_owned(),
        ));
    };
    tokio::fs::create_dir_all(parent).await?;

    let filename = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Internal("invalid attachment file name".to_owned()))?;
    let tmp_path = final_path.with_file_name(format!("{filename}.tmp-{}", generate_id()));
    if let Err(err) = tokio::fs::write(&tmp_path, bytes).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(AppError::Io(err));
    }
    if let Err(err) = tokio::fs::rename(&tmp_path, &final_path).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(AppError::Io(err));
    }

    Ok(())
}

fn attachment_paths(attachments: &[PendingAttachmentInsert]) -> Vec<String> {
    let mut paths = Vec::with_capacity(attachments.len() * 2);
    for attachment in attachments {
        paths.push(attachment.storage_path.clone());
        paths.push(attachment.thumbnail_storage_path.clone());
    }
    paths
}

fn stored_attachment_paths(attachments: &[entity::message_attachment::Model]) -> Vec<String> {
    let mut paths = Vec::with_capacity(attachments.len() * 2);
    for attachment in attachments {
        paths.push(attachment.storage_path.clone());
        paths.push(attachment.thumbnail_storage_path.clone());
    }
    paths
}

async fn cleanup_attachment_files(storage_root: &Path, relative_paths: &[String]) {
    for relative_path in relative_paths {
        let path = match resolve_storage_path(storage_root, relative_path) {
            Ok(path) => path,
            Err(err) => {
                tracing::warn!(relative_path, error = %err, "attachment cleanup path rejected");
                continue;
            }
        };
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                tracing::warn!(path = %path.display(), error = %err, "attachment cleanup failed");
            }
        }
    }
}

async fn cleanup_deleted_attachment_files(
    storage: Option<&AttachmentStorage>,
    message_id: i64,
    cleanup_paths: &[String],
) {
    if cleanup_paths.is_empty() {
        return;
    }

    let Some(storage) = storage else {
        tracing::warn!(
            message_id,
            "attachment storage missing; skipped file cleanup"
        );
        return;
    };

    cleanup_attachment_files(&storage.dir, cleanup_paths).await;
}

async fn ensure_channel_exists(db: &DatabaseConnection, channel_id: i64) -> Result<(), AppError> {
    let channel = entity::channel::Entity::find_by_id(channel_id)
        .one(db)
        .await?;
    if channel.is_none() {
        return Err(AppError::NoChannelFound);
    }
    Ok(())
}

async fn insert_message_rows(
    db: &DatabaseConnection,
    message_id: i64,
    user_id: i64,
    target: MessageCreateTarget,
    text: String,
    attachments: Vec<PendingAttachmentInsert>,
) -> Result<(entity::message::Model, Vec<AttachmentResponse>), AppError> {
    let (channel_id, parent_id) = match target {
        MessageCreateTarget::TopLevel { channel_id } => {
            ensure_channel_exists(db, channel_id).await?;
            (channel_id, None)
        }
        MessageCreateTarget::ThreadReply { root_message_id } => {
            let root = validated_thread_root(db, root_message_id).await?;
            (root.channel_id, Some(root_message_id))
        }
    };

    let txn = db.begin().await?;
    let new_message = entity::message::ActiveModel {
        id: Set(message_id),
        user_id: Set(user_id),
        channel_id: Set(channel_id),
        parent_id: Set(parent_id),
        created_at: Set(now_unix_micros()),
        deleted_at: Set(None),
        text: Set(text),
        suppress_embeds: Set(false),
    };
    let inserted = new_message.insert(&txn).await?;

    let mut saved_attachments = Vec::with_capacity(attachments.len());
    for attachment in attachments {
        let saved = entity::message_attachment::ActiveModel {
            id: Set(attachment.id),
            message_id: Set(inserted.id),
            position: Set(attachment.position),
            content_type: Set(attachment.content_type),
            byte_size: Set(attachment.byte_size),
            width: Set(attachment.width),
            height: Set(attachment.height),
            storage_path: Set(attachment.storage_path),
            thumbnail_content_type: Set(attachment.thumbnail_content_type),
            thumbnail_byte_size: Set(attachment.thumbnail_byte_size),
            thumbnail_width: Set(attachment.thumbnail_width),
            thumbnail_height: Set(attachment.thumbnail_height),
            thumbnail_storage_path: Set(attachment.thumbnail_storage_path),
            created_at: Set(now_unix_micros()),
        }
        .insert(&txn)
        .await?;
        saved_attachments.push(AttachmentResponse::from(saved));
    }
    txn.commit().await?;

    Ok((inserted, saved_attachments))
}

fn created_message_response(
    inserted: &entity::message::Model,
    user: &AuthUser,
    attachments: Vec<AttachmentResponse>,
) -> MessageResponse {
    MessageResponse {
        id: inserted.id,
        user_id: inserted.user_id,
        channel_id: inserted.channel_id,
        parent_id: inserted.parent_id,
        created_at: inserted.created_at,
        deleted_at: inserted.deleted_at,
        text: inserted.text.clone(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        avatar_url: avatar_url(user.avatar_path.as_deref(), user.avatar_updated_at),
        suppress_embeds: inserted.suppress_embeds,
        attachments,
        embeds: Vec::new(),
        reactions: Vec::new(),
        thread_summary: None,
    }
}

async fn publish_created_message(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    embed_fetcher: web::Data<EmbedFetcher>,
    inserted: entity::message::Model,
    resp: MessageResponse,
) -> Result<web::Json<MessageResponse>, AppError> {
    if let Some(root_message_id) = inserted.parent_id {
        let thread_summary = load_thread_summaries_for_roots(db.get_ref(), &[root_message_id])
            .await?
            .remove(&root_message_id)
            .unwrap_or(ThreadSummary {
                reply_count: 1,
                last_reply_created_at: inserted.created_at,
            });
        broadcaster
            .publish(&BroadcastEvent::ThreadReplyCreated(
                ThreadReplyCreatedEvent {
                    channel_id: inserted.channel_id,
                    root_message_id,
                    reply: resp.clone(),
                    thread_summary,
                },
            ))
            .await?;
    } else {
        broadcaster
            .publish(&BroadcastEvent::Message(resp.clone()))
            .await?;
    }

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

#[get("/threads/participated")]
async fn get_participated_threads(
    db: web::Data<DatabaseConnection>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let reply_rows = entity::message::Entity::find()
        .filter(entity::message::Column::ParentId.is_not_null())
        .filter(entity::message::Column::DeletedAt.is_null())
        .order_by_asc(entity::message::Column::ParentId)
        .order_by_asc(entity::message::Column::CreatedAt)
        .order_by_asc(entity::message::Column::Id)
        .find_also_related(entity::user::Entity)
        .all(db.get_ref())
        .await?;

    let mut replies_by_root: HashMap<
        i64,
        Vec<(entity::message::Model, Option<entity::user::Model>)>,
    > = HashMap::new();
    for (reply, reply_user) in reply_rows {
        if let Some(root_id) = reply.parent_id {
            replies_by_root
                .entry(root_id)
                .or_default()
                .push((reply, reply_user));
        }
    }

    if replies_by_root.is_empty() {
        return Ok(web::Json(Vec::<ParticipatedThreadPreview>::new()));
    }

    let root_ids: Vec<i64> = replies_by_root.keys().copied().collect();
    let root_rows = entity::message::Entity::find()
        .filter(entity::message::Column::Id.is_in(root_ids))
        .filter(entity::message::Column::ParentId.is_null())
        .find_also_related(entity::user::Entity)
        .all(db.get_ref())
        .await?;

    let channel_ids: Vec<i64> = root_rows.iter().map(|(root, _)| root.channel_id).collect();
    let channels = entity::channel::Entity::find()
        .filter(entity::channel::Column::Id.is_in(channel_ids))
        .all(db.get_ref())
        .await?;
    let channels_by_id: HashMap<i64, entity::channel::Model> = channels
        .into_iter()
        .map(|channel| (channel.id, channel))
        .collect();

    let mut preview_message_ids = Vec::new();
    for (root, _) in &root_rows {
        preview_message_ids.push(root.id);
        if let Some(replies) = replies_by_root.get(&root.id) {
            let start = replies.len().saturating_sub(3);
            preview_message_ids.extend(replies[start..].iter().map(|(reply, _)| reply.id));
        }
    }
    let embeds_by_message = load_embeds_for_messages(db.get_ref(), &preview_message_ids).await?;
    let attachments_by_message =
        load_attachments_for_messages(db.get_ref(), &preview_message_ids).await?;
    let reactions_by_message =
        load_reaction_summaries(db.get_ref(), &preview_message_ids, user.id).await?;

    let mut previews_with_sort_key = Vec::new();
    for (root, root_user) in root_rows {
        let Some(channel) = channels_by_id.get(&root.channel_id) else {
            continue;
        };
        if channel.channel_type != CHANNEL_TYPE_TEXT {
            continue;
        }

        let Some(replies) = replies_by_root.get(&root.id) else {
            continue;
        };
        if replies.is_empty() {
            continue;
        }

        let participated =
            root.user_id == user.id || replies.iter().any(|(reply, _)| reply.user_id == user.id);
        if !participated {
            continue;
        }

        let Some((last_reply, _)) = replies.last() else {
            continue;
        };
        let recent_start = replies.len().saturating_sub(3);
        let recent_replies = replies[recent_start..]
            .iter()
            .map(|(reply, reply_user)| {
                let embeds = embeds_by_message
                    .get(&reply.id)
                    .cloned()
                    .unwrap_or_default();
                let attachments = attachments_by_message
                    .get(&reply.id)
                    .cloned()
                    .unwrap_or_default();
                let reactions = reactions_by_message
                    .get(&reply.id)
                    .cloned()
                    .unwrap_or_default();
                message_response_from_model(
                    reply.clone(),
                    reply_user.clone(),
                    embeds,
                    attachments,
                    reactions,
                )
            })
            .collect();
        let root_embeds = embeds_by_message.get(&root.id).cloned().unwrap_or_default();
        let root_attachments = attachments_by_message
            .get(&root.id)
            .cloned()
            .unwrap_or_default();
        let root_reactions = reactions_by_message
            .get(&root.id)
            .cloned()
            .unwrap_or_default();
        let preview = ParticipatedThreadPreview {
            channel: ChannelResponse::from(channel.clone()),
            root: message_response_from_model(
                root,
                root_user,
                root_embeds,
                root_attachments,
                root_reactions,
            ),
            reply_count: replies.len() as u64,
            last_reply_created_at: last_reply.created_at,
            recent_replies,
        };
        previews_with_sort_key.push((preview, last_reply.id));
    }

    previews_with_sort_key.sort_by(|(left, left_last_id), (right, right_last_id)| {
        right
            .last_reply_created_at
            .cmp(&left.last_reply_created_at)
            .then_with(|| right_last_id.cmp(left_last_id))
            .then_with(|| right.root.id.cmp(&left.root.id))
    });

    let previews: Vec<ParticipatedThreadPreview> = previews_with_sort_key
        .into_iter()
        .map(|(preview, _)| preview)
        .collect();
    Ok(web::Json(previews))
}

#[get("/thread/{root_message_id}")]
async fn get_thread(
    db: web::Data<DatabaseConnection>,
    path: web::Path<i64>,
    query: web::Query<ThreadPageQuery>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let root_message_id = path.into_inner();
    let root = validated_thread_root(db.get_ref(), root_message_id).await?;
    let reply_limit = query
        .limit
        .unwrap_or(DEFAULT_THREAD_REPLY_LIMIT)
        .clamp(1, MAX_THREAD_REPLY_LIMIT);

    let before_cursor = match (query.before_created_at, query.before_id) {
        (Some(created_at), Some(id)) => Some((created_at, id)),
        (None, None) => None,
        _ => return Err(AppError::InvalidRequest),
    };

    let mut reply_query = entity::message::Entity::find()
        .filter(entity::message::Column::ParentId.eq(root_message_id))
        .find_also_related(entity::user::Entity);

    if let Some((before_created_at, before_id)) = before_cursor {
        reply_query = reply_query.filter(
            Condition::any()
                .add(entity::message::Column::CreatedAt.lt(before_created_at))
                .add(
                    Condition::all()
                        .add(entity::message::Column::CreatedAt.eq(before_created_at))
                        .add(entity::message::Column::Id.lt(before_id)),
                ),
        );
    }

    let mut rows = reply_query
        .order_by_desc(entity::message::Column::CreatedAt)
        .order_by_desc(entity::message::Column::Id)
        .limit(reply_limit + 1)
        .all(db.get_ref())
        .await?;
    let has_more_replies = rows.len() > reply_limit as usize;
    rows.truncate(reply_limit as usize);
    rows.reverse();

    let mut message_ids: Vec<i64> = rows.iter().map(|(m, _)| m.id).collect();
    message_ids.push(root.id);
    let embeds_by_message = load_embeds_for_messages(db.get_ref(), &message_ids).await?;
    let attachments_by_message = load_attachments_for_messages(db.get_ref(), &message_ids).await?;
    let reactions_by_message = load_reaction_summaries(db.get_ref(), &message_ids, user.id).await?;

    let root_user = entity::user::Entity::find_by_id(root.user_id)
        .one(db.get_ref())
        .await?;
    let root_response = message_response_from_model(
        root,
        root_user,
        embeds_by_message
            .get(&root_message_id)
            .cloned()
            .unwrap_or_default(),
        attachments_by_message
            .get(&root_message_id)
            .cloned()
            .unwrap_or_default(),
        reactions_by_message
            .get(&root_message_id)
            .cloned()
            .unwrap_or_default(),
    );
    let replies = rows
        .into_iter()
        .map(|(m, u)| {
            let embeds = embeds_by_message.get(&m.id).cloned().unwrap_or_default();
            let attachments = attachments_by_message
                .get(&m.id)
                .cloned()
                .unwrap_or_default();
            let reactions = reactions_by_message.get(&m.id).cloned().unwrap_or_default();
            message_response_from_model(m, u, embeds, attachments, reactions)
        })
        .collect();

    Ok(web::Json(ThreadResponse {
        root: root_response,
        replies,
        has_more_replies,
    }))
}

#[post("/thread/{root_message_id}/reply", guard = "message_create_is_json")]
async fn create_thread_reply_json(
    db: web::Data<DatabaseConnection>,
    embed_fetcher: web::Data<EmbedFetcher>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
    body: web::Json<SendMessageRequest>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let root_message_id = path.into_inner();
    let message_id = generate_id();
    let (inserted, attachments) = insert_message_rows(
        db.get_ref(),
        message_id,
        user.id,
        MessageCreateTarget::ThreadReply { root_message_id },
        body.text.clone(),
        Vec::new(),
    )
    .await?;
    let resp = created_message_response(&inserted, &user, attachments);
    publish_created_message(db, broadcaster, embed_fetcher, inserted, resp).await
}

#[post(
    "/thread/{root_message_id}/reply",
    guard = "message_create_is_multipart"
)]
async fn create_thread_reply_multipart(
    db: web::Data<DatabaseConnection>,
    storage: web::Data<AttachmentStorage>,
    embed_fetcher: web::Data<EmbedFetcher>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
    payload: Multipart,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let root_message_id = path.into_inner();
    create_multipart_message(
        db,
        storage,
        broadcaster,
        embed_fetcher,
        MessageCreateTarget::ThreadReply { root_message_id },
        payload,
        user,
    )
    .await
}

#[post("/thread/{root_message_id}/reply")]
async fn create_thread_reply_unsupported_content_type() -> Result<HttpResponse, AppError> {
    Err(AppError::InvalidRequest)
}

async fn validated_thread_root(
    db: &DatabaseConnection,
    root_message_id: i64,
) -> Result<entity::message::Model, AppError> {
    let root = entity::message::Entity::find_by_id(root_message_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    if root.parent_id.is_some() {
        return Err(AppError::InvalidRequest);
    }

    let channel = entity::channel::Entity::find_by_id(root.channel_id)
        .one(db)
        .await?
        .ok_or(AppError::NoChannelFound)?;
    if channel.channel_type != CHANNEL_TYPE_TEXT {
        return Err(AppError::InvalidRequest);
    }

    Ok(root)
}

fn message_response_from_model(
    message: entity::message::Model,
    user: Option<entity::user::Model>,
    embeds: Vec<EmbedResponse>,
    attachments: Vec<AttachmentResponse>,
    reactions: Vec<ReactionSummary>,
) -> MessageResponse {
    let is_deleted = message.deleted_at.is_some();
    let (username, display_name, avatar_url) = match user {
        Some(u) => (
            u.username,
            u.display_name,
            avatar_url(u.avatar_path.as_deref(), u.avatar_updated_at),
        ),
        None => ("[deleted]".into(), None, None),
    };
    MessageResponse {
        id: message.id,
        user_id: message.user_id,
        channel_id: message.channel_id,
        parent_id: message.parent_id,
        created_at: message.created_at,
        deleted_at: message.deleted_at,
        text: message.text,
        username,
        display_name,
        avatar_url,
        suppress_embeds: message.suppress_embeds,
        attachments: if is_deleted { Vec::new() } else { attachments },
        embeds: if is_deleted { Vec::new() } else { embeds },
        reactions: if is_deleted { Vec::new() } else { reactions },
        thread_summary: None,
    }
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
    if existing.deleted_at.is_some() {
        return Err(AppError::NotFound);
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
    let existing_attachments = load_attachments_for_messages(db.get_ref(), &[updated.id])
        .await?
        .remove(&updated.id)
        .unwrap_or_default();
    let thread_summary = if updated.parent_id.is_none() {
        load_thread_summaries_for_roots(db.get_ref(), &[updated.id])
            .await?
            .remove(&updated.id)
    } else {
        None
    };
    let reactions = load_reaction_summaries(db.get_ref(), &[updated.id], user.id)
        .await?
        .remove(&updated.id)
        .unwrap_or_default();

    let resp = MessageResponse {
        id: updated.id,
        user_id: updated.user_id,
        channel_id,
        parent_id: updated.parent_id,
        created_at: updated.created_at,
        deleted_at: updated.deleted_at,
        text: updated.text.clone(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        avatar_url: avatar_url(user.avatar_path.as_deref(), user.avatar_updated_at),
        suppress_embeds: updated.suppress_embeds,
        attachments: existing_attachments,
        embeds: existing_embeds,
        reactions,
        thread_summary,
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
    if existing.deleted_at.is_some() {
        return Err(AppError::NotFound);
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

#[post("/message/{message_id}/reactions")]
async fn add_message_reaction(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
    body: web::Json<ReactionRequest>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let message_id = path.into_inner();
    let reactions = add_reaction(db.get_ref(), message_id, user.id, &body).await?;
    let message = entity::message::Entity::find_by_id(message_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound)?;

    broadcaster
        .publish(&BroadcastEvent::MessageReactionsUpdated(
            MessageReactionsUpdatedEvent {
                id: message_id,
                channel_id: message.channel_id,
                parent_id: message.parent_id,
                root_message_id: message.parent_id.unwrap_or(message.id),
                user_id: user.id,
                reactions: reactions.clone(),
            },
        ))
        .await?;

    Ok(web::Json(reactions))
}

#[delete("/message/{message_id}/reactions")]
async fn remove_message_reaction(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
    body: web::Json<ReactionRequest>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let message_id = path.into_inner();
    let reactions = remove_reaction(db.get_ref(), message_id, user.id, &body).await?;
    let message = entity::message::Entity::find_by_id(message_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound)?;

    broadcaster
        .publish(&BroadcastEvent::MessageReactionsUpdated(
            MessageReactionsUpdatedEvent {
                id: message_id,
                channel_id: message.channel_id,
                parent_id: message.parent_id,
                root_message_id: message.parent_id.unwrap_or(message.id),
                user_id: user.id,
                reactions: reactions.clone(),
            },
        ))
        .await?;

    Ok(web::Json(reactions))
}

#[delete("/message/{message_id}")]
async fn delete_message(
    db: web::Data<DatabaseConnection>,
    storage: Option<web::Data<AttachmentStorage>>,
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
    if existing.deleted_at.is_some() {
        return Err(AppError::NotFound);
    }

    let channel_id = existing.channel_id;
    let parent_id = existing.parent_id;
    let root_summary = if parent_id.is_none() {
        load_thread_summaries_for_roots(db.get_ref(), &[message_id])
            .await?
            .remove(&message_id)
    } else {
        None
    };
    let attachment_rows = entity::message_attachment::Entity::find()
        .filter(entity::message_attachment::Column::MessageId.eq(message_id))
        .all(db.get_ref())
        .await?;
    let cleanup_paths = stored_attachment_paths(&attachment_rows);

    let txn = db.begin().await?;
    entity::embed::Entity::delete_many()
        .filter(entity::embed::Column::MessageId.eq(message_id))
        .exec(&txn)
        .await?;
    entity::message_reaction::Entity::delete_many()
        .filter(entity::message_reaction::Column::MessageId.eq(message_id))
        .exec(&txn)
        .await?;
    entity::message_attachment::Entity::delete_many()
        .filter(entity::message_attachment::Column::MessageId.eq(message_id))
        .exec(&txn)
        .await?;

    if parent_id.is_none() && root_summary.is_some() {
        let mut active: entity::message::ActiveModel = existing.into();
        active.text = Set(String::new());
        active.deleted_at = Set(Some(now_unix_micros()));
        active.suppress_embeds = Set(true);
        let updated = active.update(&txn).await?;
        txn.commit().await?;
        cleanup_deleted_attachment_files(
            storage.as_ref().map(web::Data::get_ref),
            message_id,
            &cleanup_paths,
        )
        .await;

        broadcaster
            .publish(&BroadcastEvent::MessageUpdated(MessageResponse {
                id: updated.id,
                user_id: updated.user_id,
                channel_id,
                parent_id: updated.parent_id,
                created_at: updated.created_at,
                deleted_at: updated.deleted_at,
                text: updated.text,
                username: user.username.clone(),
                display_name: user.display_name.clone(),
                avatar_url: avatar_url(user.avatar_path.as_deref(), user.avatar_updated_at),
                suppress_embeds: updated.suppress_embeds,
                attachments: Vec::new(),
                embeds: Vec::new(),
                reactions: Vec::new(),
                thread_summary: root_summary,
            }))
            .await?;
    } else {
        let active: entity::message::ActiveModel = existing.into();
        active.delete(&txn).await?;
        txn.commit().await?;
        cleanup_deleted_attachment_files(
            storage.as_ref().map(web::Data::get_ref),
            message_id,
            &cleanup_paths,
        )
        .await;

        if let Some(root_message_id) = parent_id {
            let thread_summary = load_thread_summaries_for_roots(db.get_ref(), &[root_message_id])
                .await?
                .remove(&root_message_id);
            broadcaster
                .publish(&BroadcastEvent::ThreadReplyDeleted(
                    ThreadReplyDeletedEvent {
                        channel_id,
                        root_message_id,
                        reply_id: message_id,
                        thread_summary,
                    },
                ))
                .await?;
        } else {
            broadcaster
                .publish(&BroadcastEvent::MessageDeleted(MessageDeletedEvent {
                    id: message_id,
                    channel_id,
                }))
                .await?;
        }
    }

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
    if msg.deleted_at.is_some() {
        return Ok(());
    }

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
        .service(create_message_json)
        .service(create_message_multipart)
        .service(create_message_unsupported_content_type)
        .service(get_participated_threads)
        .service(get_thread)
        .service(create_thread_reply_json)
        .service(create_thread_reply_multipart)
        .service(create_thread_reply_unsupported_content_type)
        .service(update_message)
        .service(add_message_reaction)
        .service(remove_message_reaction)
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
            emoji_storage: web::Data::new(crate::api::emoji::EmojiStorage {
                dir: std::env::temp_dir(),
            }),
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
        assert!(event_str.contains("\\\"attachments\\\":[]"));
    }

    #[actix_web::test]
    async fn test_message_reaction_add_broadcasts_update_event_to_clients() {
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
            parent_id: Set(None),
            created_at: Set(now_unix_micros()),
            deleted_at: Set(None),
            text: Set("react here".into()),
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
        let req = test::TestRequest::post()
            .uri(&format!("/message/{msg_id}/reactions"))
            .insert_header(ContentType::json())
            .insert_header((name, value))
            .set_payload(serde_json::json!({"kind": "native", "emoji": "👍"}).to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — broadcast was never sent")
            .expect("channel closed");

        let event_str = format!("{:?}", event);
        assert!(event_str.contains("kind\\\":\\\"message_reactions_updated\\\""));
        assert!(event_str.contains(&format!("\\\"id\\\":{}", msg_id)));
        assert!(event_str.contains(&format!("\\\"channel_id\\\":{}", chan_id)));
        assert!(event_str.contains(&format!("\\\"root_message_id\\\":{}", msg_id)));
        assert!(event_str.contains("\\\"parent_id\\\":null"));
        assert!(event_str.contains(&format!("\\\"user_id\\\":{}", user.id)));
        assert!(event_str.contains("👍"));
        assert!(event_str.contains("\\\"count\\\":1"));
        assert!(event_str.contains("\\\"me_reacted\\\":true"));
    }

    #[actix_web::test]
    async fn test_thread_reply_reaction_broadcast_includes_thread_context() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let root_id = generate_id();
        entity::message::ActiveModel {
            id: Set(root_id),
            user_id: Set(user.id),
            channel_id: Set(chan_id),
            parent_id: Set(None),
            created_at: Set(now_unix_micros()),
            deleted_at: Set(None),
            text: Set("root".into()),
            suppress_embeds: Set(false),
        }
        .insert(&db)
        .await
        .unwrap();
        let reply_id = generate_id();
        entity::message::ActiveModel {
            id: Set(reply_id),
            user_id: Set(user.id),
            channel_id: Set(chan_id),
            parent_id: Set(Some(root_id)),
            created_at: Set(now_unix_micros()),
            deleted_at: Set(None),
            text: Set("reply".into()),
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
        let req = test::TestRequest::post()
            .uri(&format!("/message/{reply_id}/reactions"))
            .insert_header(ContentType::json())
            .insert_header((name, value))
            .set_payload(serde_json::json!({"kind": "native", "emoji": "👍"}).to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — broadcast was never sent")
            .expect("channel closed");

        let event_str = format!("{:?}", event);
        assert!(event_str.contains("kind\\\":\\\"message_reactions_updated\\\""));
        assert!(event_str.contains(&format!("\\\"id\\\":{}", reply_id)));
        assert!(event_str.contains(&format!("\\\"channel_id\\\":{}", chan_id)));
        assert!(event_str.contains(&format!("\\\"parent_id\\\":{}", root_id)));
        assert!(event_str.contains(&format!("\\\"root_message_id\\\":{}", root_id)));
        assert!(event_str.contains("👍"));
    }

    #[actix_web::test]
    async fn test_message_reaction_remove_broadcasts_update_event_to_clients() {
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
            parent_id: Set(None),
            created_at: Set(now_unix_micros()),
            deleted_at: Set(None),
            text: Set("react here".into()),
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
        let add_req = test::TestRequest::post()
            .uri(&format!("/message/{msg_id}/reactions"))
            .insert_header(ContentType::json())
            .insert_header((name.clone(), value.clone()))
            .set_payload(serde_json::json!({"kind": "native", "emoji": "👍"}).to_string())
            .to_request();
        let add_resp = test::call_service(&app, add_req).await;
        assert!(add_resp.status().is_success());
        let _ = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — add broadcast was never sent")
            .expect("channel closed");

        let remove_req = test::TestRequest::delete()
            .uri(&format!("/message/{msg_id}/reactions"))
            .insert_header(ContentType::json())
            .insert_header((name, value))
            .set_payload(serde_json::json!({"kind": "native", "emoji": "👍"}).to_string())
            .to_request();
        let remove_resp = test::call_service(&app, remove_req).await;
        assert!(remove_resp.status().is_success());

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — remove broadcast was never sent")
            .expect("channel closed");

        let event_str = format!("{:?}", event);
        assert!(event_str.contains("kind\\\":\\\"message_reactions_updated\\\""));
        assert!(event_str.contains(&format!("\\\"id\\\":{}", msg_id)));
        assert!(event_str.contains(&format!("\\\"channel_id\\\":{}", chan_id)));
        assert!(event_str.contains(&format!("\\\"root_message_id\\\":{}", msg_id)));
        assert!(event_str.contains("\\\"parent_id\\\":null"));
        assert!(event_str.contains(&format!("\\\"user_id\\\":{}", user.id)));
        assert!(event_str.contains("\\\"reactions\\\":[]"));
    }

    #[actix_web::test]
    async fn test_thread_reply_create_broadcasts_to_clients() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let root_id = generate_id();
        entity::message::ActiveModel {
            id: Set(root_id),
            user_id: Set(user.id),
            channel_id: Set(chan_id),
            parent_id: Set(None),
            created_at: Set(now_unix_micros()),
            deleted_at: Set(None),
            text: Set("root".into()),
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
        let req = test::TestRequest::post()
            .uri(&format!("/thread/{root_id}/reply"))
            .insert_header(ContentType::json())
            .insert_header((name, value))
            .set_payload(
                serde_json::to_string(&SendMessageRequest {
                    text: "reply over sse".into(),
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
        assert!(event_str.contains("kind\\\":\\\"thread_reply_created\\\""));
        assert!(event_str.contains("reply over sse"));
        assert!(event_str.contains("\\\"attachments\\\":[]"));
        assert!(event_str.contains(&format!("\\\"channel_id\\\":{}", chan_id)));
        assert!(event_str.contains(&format!("\\\"root_message_id\\\":{}", root_id)));
        assert!(event_str.contains(&format!("\\\"parent_id\\\":{}", root_id)));
        assert!(event_str.contains("\\\"reply_count\\\":1"));
    }

    #[actix_web::test]
    async fn test_thread_reply_delete_broadcasts_summary_recalculation() {
        let (db, chan_id) = setup_db().await;
        let user = auth::register_user(&db, "alice", "hunter2", None)
            .await
            .unwrap();
        let session = auth::create_session(&db, user.id).await.unwrap();

        let root_id = generate_id();
        entity::message::ActiveModel {
            id: Set(root_id),
            user_id: Set(user.id),
            channel_id: Set(chan_id),
            parent_id: Set(None),
            created_at: Set(now_unix_micros()),
            deleted_at: Set(None),
            text: Set("root".into()),
            suppress_embeds: Set(false),
        }
        .insert(&db)
        .await
        .unwrap();
        let reply_id = generate_id();
        entity::message::ActiveModel {
            id: Set(reply_id),
            user_id: Set(user.id),
            channel_id: Set(chan_id),
            parent_id: Set(Some(root_id)),
            created_at: Set(now_unix_micros()),
            deleted_at: Set(None),
            text: Set("reply".into()),
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
            .uri(&format!("/message/{reply_id}"))
            .insert_header((name, value))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out — broadcast was never sent")
            .expect("channel closed");

        let event_str = format!("{:?}", event);
        assert!(event_str.contains("kind\\\":\\\"thread_reply_deleted\\\""));
        assert!(event_str.contains(&format!("\\\"root_message_id\\\":{}", root_id)));
        assert!(event_str.contains(&format!("\\\"reply_id\\\":{}", reply_id)));
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
            parent_id: Set(None),
            created_at: Set(now_unix_micros()),
            deleted_at: Set(None),
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
