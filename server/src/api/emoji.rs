use std::io::Cursor;
use std::path::PathBuf;

use actix_multipart::Multipart;
use actix_web::{HttpResponse, Responder, delete, get, patch, post, web};
use futures_util::TryStreamExt;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, Set,
};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::broadcast::{BroadcastEvent, Broadcaster};
use crate::entity;
use crate::error::AppError;
use crate::util::{generate_id, now_unix_secs};

pub const EMOJIS_SUBDIR: &str = "emojis";
const EMOJI_MAX_BYTES: usize = 2 * 1024 * 1024;
const EMOJI_IMAGE_SIZE: u32 = 256;

/// Where emoji files are written on disk. Registered as `web::Data` by
/// `start_server` (and by tests that exercise `/emojis`).
#[derive(Clone, Debug)]
pub struct EmojiStorage {
    pub dir: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
pub struct EmojiResponse {
    pub id: i64,
    pub name: String,
    pub image_url: String,
    pub animated: bool,
    pub created_by_user_id: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RenameEmojiRequest {
    name: String,
}

pub fn emoji_url(image_path: &str, updated_at: i64) -> String {
    format!("/uploads/{image_path}?v={updated_at}")
}

impl From<entity::emoji::Model> for EmojiResponse {
    fn from(value: entity::emoji::Model) -> Self {
        Self {
            id: value.id,
            name: value.name,
            image_url: emoji_url(&value.image_path, value.updated_at),
            animated: value.animated,
            created_by_user_id: value.created_by_user_id,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
        }
    }
}

fn validate_emoji_name(raw: &str) -> Result<(String, String), AppError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(AppError::EmojiNameRequired);
    }

    let len = name.chars().count();
    let valid_chars = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !(2..=32).contains(&len) || !valid_chars {
        return Err(AppError::InvalidEmojiName);
    }

    Ok((name.to_owned(), name.to_ascii_lowercase()))
}

enum PreparedEmojiUpload {
    StaticWebp(Vec<u8>),
    Animated {
        bytes: Vec<u8>,
        extension: &'static str,
    },
}

fn has_png_signature(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

fn has_jpeg_signature(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff
}

fn has_gif_signature(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}

fn has_webp_signature(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
}

fn is_animated_webp(bytes: &[u8]) -> bool {
    if !has_webp_signature(bytes) {
        return false;
    }

    let mut offset = 12usize;
    while offset + 8 <= bytes.len() {
        let chunk = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        offset += 8;

        if chunk == b"VP8X" && size > 0 && offset < bytes.len() {
            // WebP extended header bit 1 marks animation.
            return bytes[offset] & 0b0000_0010 != 0;
        }

        let padded = size.saturating_add(size % 2);
        offset = offset.saturating_add(padded);
    }

    false
}

fn skip_gif_sub_blocks(bytes: &[u8], offset: &mut usize) -> bool {
    while *offset < bytes.len() {
        let size = bytes[*offset] as usize;
        *offset += 1;
        if size == 0 {
            return true;
        }
        if (*offset).saturating_add(size) > bytes.len() {
            return false;
        }
        *offset += size;
    }
    false
}

fn animated_gif_frame_count(bytes: &[u8]) -> Option<usize> {
    if !has_gif_signature(bytes) || bytes.len() < 13 {
        return None;
    }

    let packed = bytes[10];
    let global_color_table_len = if packed & 0b1000_0000 != 0 {
        3usize.saturating_mul(1usize << ((packed & 0b0000_0111) + 1))
    } else {
        0
    };
    let mut offset = 13usize.saturating_add(global_color_table_len);
    let mut frames = 0usize;

    while offset < bytes.len() {
        match bytes[offset] {
            0x3b => return Some(frames),
            0x21 => {
                offset = offset.saturating_add(2);
                if !skip_gif_sub_blocks(bytes, &mut offset) {
                    return None;
                }
            }
            0x2c => {
                frames = frames.saturating_add(1);
                offset = offset.saturating_add(10);
                if offset > bytes.len() {
                    return None;
                }
                let image_packed = bytes[offset - 1];
                if image_packed & 0b1000_0000 != 0 {
                    let local_color_table_len =
                        3usize.saturating_mul(1usize << ((image_packed & 0b0000_0111) + 1));
                    offset = offset.saturating_add(local_color_table_len);
                }
                // LZW minimum code size.
                offset = offset.saturating_add(1);
                if !skip_gif_sub_blocks(bytes, &mut offset) {
                    return None;
                }
            }
            _ => return None,
        }
    }

    None
}

fn is_animated_gif(bytes: &[u8]) -> bool {
    animated_gif_frame_count(bytes).is_some_and(|frames| frames > 1)
}

async fn read_text_field(mut field: actix_multipart::Field) -> Result<String, AppError> {
    let mut buf = Vec::new();
    while let Some(chunk) = field
        .try_next()
        .await
        .map_err(|_| AppError::InvalidRequest)?
    {
        if buf.len() + chunk.len() > 1024 {
            return Err(AppError::InvalidEmojiName);
        }
        buf.extend_from_slice(&chunk);
    }
    String::from_utf8(buf).map_err(|_| AppError::InvalidEmojiName)
}

async fn read_file_field(mut field: actix_multipart::Field) -> Result<Vec<u8>, AppError> {
    let mut buf = Vec::new();
    while let Some(chunk) = field
        .try_next()
        .await
        .map_err(|_| AppError::InvalidRequest)?
    {
        if buf.len() + chunk.len() > EMOJI_MAX_BYTES {
            return Err(AppError::PayloadTooLarge);
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

fn prepare_emoji_upload(
    content_type: &str,
    bytes: Vec<u8>,
) -> Result<PreparedEmojiUpload, AppError> {
    match content_type {
        "image/png" if has_png_signature(&bytes) => Ok(PreparedEmojiUpload::StaticWebp(
            normalize_static_image(&bytes)?,
        )),
        "image/jpeg" if has_jpeg_signature(&bytes) => Ok(PreparedEmojiUpload::StaticWebp(
            normalize_static_image(&bytes)?,
        )),
        "image/webp" if has_webp_signature(&bytes) => {
            if is_animated_webp(&bytes) {
                Ok(PreparedEmojiUpload::Animated {
                    bytes,
                    extension: "webp",
                })
            } else {
                Ok(PreparedEmojiUpload::StaticWebp(normalize_static_image(
                    &bytes,
                )?))
            }
        }
        "image/gif" if has_gif_signature(&bytes) && is_animated_gif(&bytes) => {
            Ok(PreparedEmojiUpload::Animated {
                bytes,
                extension: "gif",
            })
        }
        _ => Err(AppError::UnsupportedEmojiFile),
    }
}

fn normalize_static_image(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    let img = image::load_from_memory(bytes).map_err(|_| AppError::UnsupportedEmojiFile)?;
    let (w, h) = (img.width(), img.height());
    let side = w.min(h);
    if side == 0 {
        return Err(AppError::UnsupportedEmojiFile);
    }
    let x = (w - side) / 2;
    let y = (h - side) / 2;
    let cropped = img.crop_imm(x, y, side, side);
    let resized = cropped.resize_exact(
        EMOJI_IMAGE_SIZE,
        EMOJI_IMAGE_SIZE,
        image::imageops::FilterType::Lanczos3,
    );

    let mut out = Cursor::new(Vec::new());
    resized
        .write_to(&mut out, image::ImageFormat::WebP)
        .map_err(|e| AppError::Internal(format!("encode emoji webp: {e}")))?;
    Ok(out.into_inner())
}

#[get("/emojis")]
async fn list_emojis(db: web::Data<DatabaseConnection>) -> Result<impl Responder, AppError> {
    let emojis = entity::emoji::Entity::find()
        // SQLite sorts NULL before non-NULL in ascending order, which keeps
        // active emojis before soft-deleted emojis.
        .order_by_asc(entity::emoji::Column::DeletedAt)
        .order_by_asc(entity::emoji::Column::NormalizedName)
        .all(db.get_ref())
        .await?
        .into_iter()
        .map(EmojiResponse::from)
        .collect::<Vec<_>>();

    Ok(web::Json(emojis))
}

#[post("/emojis")]
async fn create_emoji(
    db: web::Data<DatabaseConnection>,
    storage: web::Data<EmojiStorage>,
    broadcaster: web::Data<Broadcaster>,
    user: AuthUser,
    mut payload: Multipart,
) -> Result<impl Responder, AppError> {
    let mut name: Option<String> = None;
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;

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
            "name" => name = Some(read_text_field(field).await?),
            "file" => {
                content_type = field.content_type().map(|m| m.essence_str().to_owned());
                file_bytes = Some(read_file_field(field).await?);
            }
            _ => {}
        }
    }

    let (name, normalized_name) = validate_emoji_name(&name.ok_or(AppError::EmojiNameRequired)?)?;
    let bytes = file_bytes.ok_or(AppError::EmojiFileRequired)?;
    if bytes.is_empty() {
        return Err(AppError::EmojiFileRequired);
    }

    let content_type = content_type.unwrap_or_default();
    let prepared = prepare_emoji_upload(&content_type, bytes)?;

    let duplicate = entity::emoji::Entity::find()
        .filter(entity::emoji::Column::NormalizedName.eq(&normalized_name))
        .filter(entity::emoji::Column::DeletedAt.is_null())
        .one(db.get_ref())
        .await?;
    if duplicate.is_some() {
        return Err(AppError::EmojiNameTaken);
    }

    let emoji_id = generate_id();
    let (asset_bytes, extension, animated) = match prepared {
        PreparedEmojiUpload::StaticWebp(bytes) => (bytes, "webp", false),
        PreparedEmojiUpload::Animated { bytes, extension } => (bytes, extension, true),
    };
    let filename = format!("{emoji_id}.{extension}");
    let rel_path = format!("{EMOJIS_SUBDIR}/{filename}");
    let emojis_dir = storage.dir.join(EMOJIS_SUBDIR);
    std::fs::create_dir_all(&emojis_dir)?;
    let final_path = emojis_dir.join(&filename);
    let tmp_path = emojis_dir.join(format!("{filename}.tmp"));
    std::fs::write(&tmp_path, &asset_bytes)?;
    std::fs::rename(&tmp_path, &final_path)?;

    let now = now_unix_secs();
    let emoji = entity::emoji::ActiveModel {
        id: Set(emoji_id),
        image_path: Set(rel_path),
        name: Set(name),
        normalized_name: Set(normalized_name),
        animated: Set(animated),
        created_by_user_id: Set(user.id),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
    };

    let inserted = match emoji.insert(db.get_ref()).await {
        Ok(inserted) => inserted,
        Err(err) => {
            let _ = std::fs::remove_file(&final_path);
            return Err(AppError::Db(err));
        }
    };

    let response = EmojiResponse::from(inserted);
    broadcaster
        .publish(&BroadcastEvent::EmojiCreated(response.clone()))
        .await?;

    Ok(HttpResponse::Created().json(response))
}

#[patch("/emojis/{emoji_id}")]
async fn rename_emoji(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
    body: web::Json<RenameEmojiRequest>,
) -> Result<impl Responder, AppError> {
    let emoji_id = path.into_inner();
    let (name, normalized_name) = validate_emoji_name(&body.name)?;
    let existing = entity::emoji::Entity::find_by_id(emoji_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound)?;

    let duplicate = entity::emoji::Entity::find()
        .filter(entity::emoji::Column::NormalizedName.eq(&normalized_name))
        .filter(entity::emoji::Column::DeletedAt.is_null())
        .filter(entity::emoji::Column::Id.ne(emoji_id))
        .one(db.get_ref())
        .await?;
    if duplicate.is_some() {
        return Err(AppError::EmojiNameTaken);
    }

    let mut active = existing.into_active_model();
    active.name = Set(name);
    active.normalized_name = Set(normalized_name);
    active.updated_at = Set(now_unix_secs());
    let updated = active.update(db.get_ref()).await?;
    let response = EmojiResponse::from(updated);

    broadcaster
        .publish(&BroadcastEvent::EmojiUpdated(response.clone()))
        .await?;

    Ok(web::Json(response))
}

#[delete("/emojis/{emoji_id}")]
async fn delete_emoji(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
) -> Result<impl Responder, AppError> {
    let emoji_id = path.into_inner();
    let existing = entity::emoji::Entity::find_by_id(emoji_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound)?;

    if existing.deleted_at.is_some() {
        return Ok(web::Json(EmojiResponse::from(existing)));
    }

    let now = now_unix_secs();
    let mut active = existing.into_active_model();
    active.deleted_at = Set(Some(now));
    active.updated_at = Set(now);
    let updated = active.update(db.get_ref()).await?;
    let response = EmojiResponse::from(updated);

    broadcaster
        .publish(&BroadcastEvent::EmojiDeleted(response.clone()))
        .await?;

    Ok(web::Json(response))
}

#[post("/emojis/{emoji_id}/restore")]
async fn restore_emoji(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
) -> Result<impl Responder, AppError> {
    let emoji_id = path.into_inner();
    let existing = entity::emoji::Entity::find_by_id(emoji_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound)?;

    let duplicate = entity::emoji::Entity::find()
        .filter(entity::emoji::Column::NormalizedName.eq(&existing.normalized_name))
        .filter(entity::emoji::Column::DeletedAt.is_null())
        .filter(entity::emoji::Column::Id.ne(emoji_id))
        .one(db.get_ref())
        .await?;
    if duplicate.is_some() {
        return Err(AppError::EmojiNameTaken);
    }

    if existing.deleted_at.is_none() {
        return Ok(web::Json(EmojiResponse::from(existing)));
    }

    let mut active = existing.into_active_model();
    active.deleted_at = Set(None);
    active.updated_at = Set(now_unix_secs());
    let updated = active.update(db.get_ref()).await?;
    let response = EmojiResponse::from(updated);

    broadcaster
        .publish(&BroadcastEvent::EmojiUpdated(response.clone()))
        .await?;

    Ok(web::Json(response))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_emojis)
        .service(create_emoji)
        .service(rename_emoji)
        .service(delete_emoji)
        .service(restore_emoji);
}
