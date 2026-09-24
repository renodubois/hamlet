use crate::{AuthResponse, User, digest, new_id, new_token, valid_password, valid_username};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString},
};
use chrono::{Duration, Utc};
use rand::RngCore;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};

pub enum SignupError {
    Invalid,
    Duplicate,
    Internal,
}

/// Complete the user and first session atomically; only a random ID conflict is retryable.
pub async fn register(
    db: &DatabaseConnection,
    username: &str,
    password: &str,
) -> Result<AuthResponse, SignupError> {
    if !valid_username(username) || !valid_password(password) {
        return Err(SignupError::Invalid);
    }
    let password = password.to_owned();
    let hash = actix_web::rt::task::spawn_blocking(move || {
        let mut bytes = [0u8; 16];
        rand::rng().fill_bytes(&mut bytes);
        let salt = SaltString::encode_b64(&bytes).expect("valid salt");
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
    })
    .await
    .map_err(|_| SignupError::Internal)?
    .map_err(|_| SignupError::Internal)?;
    for _ in 0..5 {
        let id = new_id();
        let tx = db.begin().await.map_err(|_| SignupError::Internal)?;
        let result = tx
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO users (id, username, username_key, password_hash) VALUES (?, ?, ?, ?)",
                [
                    id.into(),
                    username.to_owned().into(),
                    username.to_ascii_lowercase().into(),
                    hash.clone().into(),
                ],
            ))
            .await;
        match result {
            Ok(_) => {
                let token = new_token();
                let expiry = Utc::now() + Duration::days(30);
                tx.execute_raw(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "INSERT INTO sessions (token_digest, user_id, expires_at) VALUES (?, ?, ?)",
                    [digest(&token).into(), id.into(), expiry.to_rfc3339().into()],
                ))
                .await
                .map_err(|_| SignupError::Internal)?;
                tx.commit().await.map_err(|_| SignupError::Internal)?;
                return Ok(AuthResponse {
                    user: User {
                        id: id.to_string(),
                        username: username.into(),
                    },
                    access_token: token,
                    expires_at: expiry,
                });
            }
            Err(error) => {
                let message = error.to_string();
                if message.contains("users.username_key") {
                    return Err(SignupError::Duplicate);
                }
                if message.contains("users.id") {
                    continue;
                }
                return Err(SignupError::Internal);
            }
        }
    }
    Err(SignupError::Internal)
}
