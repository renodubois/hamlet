//! Avatar upload / delete handlers + the `AvatarStorage` data type.
//!
//! Files land in `<storage.dir>/avatars/<user_id>.webp` and are served
//! from `/uploads/avatars/<user_id>.webp` by an `actix_files::Files`
//! mount registered in `crate::startup`.

use std::io::Cursor;
use std::path::PathBuf;

use actix_multipart::Multipart;
use actix_web::{Responder, delete, post, web};
use futures_util::TryStreamExt;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};

use crate::api::auth::UserResponse;
use crate::auth::AuthUser;
use crate::entity;
use crate::error::AppError;
use crate::photos::{ImageLimitError, UPLOAD_IMAGE_MAX_PIXELS, ensure_within_pixel_limit};
use crate::util::now_unix_secs;

const AVATAR_MAX_BYTES: usize = 2 * 1024 * 1024;
pub const AVATARS_SUBDIR: &str = "avatars";

/// Where avatar files are written on disk. Registered as `web::Data` by
/// `start_server` (and by tests that exercise `/me/avatar`).
#[derive(Clone, Debug)]
pub struct AvatarStorage {
    pub dir: PathBuf,
}

/// Compute the `avatar_url` field for any user payload. Returns `None`
/// when either the path or the cache-busting timestamp is missing.
pub fn avatar_url(path: Option<&str>, updated_at: Option<i64>) -> Option<String> {
    match (path, updated_at) {
        (Some(p), Some(ts)) => Some(format!("/uploads/{p}?v={ts}")),
        _ => None,
    }
}

#[post("/me/avatar")]
async fn upload_avatar(
    db: web::Data<DatabaseConnection>,
    storage: web::Data<AvatarStorage>,
    user: AuthUser,
    mut payload: Multipart,
) -> Result<impl Responder, AppError> {
    // Collect the first (and only) `file` field, bounded by AVATAR_MAX_BYTES.
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;

    while let Some(mut field) = payload
        .try_next()
        .await
        .map_err(|_| AppError::InvalidRequest)?
    {
        let is_file = field
            .content_disposition()
            .and_then(|d| d.get_name())
            .is_some_and(|n| n == "file");
        if !is_file {
            continue;
        }
        content_type = field.content_type().map(|m| m.essence_str().to_owned());

        let mut buf = Vec::new();
        while let Some(chunk) = field
            .try_next()
            .await
            .map_err(|_| AppError::InvalidRequest)?
        {
            if buf.len() + chunk.len() > AVATAR_MAX_BYTES {
                return Err(AppError::PayloadTooLarge);
            }
            buf.extend_from_slice(&chunk);
        }
        file_bytes = Some(buf);
        break;
    }

    let bytes = file_bytes.ok_or(AppError::InvalidRequest)?;
    let ct = content_type.unwrap_or_default();
    if !matches!(ct.as_str(), "image/jpeg" | "image/png" | "image/webp") {
        return Err(AppError::InvalidRequest);
    }

    // Decode, cover-crop to a square, resize to 256x256, re-encode as WebP.
    // A crafted small file can declare huge dimensions, so the pixel count is
    // capped before the full decode, and the CPU-heavy decode/resize/encode
    // runs on a blocking thread rather than stalling an async worker.
    let webp = tokio::task::spawn_blocking(move || process_avatar_image(&bytes))
        .await
        .map_err(|e| AppError::Internal(format!("avatar processing task failed: {e}")))??;

    // Atomic write: <dir>/avatars/<id>.webp.tmp -> <dir>/avatars/<id>.webp
    let avatars_dir = storage.dir.join(AVATARS_SUBDIR);
    std::fs::create_dir_all(&avatars_dir)?;
    let filename = format!("{}.webp", user.id);
    let final_path = avatars_dir.join(&filename);
    let tmp_path = avatars_dir.join(format!("{}.webp.tmp", user.id));
    std::fs::write(&tmp_path, &webp)?;
    std::fs::rename(&tmp_path, &final_path)?;

    let rel_path = format!("{AVATARS_SUBDIR}/{filename}");
    let now = now_unix_secs();
    let existing = entity::user::Entity::find_by_id(user.id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::Unauthorized)?;
    let mut model: entity::user::ActiveModel = existing.into();
    model.avatar_path = Set(Some(rel_path));
    model.avatar_updated_at = Set(Some(now));
    let updated = model.update(db.get_ref()).await?;

    Ok(web::Json(UserResponse::from(updated)))
}

/// Decode `bytes`, cover-crop to a square, resize to 256x256, and re-encode as
/// WebP. Rejects oversized images before decoding (decompression-bomb guard).
/// Intended to run inside `spawn_blocking`.
fn process_avatar_image(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    match ensure_within_pixel_limit(bytes, UPLOAD_IMAGE_MAX_PIXELS) {
        Ok(()) => {}
        Err(ImageLimitError::TooLarge) => return Err(AppError::PayloadTooLarge),
        Err(ImageLimitError::Unreadable) => return Err(AppError::InvalidRequest),
    }

    let img = image::load_from_memory(bytes).map_err(|_| AppError::InvalidRequest)?;
    let (w, h) = (img.width(), img.height());
    let side = w.min(h);
    if side == 0 {
        return Err(AppError::InvalidRequest);
    }
    let x = (w - side) / 2;
    let y = (h - side) / 2;
    let cropped = img.crop_imm(x, y, side, side);
    let resized = cropped.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);

    let mut out = Cursor::new(Vec::new());
    resized
        .write_to(&mut out, image::ImageFormat::WebP)
        .map_err(|e| AppError::Internal(format!("encode webp: {e}")))?;
    Ok(out.into_inner())
}

#[delete("/me/avatar")]
async fn delete_avatar(
    db: web::Data<DatabaseConnection>,
    storage: web::Data<AvatarStorage>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let existing = entity::user::Entity::find_by_id(user.id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::Unauthorized)?;

    if let Some(rel) = existing.avatar_path.clone() {
        let path = storage.dir.join(&rel);
        // Ignore ENOENT so delete is idempotent.
        if let Err(e) = std::fs::remove_file(&path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            return Err(AppError::Io(e));
        }
    }

    let mut model: entity::user::ActiveModel = existing.into();
    model.avatar_path = Set(None);
    model.avatar_updated_at = Set(None);
    let updated = model.update(db.get_ref()).await?;

    Ok(web::Json(UserResponse::from(updated)))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(upload_avatar).service(delete_avatar);
}
