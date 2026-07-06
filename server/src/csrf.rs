//! Signed double-submit CSRF tokens for credentialed browser writes.
//!
//! Tokens are bound to the opaque session cookie value: `/csrf` returns a
//! `v1.<nonce>.<hmac>` token and sets the same value in a non-HttpOnly cookie.
//! Browser writes echo the value in `X-Hamlet-CSRF`; middleware verifies that
//! the cookie and header match and that the signature was generated for the
//! caller's current session token.

use actix_web::{HttpRequest, cookie::Cookie, dev::ServiceRequest, http::Method};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

use crate::config::CookieConfig;
use crate::error::AppError;

pub const CSRF_COOKIE: &str = "hamlet_csrf";
pub const CSRF_HEADER: &str = "X-Hamlet-CSRF";

const TOKEN_VERSION: &str = "v1";
const TOKEN_CONTEXT: &[u8] = b"hamlet-csrf-v1\0";
const NONCE_BYTES: usize = 32;
const HEX_SHA256_BYTES: usize = 64;

type HmacSha256 = Hmac<Sha256>;

pub fn generate_token(session_token: &str) -> Result<String, AppError> {
    let mut nonce = [0_u8; NONCE_BYTES];
    rand::rng().fill_bytes(&mut nonce);
    let nonce_hex = hex::encode(nonce);
    let signature = sign(session_token, &nonce_hex)?;

    Ok(format!(
        "{TOKEN_VERSION}.{nonce_hex}.{}",
        hex::encode(signature)
    ))
}

pub fn csrf_cookie(token: String, config: &CookieConfig) -> Cookie<'static> {
    Cookie::build(CSRF_COOKIE, token)
        .http_only(false)
        .path("/")
        .secure(config.secure)
        .same_site(config.same_site.into())
        .finish()
}

pub fn clear_csrf_cookie(config: &CookieConfig) -> Cookie<'static> {
    let mut cookie = csrf_cookie(String::new(), config);
    cookie.make_removal();
    cookie
}

/// Only browser-shaped unsafe writes are challenged. This keeps non-browser
/// local tooling and existing route tests from needing a CSRF bootstrap while
/// still protecting credentialed browser writes, which include an Origin and
/// cannot set the custom header without first reading `/csrf`.
pub fn should_validate_browser_write(req: &ServiceRequest) -> bool {
    is_unsafe_method(req.method())
        && (req.headers().contains_key(actix_web::http::header::ORIGIN)
            || req.headers().contains_key(CSRF_HEADER)
            || req.cookie(CSRF_COOKIE).is_some())
}

pub fn validate_service_request(req: &ServiceRequest, session_token: &str) -> Result<(), AppError> {
    let header_token = req
        .headers()
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or(AppError::InvalidCsrfToken)?;
    let cookie_token = req
        .cookie(CSRF_COOKIE)
        .map(|cookie| cookie.value().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or(AppError::InvalidCsrfToken)?;

    if header_token != cookie_token {
        return Err(AppError::InvalidCsrfToken);
    }

    validate_token(header_token, session_token)
}

pub fn token_from_request(req: &HttpRequest, session_token: &str) -> Result<String, AppError> {
    // Keep `/csrf` idempotent within a browser session when the cookie is
    // already present and still valid for the current session. This avoids
    // rotating the cookie out from under concurrent unsafe requests.
    if let Some(existing) = req
        .cookie(CSRF_COOKIE)
        .map(|cookie| cookie.value().to_owned())
        && validate_token(&existing, session_token).is_ok()
    {
        return Ok(existing);
    }

    generate_token(session_token)
}

pub fn validate_token(token: &str, session_token: &str) -> Result<(), AppError> {
    let mut parts = token.split('.');
    let Some(version) = parts.next() else {
        return Err(AppError::InvalidCsrfToken);
    };
    let Some(nonce_hex) = parts.next() else {
        return Err(AppError::InvalidCsrfToken);
    };
    let Some(signature_hex) = parts.next() else {
        return Err(AppError::InvalidCsrfToken);
    };
    if parts.next().is_some() || version != TOKEN_VERSION || !is_hex_32_bytes(nonce_hex) {
        return Err(AppError::InvalidCsrfToken);
    }

    let signature = hex::decode(signature_hex).map_err(|_| AppError::InvalidCsrfToken)?;
    if signature.len() != NONCE_BYTES {
        return Err(AppError::InvalidCsrfToken);
    }

    let mut mac = mac_for_session(session_token)?;
    mac.update(TOKEN_CONTEXT);
    mac.update(nonce_hex.as_bytes());
    mac.verify_slice(&signature)
        .map_err(|_| AppError::InvalidCsrfToken)
}

fn is_unsafe_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

fn is_hex_32_bytes(value: &str) -> bool {
    value.len() == HEX_SHA256_BYTES && value.as_bytes().iter().all(|byte| byte.is_ascii_hexdigit())
}

fn sign(session_token: &str, nonce_hex: &str) -> Result<Vec<u8>, AppError> {
    let mut mac = mac_for_session(session_token)?;
    mac.update(TOKEN_CONTEXT);
    mac.update(nonce_hex.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

fn mac_for_session(session_token: &str) -> Result<HmacSha256, AppError> {
    HmacSha256::new_from_slice(session_token.as_bytes())
        .map_err(|error| AppError::Internal(format!("csrf hmac key: {error:?}")))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn generated_token_validates_for_matching_session() {
        let token = generate_token("session-a").unwrap();

        assert!(validate_token(&token, "session-a").is_ok());
        assert!(validate_token(&token, "session-b").is_err());
    }

    #[test]
    fn malformed_tokens_are_rejected() {
        for token in [
            "",
            "v1",
            "v1.not-hex.signature",
            "v2.0000000000000000000000000000000000000000000000000000000000000000.00",
            "v1.0000000000000000000000000000000000000000000000000000000000000000.00",
        ] {
            assert!(validate_token(token, "session").is_err(), "{token}");
        }
    }
}
