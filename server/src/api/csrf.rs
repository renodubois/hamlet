//! Public CSRF bootstrap endpoint.

use actix_web::{HttpRequest, HttpResponse, get, web};
use sea_orm::DatabaseConnection;
use serde::Serialize;

use crate::auth;
use crate::config::CookieConfig;
use crate::csrf;
use crate::error::AppError;

#[derive(Debug, Serialize)]
struct CsrfResponse {
    token: String,
}

#[get("/csrf")]
async fn get_csrf_token(
    db: web::Data<DatabaseConnection>,
    cookie_config: web::Data<CookieConfig>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let session_token = req
        .cookie(auth::SESSION_COOKIE)
        .map(|cookie| cookie.value().to_owned())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(db.get_ref(), &session_token).await?;

    let token = csrf::token_from_request(&req, &session_token)?;
    Ok(HttpResponse::Ok()
        .cookie(csrf::csrf_cookie(token.clone(), cookie_config.get_ref()))
        .json(CsrfResponse { token }))
}

pub fn configure_public(cfg: &mut web::ServiceConfig) {
    cfg.service(get_csrf_token);
}
