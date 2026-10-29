//! Status classification shared by every endpoint: one success status, endpoint-specific
//! coded failures, then common fallbacks. A coded status is trusted only when the minimal
//! error body carries the matching code; otherwise the response is malformed.

use super::{ApiError, client::Response, error::ErrorResponse};
use reqwest::StatusCode;

/// An endpoint-defined failure: status, required `error.code`, and the typed result.
pub(super) type Coded = (StatusCode, &'static str, ApiError);

pub(super) const BAD_REQUEST: Coded = (
    StatusCode::BAD_REQUEST,
    "bad_request",
    ApiError::InvalidInput,
);
pub(super) const CONFLICT: Coded = (StatusCode::CONFLICT, "conflict", ApiError::Conflict);
pub(super) const FORBIDDEN: Coded = (StatusCode::FORBIDDEN, "forbidden", ApiError::Forbidden);
pub(super) const NOT_FOUND: Coded = (StatusCode::NOT_FOUND, "not_found", ApiError::NotFound);

/// Fallback for statuses an operation does not define; status-only, never reads a body.
pub(super) fn status_error(status: StatusCode) -> ApiError {
    match status {
        StatusCode::UNAUTHORIZED => ApiError::SessionRejected,
        StatusCode::NOT_FOUND => ApiError::NotFound,
        StatusCode::INTERNAL_SERVER_ERROR => ApiError::ServerFailure,
        _ => ApiError::UnexpectedStatus,
    }
}

/// Body decoding stays lazy: success and fallback statuses never wait for an error body.
pub(super) async fn expect(
    response: Response,
    success: StatusCode,
    coded: &[Coded],
) -> Result<Response, ApiError> {
    let status = response.status();
    if status == success {
        return Ok(response);
    }
    if !coded.iter().any(|(candidate, ..)| *candidate == status) {
        return Err(status_error(status));
    }
    let body: ErrorResponse = response.json().await?;
    Err(coded
        .iter()
        .find(|(candidate, code, _)| *candidate == status && *code == body.error.code)
        .map_or(ApiError::InvalidResponse, |(.., error)| *error))
}

pub(super) async fn expect_json<T: serde::de::DeserializeOwned>(
    response: Response,
    success: StatusCode,
    coded: &[Coded],
) -> Result<T, ApiError> {
    expect(response, success, coded).await?.json().await
}
