use crate::{
    AppState, digest,
    http::error::{internal, unauthorized},
};
use actix_web::{Error, HttpMessage, dev::ServiceRequest, middleware::Next, web};
use chrono::{DateTime, Utc};
use sea_orm::{ConnectionTrait, DbBackend, Statement};

#[derive(Clone)]
pub struct Identity {
    pub user: UserIdentity,
    pub token_digest: String,
}
#[derive(Clone)]
pub struct UserIdentity {
    pub id: i64,
    pub username: String,
}

/// Extracts a bearer token from a request, and validates what user it belongs to
pub(crate) async fn bearer(
    req: ServiceRequest,
    next: Next<impl actix_web::body::MessageBody + 'static>,
) -> Result<actix_web::dev::ServiceResponse<actix_web::body::BoxBody>, Error> {
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|t| !t.is_empty() && !t.contains(' '));
    let identity = if let (Some(token), Some(db)) = (token, req.app_data::<web::Data<AppState>>()) {
        match db.db.query_one_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
            "SELECT u.id, u.username, s.expires_at FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token_digest = ?",
            [digest(token).into()])).await {
            Ok(Some(row)) => {
                let fields = (|| -> Option<(i64, String, DateTime<chrono::FixedOffset>)> {
                    Some((row.try_get("", "id").ok()?, row.try_get("", "username").ok()?,
                        DateTime::parse_from_rfc3339(&row.try_get::<String>("", "expires_at").ok()?).ok()?))
                })();
                match fields {
                    Some((id, username, expiry)) if expiry > Utc::now() => Some(Identity {
                        user: UserIdentity { id, username }, token_digest: digest(token),
                    }),
                    Some(_) => None,
                    None => return Ok(req.into_response(internal())),
                }
            }
            Ok(None) => None,
            Err(_) => return Ok(req.into_response(internal())),
        }
    } else {
        None
    };
    match identity {
        Some(identity) => {
            req.extensions_mut().insert(identity);
            Ok(next.call(req).await?.map_into_boxed_body())
        }
        None => Ok(req.into_response(unauthorized())),
    }
}
