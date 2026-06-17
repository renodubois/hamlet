//! Dev-only seed data. Inserts two dev users (`baipas` / `teo`), seeds an
//! avatar for `baipas`, and prints a fixed session token for the impatient.
//!
//! Default channel bootstrap lives in `crate::bootstrap`, and schema
//! initialization lives in `crate::database`; this module assumes both have
//! already run when needed.

use std::io::{ErrorKind, Write};
use std::path::Path;

use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::api::avatars::AVATARS_SUBDIR;
use crate::auth::{self, PASSWORD_PROVIDER};
use crate::entity;
use crate::error::AppError;
use crate::util::now_unix_secs;

const DEV_SESSION_TOKEN: &str =
    "devdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdev";
const DEV_SESSION_DURATION_SECS: i64 = 60 * 60 * 24 * 365;

pub async fn seed_development_data(
    db: &DatabaseConnection,
    uploads_dir: &Path,
) -> Result<(), AppError> {
    let baipas = ensure_password_user(db, "baipas", "password").await?;
    ensure_password_user(db, "teo", "password").await?;

    ensure_placeholder_avatar(db, uploads_dir, &baipas).await?;
    upsert_dev_session(db, baipas.id).await?;

    tracing::info!(
        token = DEV_SESSION_TOKEN,
        "DEV: baipas session active — set cookie: session=<token>"
    );

    Ok(())
}

async fn ensure_password_user(
    db: &DatabaseConnection,
    username: &str,
    password: &str,
) -> Result<entity::user::Model, AppError> {
    let credential = entity::credential::Entity::find()
        .filter(entity::credential::Column::Provider.eq(PASSWORD_PROVIDER))
        .filter(entity::credential::Column::ExternalId.eq(username))
        .one(db)
        .await?;

    let Some(credential) = credential else {
        return auth::register_user(db, username, password, None).await;
    };

    entity::user::Entity::find_by_id(credential.user_id)
        .one(db)
        .await?
        .ok_or_else(|| {
            AppError::Internal(format!(
                "development seed credential {username:?} points at missing user {}",
                credential.user_id
            ))
        })
}

async fn ensure_placeholder_avatar(
    db: &DatabaseConnection,
    uploads_dir: &Path,
    user: &entity::user::Model,
) -> Result<entity::user::Model, AppError> {
    let avatar_path = seeded_avatar_relative_path(user.id);

    if let Some(existing_path) = user.avatar_path.as_deref() {
        if existing_path == avatar_path {
            write_placeholder_avatar_if_missing(uploads_dir, user.id)?;
        }
        return Ok(user.clone());
    }

    write_placeholder_avatar_if_missing(uploads_dir, user.id)?;

    let mut model: entity::user::ActiveModel = user.clone().into();
    model.avatar_path = Set(Some(avatar_path));
    model.avatar_updated_at = Set(Some(now_unix_secs()));
    Ok(model.update(db).await?)
}

async fn upsert_dev_session(db: &DatabaseConnection, user_id: i64) -> Result<(), AppError> {
    let now = now_unix_secs();
    let expires_at = now + DEV_SESSION_DURATION_SECS;

    if let Some(existing) = entity::session::Entity::find_by_id(DEV_SESSION_TOKEN.to_owned())
        .one(db)
        .await?
    {
        let mut model: entity::session::ActiveModel = existing.into();
        model.user_id = Set(user_id);
        model.created_at = Set(now);
        model.expires_at = Set(expires_at);
        model.update(db).await?;
    } else {
        entity::session::ActiveModel {
            token: Set(DEV_SESSION_TOKEN.to_owned()),
            user_id: Set(user_id),
            created_at: Set(now),
            expires_at: Set(expires_at),
        }
        .insert(db)
        .await?;
    }

    Ok(())
}

fn write_placeholder_avatar_if_missing(uploads_dir: &Path, user_id: i64) -> Result<(), AppError> {
    let avatars_dir = uploads_dir.join(AVATARS_SUBDIR);
    std::fs::create_dir_all(&avatars_dir)?;

    let avatar_path = avatars_dir.join(seeded_avatar_filename(user_id));
    match std::fs::metadata(&avatar_path) {
        Ok(metadata) if metadata.is_file() => return Ok(()),
        Ok(_) => {
            return Err(AppError::Io(std::io::Error::new(
                ErrorKind::AlreadyExists,
                format!(
                    "development avatar path {} exists but is not a file",
                    avatar_path.display()
                ),
            )));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let bytes = default_avatar_webp()?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&avatar_path)
    {
        Ok(mut file) => file.write_all(&bytes).map_err(AppError::from),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn seeded_avatar_relative_path(user_id: i64) -> String {
    format!("{AVATARS_SUBDIR}/{}", seeded_avatar_filename(user_id))
}

fn seeded_avatar_filename(user_id: i64) -> String {
    format!("{user_id}.webp")
}

/// 256x256 placeholder webp (a simple two-color circle) used for the dev
/// user's seeded avatar.
fn default_avatar_webp() -> Result<Vec<u8>, AppError> {
    use std::io::Cursor;

    use image::{Rgb, RgbImage};
    let size = 256u32;
    let mut img = RgbImage::new(size, size);
    let cx = size as i32 / 2;
    let cy = size as i32 / 2;
    let r2 = (size as i32 / 2 - 8).pow(2);
    let bg = Rgb([44u8, 82, 130]);
    let fg = Rgb([129u8, 230, 217]);
    for y in 0..size {
        for x in 0..size {
            let dx = x as i32 - cx;
            let dy = y as i32 - cy;
            let pixel = if dx * dx + dy * dy < r2 { fg } else { bg };
            img.put_pixel(x, y, pixel);
        }
    }
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageFormat::WebP)
        .map_err(|error| {
            AppError::Internal(format!("development avatar encoding failed: {error}"))
        })?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait};

    use super::*;
    use crate::auth;
    use crate::database::connect_initialized_database_url;
    use crate::util::generate_id;

    #[actix_web::test]
    async fn repeated_seed_runs_reuse_dev_users_credentials_and_session() {
        let db = initialized_db("repeated_seed").await;
        let uploads_dir = tmp_uploads_dir("repeated_seed");

        seed_development_data(&db, &uploads_dir).await.unwrap();
        let baipas = user_for_password_credential(&db, "baipas").await;
        let teo = user_for_password_credential(&db, "teo").await;
        let avatar_path = baipas.avatar_path.clone();

        seed_development_data(&db, &uploads_dir).await.unwrap();

        assert_eq!(entity::user::Entity::find().count(&db).await.unwrap(), 2);
        assert_eq!(
            entity::credential::Entity::find().count(&db).await.unwrap(),
            2
        );
        assert_eq!(entity::session::Entity::find().count(&db).await.unwrap(), 1);

        let baipas_after = user_for_password_credential(&db, "baipas").await;
        let teo_after = user_for_password_credential(&db, "teo").await;
        assert_eq!(baipas_after.id, baipas.id);
        assert_eq!(teo_after.id, teo.id);
        assert_eq!(baipas_after.avatar_path, avatar_path);

        let authenticated_baipas = auth::authenticate_password(&db, "baipas", "password")
            .await
            .unwrap();
        let authenticated_teo = auth::authenticate_password(&db, "teo", "password")
            .await
            .unwrap();
        assert_eq!(authenticated_baipas.id, baipas.id);
        assert_eq!(authenticated_teo.id, teo.id);

        let validated = auth::validate_session(&db, DEV_SESSION_TOKEN)
            .await
            .unwrap();
        assert_eq!(validated.id, baipas.id);
        assert_eq!(validated.username, "baipas");

        let _ = std::fs::remove_dir_all(uploads_dir);
    }

    #[actix_web::test]
    async fn fixed_dev_session_is_replaced_for_baipas_with_fresh_expiration() {
        let db = initialized_db("replaced_session").await;
        let uploads_dir = tmp_uploads_dir("replaced_session");
        let other_user = insert_user(&db, "other").await;

        entity::session::ActiveModel {
            token: Set(DEV_SESSION_TOKEN.to_owned()),
            user_id: Set(other_user.id),
            created_at: Set(1),
            expires_at: Set(2),
        }
        .insert(&db)
        .await
        .unwrap();

        let before_seed = now_unix_secs();
        seed_development_data(&db, &uploads_dir).await.unwrap();

        let validated = auth::validate_session(&db, DEV_SESSION_TOKEN)
            .await
            .unwrap();
        assert_eq!(validated.username, "baipas");
        assert_ne!(validated.id, other_user.id);

        let session = entity::session::Entity::find_by_id(DEV_SESSION_TOKEN.to_owned())
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(session.user_id, validated.id);
        assert!(session.created_at >= before_seed);
        assert!(session.expires_at >= before_seed + DEV_SESSION_DURATION_SECS);

        let _ = std::fs::remove_dir_all(uploads_dir);
    }

    #[actix_web::test]
    async fn dev_avatar_seed_preserves_existing_custom_avatar_reference() {
        let db = initialized_db("custom_avatar").await;
        let uploads_dir = tmp_uploads_dir("custom_avatar");
        seed_development_data(&db, &uploads_dir).await.unwrap();

        let baipas = user_for_password_credential(&db, "baipas").await;
        let custom_path = format!("{AVATARS_SUBDIR}/custom.webp");
        let custom_disk_path = uploads_dir.join(&custom_path);
        std::fs::create_dir_all(custom_disk_path.parent().unwrap()).unwrap();
        std::fs::write(&custom_disk_path, b"custom avatar bytes").unwrap();

        let mut model: entity::user::ActiveModel = baipas.into();
        model.avatar_path = Set(Some(custom_path.clone()));
        model.avatar_updated_at = Set(Some(123));
        model.update(&db).await.unwrap();

        seed_development_data(&db, &uploads_dir).await.unwrap();

        let after = user_for_password_credential(&db, "baipas").await;
        assert_eq!(after.avatar_path, Some(custom_path));
        assert_eq!(after.avatar_updated_at, Some(123));
        assert_eq!(
            std::fs::read(custom_disk_path).unwrap(),
            b"custom avatar bytes"
        );

        let _ = std::fs::remove_dir_all(uploads_dir);
    }

    #[actix_web::test]
    async fn repeated_seed_does_not_overwrite_existing_seeded_avatar_file_or_timestamp() {
        let db = initialized_db("seeded_avatar_preserved").await;
        let uploads_dir = tmp_uploads_dir("seeded_avatar_preserved");
        seed_development_data(&db, &uploads_dir).await.unwrap();

        let baipas = user_for_password_credential(&db, "baipas").await;
        let avatar_path = baipas.avatar_path.clone().unwrap();
        let avatar_disk_path = uploads_dir.join(&avatar_path);
        std::fs::write(&avatar_disk_path, b"keep this file").unwrap();

        let mut model: entity::user::ActiveModel = baipas.into();
        model.avatar_updated_at = Set(Some(456));
        model.update(&db).await.unwrap();

        seed_development_data(&db, &uploads_dir).await.unwrap();

        let after = user_for_password_credential(&db, "baipas").await;
        assert_eq!(after.avatar_path.as_deref(), Some(avatar_path.as_str()));
        assert_eq!(after.avatar_updated_at, Some(456));
        assert_eq!(std::fs::read(avatar_disk_path).unwrap(), b"keep this file");

        let _ = std::fs::remove_dir_all(uploads_dir);
    }

    #[actix_web::test]
    async fn seed_returns_typed_error_when_schema_is_missing() {
        let db = crate::database::connect_database_url(&unique_memory_url("missing_schema"))
            .await
            .unwrap();
        let uploads_dir = tmp_uploads_dir("missing_schema");

        let error = seed_development_data(&db, &uploads_dir).await.unwrap_err();

        assert!(matches!(error, AppError::Db(_)));

        let _ = std::fs::remove_dir_all(uploads_dir);
    }

    async fn initialized_db(label: &str) -> DatabaseConnection {
        connect_initialized_database_url(&unique_memory_url(label))
            .await
            .unwrap()
    }

    async fn user_for_password_credential(
        db: &DatabaseConnection,
        username: &str,
    ) -> entity::user::Model {
        let credential = entity::credential::Entity::find()
            .filter(entity::credential::Column::Provider.eq(PASSWORD_PROVIDER))
            .filter(entity::credential::Column::ExternalId.eq(username))
            .one(db)
            .await
            .unwrap()
            .unwrap();

        entity::user::Entity::find_by_id(credential.user_id)
            .one(db)
            .await
            .unwrap()
            .unwrap()
    }

    async fn insert_user(db: &DatabaseConnection, username: &str) -> entity::user::Model {
        entity::user::ActiveModel {
            id: Set(generate_id()),
            username: Set(username.to_owned()),
            display_name: Set(None),
            email: Set(None),
            email_verified: Set(false),
            avatar_path: Set(None),
            avatar_updated_at: Set(None),
        }
        .insert(db)
        .await
        .unwrap()
    }

    fn unique_memory_url(label: &str) -> String {
        format!(
            "sqlite:file:hamlet_seed_test_{label}_{}?mode=memory&cache=shared",
            generate_id()
        )
    }

    fn tmp_uploads_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("hamlet-seed-test-{label}-{}", generate_id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
