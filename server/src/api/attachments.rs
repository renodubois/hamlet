//! Authenticated message attachment serving.
//!
//! Message photo bytes live in private storage and are served only after the
//! attachment metadata row has been loaded from the database. Stored paths are
//! relative to `AttachmentStorage::dir`; callers never provide filesystem
//! paths in the URL.

use std::path::{Component, Path, PathBuf};

use actix_web::http::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderValue};
use actix_web::{HttpResponse, get, web};
use sea_orm::{DatabaseConnection, EntityTrait};

use crate::auth::AuthUser;
use crate::entity;
use crate::error::AppError;

const PRIVATE_ATTACHMENT_CACHE_CONTROL: &str = "private, max-age=31536000, immutable";

/// Where private message attachment files are stored on disk. Registered as
/// `web::Data` by `start_server` and by tests that exercise attachment routes.
#[derive(Clone, Debug)]
pub struct AttachmentStorage {
    pub dir: PathBuf,
}

#[derive(Clone, Copy, Debug)]
enum AttachmentVariant {
    Full,
    Thumbnail,
}

impl AttachmentVariant {
    fn metadata(self, attachment: &entity::message_attachment::Model) -> (&str, &str) {
        match self {
            AttachmentVariant::Full => (&attachment.storage_path, &attachment.content_type),
            AttachmentVariant::Thumbnail => (
                &attachment.thumbnail_storage_path,
                &attachment.thumbnail_content_type,
            ),
        }
    }
}

#[get("/attachments/{attachment_id}")]
async fn get_attachment(
    db: web::Data<DatabaseConnection>,
    storage: web::Data<AttachmentStorage>,
    path: web::Path<i64>,
    _user: AuthUser,
) -> Result<HttpResponse, AppError> {
    serve_attachment_variant(db, storage, path.into_inner(), AttachmentVariant::Full).await
}

#[get("/attachments/{attachment_id}/thumbnail")]
async fn get_attachment_thumbnail(
    db: web::Data<DatabaseConnection>,
    storage: web::Data<AttachmentStorage>,
    path: web::Path<i64>,
    _user: AuthUser,
) -> Result<HttpResponse, AppError> {
    serve_attachment_variant(db, storage, path.into_inner(), AttachmentVariant::Thumbnail).await
}

async fn serve_attachment_variant(
    db: web::Data<DatabaseConnection>,
    storage: web::Data<AttachmentStorage>,
    attachment_id: i64,
    variant: AttachmentVariant,
) -> Result<HttpResponse, AppError> {
    let attachment = entity::message_attachment::Entity::find_by_id(attachment_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound)?;
    let (stored_path, content_type) = variant.metadata(&attachment);
    let path = resolve_storage_path(&storage.dir, stored_path)?;
    let bytes = read_attachment_file(&storage.dir, &path).await?;
    let content_type = HeaderValue::from_str(content_type)
        .map_err(|_| AppError::Internal("invalid attachment content type".to_owned()))?;

    Ok(HttpResponse::Ok()
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header((CACHE_CONTROL, PRIVATE_ATTACHMENT_CACHE_CONTROL))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .body(bytes))
}

/// Resolve a stored relative path under the private attachment root.
///
/// Only plain relative paths with normal components are accepted. Absolute
/// paths, `.` / `..`, prefixes, and empty paths are rejected before anything is
/// read from disk.
pub fn resolve_storage_path(storage_root: &Path, stored_path: &str) -> Result<PathBuf, AppError> {
    let relative = Path::new(stored_path);
    if stored_path.is_empty() || relative.is_absolute() {
        return Err(AppError::NotFound);
    }

    let mut saw_component = false;
    for component in relative.components() {
        match component {
            Component::Normal(_) => saw_component = true,
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(AppError::NotFound);
            }
        }
    }

    if !saw_component {
        return Err(AppError::NotFound);
    }

    Ok(storage_root.join(relative))
}

async fn read_attachment_file(storage_root: &Path, path: &Path) -> Result<Vec<u8>, AppError> {
    let canonical_root = canonicalize_existing_path(storage_root).await?;
    let canonical_path = canonicalize_existing_path(path).await?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(AppError::NotFound);
    }

    match tokio::fs::read(&canonical_path).await {
        Ok(bytes) => Ok(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(AppError::NotFound),
        Err(err) => Err(AppError::Io(err)),
    }
}

async fn canonicalize_existing_path(path: &Path) -> Result<PathBuf, AppError> {
    match tokio::fs::canonicalize(path).await {
        Ok(path) => Ok(path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(AppError::NotFound),
        Err(err) => Err(AppError::Io(err)),
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_attachment)
        .service(get_attachment_thumbnail);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn resolve_storage_path_accepts_nested_relative_paths() {
        let root = Path::new("/private-root");

        let resolved = resolve_storage_path(root, "messages/123/full.webp").unwrap();

        assert_eq!(resolved, root.join("messages/123/full.webp"));
    }

    #[test]
    fn resolve_storage_path_rejects_empty_absolute_and_traversal_paths() {
        let root = Path::new("/private-root");

        for stored_path in [
            "",
            "/etc/passwd",
            "../secret.jpg",
            "messages/../secret.jpg",
            "./messages/full.jpg",
        ] {
            assert!(
                resolve_storage_path(root, stored_path).is_err(),
                "{stored_path} should be rejected"
            );
        }
    }
}
