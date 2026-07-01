//! Identity HTTP handlers: register, login, logout, /me, /me update.
//!
//! The session/credential primitives live in `crate::auth`; this module is
//! the HTTP surface on top.

use actix_web::{HttpRequest, HttpResponse, Responder, get, post, put, web};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use serde::{Deserialize, Serialize};

use crate::api::avatars::avatar_url;
use crate::auth::{self, AuthUser};
use crate::entity;
use crate::error::AppError;

const DISPLAY_NAME_MAX_LEN: usize = 64;

#[derive(Clone, Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub email_verified: bool,
    pub avatar_url: Option<String>,
}

impl From<entity::user::Model> for UserResponse {
    fn from(u: entity::user::Model) -> Self {
        let avatar_url = avatar_url(u.avatar_path.as_deref(), u.avatar_updated_at);
        Self {
            id: u.id,
            username: u.username,
            display_name: u.display_name,
            email: u.email,
            email_verified: u.email_verified,
            avatar_url,
        }
    }
}

#[post("/register")]
async fn register(
    db: web::Data<DatabaseConnection>,
    body: web::Json<RegisterRequest>,
) -> Result<HttpResponse, AppError> {
    if body.username.is_empty() || body.password.is_empty() {
        return Err(AppError::InvalidRequest);
    }
    let user = auth::register_user(
        db.get_ref(),
        &body.username,
        &body.password,
        body.email.clone(),
    )
    .await?;
    let session = auth::create_session(db.get_ref(), user.id).await?;
    Ok(HttpResponse::Ok()
        .cookie(auth::session_cookie(session.token))
        .json(UserResponse::from(user)))
}

#[post("/login")]
async fn login(
    db: web::Data<DatabaseConnection>,
    body: web::Json<LoginRequest>,
) -> Result<HttpResponse, AppError> {
    let user = auth::authenticate_password(db.get_ref(), &body.username, &body.password).await?;
    let session = auth::create_session(db.get_ref(), user.id).await?;
    Ok(HttpResponse::Ok()
        .cookie(auth::session_cookie(session.token))
        .json(UserResponse::from(user)))
}

#[post("/logout")]
async fn logout(
    db: web::Data<DatabaseConnection>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    if let Some(c) = req.cookie(auth::SESSION_COOKIE) {
        auth::destroy_session(db.get_ref(), c.value()).await?;
    }
    Ok(HttpResponse::Ok()
        .cookie(auth::clear_session_cookie())
        .finish())
}

#[get("/me")]
async fn me(db: web::Data<DatabaseConnection>, user: AuthUser) -> Result<impl Responder, AppError> {
    let user = entity::user::Entity::find_by_id(user.id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(web::Json(UserResponse::from(user)))
}

#[put("/me")]
async fn update_me(
    db: web::Data<DatabaseConnection>,
    user: AuthUser,
    body: web::Json<UpdateProfileRequest>,
) -> Result<impl Responder, AppError> {
    // Treat whitespace-only input as "clear it" so users can't accidentally
    // stash a name that renders as blank.
    let new_display_name = match body.display_name.as_deref() {
        None => None,
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else if trimmed.chars().count() > DISPLAY_NAME_MAX_LEN {
                return Err(AppError::InvalidRequest);
            } else {
                Some(trimmed.to_owned())
            }
        }
    };

    let existing = entity::user::Entity::find_by_id(user.id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::Unauthorized)?;
    let mut model: entity::user::ActiveModel = existing.into();
    model.display_name = Set(new_display_name);
    let updated = model.update(db.get_ref()).await?;

    Ok(web::Json(UserResponse::from(updated)))
}

#[put("/me/password")]
async fn change_password(
    db: web::Data<DatabaseConnection>,
    user: AuthUser,
    body: web::Json<ChangePasswordRequest>,
) -> Result<HttpResponse, AppError> {
    if body.current_password.is_empty() || body.new_password.is_empty() {
        return Err(AppError::InvalidRequest);
    }

    auth::change_password(
        db.get_ref(),
        user.id,
        &body.current_password,
        &body.new_password,
    )
    .await?;

    Ok(HttpResponse::NoContent().finish())
}

/// Public surface: `register`, `login`, `logout`. Auth-gated: `/me`, `/me` PUT, `/me/password` PUT.
pub fn configure_public(cfg: &mut web::ServiceConfig) {
    cfg.service(register).service(login).service(logout);
}

pub fn configure_authed(cfg: &mut web::ServiceConfig) {
    cfg.service(me).service(update_me).service(change_password);
}
