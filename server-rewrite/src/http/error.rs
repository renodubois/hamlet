use actix_web::{HttpResponse, http::StatusCode};
use serde::Serialize;

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct ErrorBody {
    error: ErrorInfo,
}
#[derive(Serialize, utoipa::ToSchema)]
struct ErrorInfo {
    code: &'static str,
    message: &'static str,
}

/// Creates a HTTP response for error states
pub(crate) fn problem(
    status: StatusCode,
    code: &'static str,
    message: &'static str,
) -> HttpResponse {
    HttpResponse::build(status).json(ErrorBody {
        error: ErrorInfo { code, message },
    })
}

// Some generic error states
pub(crate) fn bad_request() -> HttpResponse {
    problem(StatusCode::BAD_REQUEST, "bad_request", "Invalid request")
}
pub(crate) fn internal() -> HttpResponse {
    problem(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "Internal server error",
    )
}
pub(crate) fn unauthorized() -> HttpResponse {
    problem(StatusCode::UNAUTHORIZED, "unauthorized", "Unauthorized")
}
