use std::future::Future;
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};

use actix_web::{FromRequest, HttpMessage, HttpRequest, cookie::Cookie, dev::Payload, web};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use rand::RngCore;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::entity;
use crate::{UserError, generate_id};

pub const SESSION_COOKIE: &str = "session";
pub const PASSWORD_PROVIDER: &str = "password";
const SESSION_DURATION_SECS: i64 = 60 * 60 * 24 * 30;

pub fn hash_password(password: &str) -> Result<String, UserError> {
    let mut salt_bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|_| UserError::InternalError)?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| UserError::InternalError)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

fn generate_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub async fn create_session(
    db: &DatabaseConnection,
    user_id: i64,
) -> Result<entity::session::Model, UserError> {
    let now = now_secs();
    let session = entity::session::ActiveModel {
        token: Set(generate_session_token()),
        user_id: Set(user_id),
        created_at: Set(now),
        expires_at: Set(now + SESSION_DURATION_SECS),
    };
    session.insert(db).await.map_err(|_| UserError::DbError)
}

pub async fn destroy_session(db: &DatabaseConnection, token: &str) -> Result<(), UserError> {
    entity::session::Entity::delete_by_id(token.to_owned())
        .exec(db)
        .await
        .map_err(|_| UserError::DbError)?;
    Ok(())
}

pub async fn validate_session(db: &DatabaseConnection, token: &str) -> Result<AuthUser, UserError> {
    let session = entity::session::Entity::find_by_id(token.to_owned())
        .one(db)
        .await
        .map_err(|_| UserError::DbError)?
        .ok_or(UserError::Unauthorized)?;

    if session.expires_at <= now_secs() {
        return Err(UserError::Unauthorized);
    }

    let user = entity::user::Entity::find_by_id(session.user_id)
        .one(db)
        .await
        .map_err(|_| UserError::DbError)?
        .ok_or(UserError::Unauthorized)?;

    Ok(AuthUser {
        id: user.id,
        username: user.username,
        avatar_path: user.avatar_path,
        avatar_updated_at: user.avatar_updated_at,
    })
}

pub async fn register_user(
    db: &DatabaseConnection,
    username: &str,
    password: &str,
    email: Option<String>,
) -> Result<entity::user::Model, UserError> {
    let existing = entity::credential::Entity::find()
        .filter(entity::credential::Column::Provider.eq(PASSWORD_PROVIDER))
        .filter(entity::credential::Column::ExternalId.eq(username))
        .one(db)
        .await
        .map_err(|_| UserError::DbError)?;
    if existing.is_some() {
        return Err(UserError::UsernameTaken);
    }

    let user_id = generate_id();
    let user = entity::user::ActiveModel {
        id: Set(user_id),
        username: Set(username.to_owned()),
        email: Set(email),
        email_verified: Set(false),
        avatar_path: Set(None),
        avatar_updated_at: Set(None),
    };
    let user = user.insert(db).await.map_err(|_| UserError::DbError)?;

    let hash = hash_password(password)?;
    let credential = entity::credential::ActiveModel {
        id: Set(generate_id()),
        user_id: Set(user_id),
        provider: Set(PASSWORD_PROVIDER.to_owned()),
        external_id: Set(username.to_owned()),
        secret: Set(Some(hash)),
    };
    credential
        .insert(db)
        .await
        .map_err(|_| UserError::DbError)?;

    Ok(user)
}

pub async fn authenticate_password(
    db: &DatabaseConnection,
    username: &str,
    password: &str,
) -> Result<entity::user::Model, UserError> {
    let credential = entity::credential::Entity::find()
        .filter(entity::credential::Column::Provider.eq(PASSWORD_PROVIDER))
        .filter(entity::credential::Column::ExternalId.eq(username))
        .one(db)
        .await
        .map_err(|_| UserError::DbError)?
        .ok_or(UserError::InvalidCredentials)?;

    let hash = credential
        .secret
        .as_deref()
        .ok_or(UserError::InvalidCredentials)?;
    if !verify_password(password, hash) {
        return Err(UserError::InvalidCredentials);
    }

    entity::user::Entity::find_by_id(credential.user_id)
        .one(db)
        .await
        .map_err(|_| UserError::DbError)?
        .ok_or(UserError::InvalidCredentials)
}

pub fn session_cookie(token: String) -> Cookie<'static> {
    Cookie::build(SESSION_COOKIE, token)
        .http_only(true)
        .path("/")
        .same_site(actix_web::cookie::SameSite::Lax)
        .finish()
}

pub fn clear_session_cookie() -> Cookie<'static> {
    let mut c = Cookie::build(SESSION_COOKIE, "")
        .http_only(true)
        .path("/")
        .same_site(actix_web::cookie::SameSite::Lax)
        .finish();
    c.make_removal();
    c
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    pub avatar_path: Option<String>,
    pub avatar_updated_at: Option<i64>,
}

impl FromRequest for AuthUser {
    type Error = UserError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, UserError>>>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        // Fast path: already validated by require_auth middleware
        if let Some(user) = req.extensions().get::<AuthUser>().cloned() {
            return Box::pin(async move { Ok(user) });
        }

        let db = req.app_data::<web::Data<DatabaseConnection>>().cloned();
        let token = req.cookie(SESSION_COOKIE).map(|c| c.value().to_owned());

        Box::pin(async move {
            let db = db.ok_or(UserError::InternalError)?;
            let token = token.ok_or(UserError::Unauthorized)?;
            validate_session(db.get_ref(), &token).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_password_does_not_return_plaintext() {
        let hash = hash_password("hunter2").unwrap();
        assert_ne!(hash, "hunter2");
        assert!(hash.starts_with("$argon2"));
    }

    #[test]
    fn verify_password_accepts_correct_password() {
        let hash = hash_password("hunter2").unwrap();
        assert!(verify_password("hunter2", &hash));
    }

    #[test]
    fn verify_password_rejects_wrong_password() {
        let hash = hash_password("hunter2").unwrap();
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn verify_password_rejects_malformed_hash() {
        assert!(!verify_password("anything", "not-a-real-hash"));
    }

    #[test]
    fn hash_password_uses_random_salt() {
        let h1 = hash_password("hunter2").unwrap();
        let h2 = hash_password("hunter2").unwrap();
        assert_ne!(h1, h2);
    }
}
