//! Application-wide error type.
//!
//! `AppError` is what every handler returns. It carries enough internal
//! detail (via `#[from]`) to log a useful cause, but its `ResponseError`
//! impl renders a sanitized JSON body so we never leak internal errors to
//! the client.
//!
//! Body shape: `{ "error": { "kind": "<snake_case>", "message": "..." } }`.

use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("no channel found with that ID")]
    NoChannelFound,
    #[error("not found")]
    NotFound,
    #[error("invalid request")]
    InvalidRequest,
    #[error("unauthorized")]
    Unauthorized,
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("username already taken")]
    UsernameTaken,
    #[error("forbidden")]
    Forbidden,
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("service unavailable")]
    ServiceUnavailable,

    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    fn kind(&self) -> &'static str {
        match self {
            AppError::NoChannelFound => "no_channel_found",
            AppError::NotFound => "not_found",
            AppError::InvalidRequest => "invalid_request",
            AppError::Unauthorized => "unauthorized",
            AppError::InvalidCredentials => "invalid_credentials",
            AppError::UsernameTaken => "username_taken",
            AppError::Forbidden => "forbidden",
            AppError::PayloadTooLarge => "payload_too_large",
            AppError::ServiceUnavailable => "service_unavailable",
            AppError::Db(_) | AppError::Json(_) | AppError::Io(_) | AppError::Internal(_) => {
                "internal_error"
            }
        }
    }

    /// User-facing message. Internal errors collapse to a generic string so
    /// we don't leak stack traces or driver-specific text to clients.
    fn public_message(&self) -> String {
        match self {
            AppError::Db(_) | AppError::Json(_) | AppError::Io(_) | AppError::Internal(_) => {
                "internal server error".to_owned()
            }
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody<'a> {
    error: ErrorDetails<'a>,
}

#[derive(Debug, Serialize)]
struct ErrorDetails<'a> {
    kind: &'a str,
    message: String,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::NoChannelFound | AppError::InvalidRequest => StatusCode::BAD_REQUEST,
            AppError::Unauthorized | AppError::InvalidCredentials => StatusCode::UNAUTHORIZED,
            AppError::Forbidden => StatusCode::FORBIDDEN,
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::UsernameTaken => StatusCode::CONFLICT,
            AppError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            AppError::Db(_) | AppError::Json(_) | AppError::Io(_) | AppError::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    fn error_response(&self) -> HttpResponse {
        // Internal errors carry a real cause — log it before sanitizing.
        match self {
            AppError::Db(e) => tracing::error!(error = %e, "db error"),
            AppError::Json(e) => tracing::error!(error = %e, "json error"),
            AppError::Io(e) => tracing::error!(error = %e, "io error"),
            AppError::Internal(msg) => tracing::error!(error = %msg, "internal error"),
            _ => {}
        }
        HttpResponse::build(self.status_code()).json(ErrorBody {
            error: ErrorDetails {
                kind: self.kind(),
                message: self.public_message(),
            },
        })
    }
}
