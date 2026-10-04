use crate::{
    AppState, digest,
    http::error::{internal, unauthorized},
};
use actix_web::{Error, HttpMessage, dev::ServiceRequest, middleware::Next, web};
use chrono::{DateTime, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

#[derive(Clone)]
pub(crate) struct Identity {
    pub user: UserIdentity,
    pub session: Session,
}
#[derive(Clone)]
pub(crate) struct UserIdentity {
    pub id: i64,
    pub username: String,
}

/// Private credential material retained for validation, never logged or serialized.
#[derive(Clone)]
pub(crate) struct Session {
    token_digest: String,
    expires_at: DateTime<Utc>,
}

impl Session {
    pub(crate) fn token_digest(&self) -> &str {
        &self.token_digest
    }
    pub(crate) fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
    pub(crate) async fn validate(&self, db: &DatabaseConnection) -> Result<Option<Identity>, ()> {
        validate_session(db, &self.token_digest).await
    }
}

/// One lookup/expiry policy for both HTTP handshakes and open streams.
async fn validate_session(
    db: &DatabaseConnection,
    token_digest: &str,
) -> Result<Option<Identity>, ()> {
    let row = db.query_one_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
        "SELECT u.id, u.username, s.expires_at FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token_digest = ?",
        [token_digest.into()])).await.map_err(|_| ())?;
    let Some(row) = row else {
        return Ok(None);
    };
    let id = row.try_get("", "id").map_err(|_| ())?;
    let username = row.try_get("", "username").map_err(|_| ())?;
    let expiry: String = row.try_get("", "expires_at").map_err(|_| ())?;
    let expires_at = DateTime::parse_from_rfc3339(&expiry)
        .map_err(|_| ())?
        .with_timezone(&Utc);
    if expires_at <= Utc::now() {
        return Ok(None);
    }
    Ok(Some(Identity {
        user: UserIdentity { id, username },
        session: Session {
            token_digest: token_digest.to_owned(),
            expires_at,
        },
    }))
}

/// Extracts a bearer token from a request, and validates what user it belongs to.
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
    let identity =
        if let (Some(token), Some(state)) = (token, req.app_data::<web::Data<AppState>>()) {
            match validate_session(&state.db, &digest(token)).await {
                Ok(identity) => identity,
                Err(()) => return Ok(req.into_response(internal())),
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
