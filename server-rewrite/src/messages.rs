use crate::{AppState, Identity, bad_request, internal, new_id, problem};
use actix_web::{Error, HttpMessage, HttpRequest, HttpResponse, Responder, http::StatusCode, web};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, QueryResult, Statement};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

#[derive(Serialize, utoipa::ToSchema)]
pub struct Author {
    pub id: String,
    pub display_name: String,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    pub author: Author,
    pub text: String,
    pub created_at: DateTime<Utc>,
}
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateMessage {
    pub text: String,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct History {
    pub items: Vec<Message>,
    pub next_cursor: Option<String>,
}
#[derive(Deserialize, utoipa::IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct HistoryQuery {
    /// Number of messages per page (1–100, default 50).
    pub limit: Option<u16>,
    /// Opaque cursor from a previous response.
    pub before: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u8,
    channel_id: i64,
    created_at: String,
    id: i64,
}
impl Cursor {
    fn encode(message: &Message, key: &str) -> String {
        let payload = serde_json::to_vec(&Self {
            version: 1,
            channel_id: message.channel_id.parse().expect("issued channel ID"),
            created_at: message
                .created_at
                .to_rfc3339_opts(SecondsFormat::Nanos, true),
            id: message.id.parse().expect("issued message ID"),
        })
        .expect("cursor serialization");
        let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("HMAC key");
        mac.update(&payload);
        format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(payload),
            URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
        )
    }
    fn decode(value: &str, channel_id: i64, key: &str) -> Option<Self> {
        if value.len() > 512 || value.is_empty() {
            return None;
        }
        let (payload, signature) = value.split_once('.')?;
        let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
        let signature = URL_SAFE_NO_PAD.decode(signature).ok()?;
        let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).ok()?;
        mac.update(&bytes);
        mac.verify_slice(&signature).ok()?;
        let cursor: Self = serde_json::from_slice(&bytes).ok()?;
        if cursor.version != 1
            || cursor.channel_id != channel_id
            || !(100_000_000_000_000..=999_999_999_999_999).contains(&cursor.id)
            || DateTime::parse_from_rfc3339(&cursor.created_at)
                .ok()?
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Nanos, true)
                != cursor.created_at
        {
            return None;
        }
        Some(cursor)
    }
}

fn parse_channel(id: &str) -> Option<i64> {
    if id.len() != 15 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    id.parse().ok()
}
async fn channel_exists(db: &DatabaseConnection, id: i64) -> Result<bool, DbErr> {
    Ok(db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT id FROM channels WHERE id = ?",
            [id.into()],
        ))
        .await?
        .is_some())
}
fn decode(row: QueryResult) -> Result<Message, DbErr> {
    let created: String = row.try_get("", "created_at")?;
    Ok(Message {
        id: row.try_get::<i64>("", "id")?.to_string(),
        channel_id: row.try_get::<i64>("", "channel_id")?.to_string(),
        author: Author {
            id: row.try_get::<i64>("", "author_id")?.to_string(),
            display_name: row.try_get("", "username")?,
        },
        text: row.try_get("", "text")?,
        created_at: DateTime::parse_from_rfc3339(&created)
            .map_err(|_| DbErr::Custom("invalid message timestamp".into()))?
            .with_timezone(&Utc),
    })
}

pub enum PostError {
    Invalid,
    Missing,
    Internal,
}
pub async fn post(
    db: &DatabaseConnection,
    channel_id: i64,
    author_id: i64,
    text: &str,
) -> Result<Message, PostError> {
    if text.trim().is_empty() || text.chars().count() > 4000 {
        return Err(PostError::Invalid);
    }
    if !channel_exists(db, channel_id)
        .await
        .map_err(|_| PostError::Internal)?
    {
        return Err(PostError::Missing);
    }
    for _ in 0..5 {
        let id = new_id();
        let created = Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true);
        match db.execute_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
            "INSERT INTO messages (id, channel_id, author_id, text, created_at) VALUES (?, ?, ?, ?, ?)",
            [id.into(), channel_id.into(), author_id.into(), text.to_owned().into(), created.into()])).await {
            Ok(_) => {
                let row = db.query_one_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
                    "SELECT m.id, m.channel_id, m.author_id, m.text, m.created_at, u.username FROM messages m JOIN users u ON u.id = m.author_id WHERE m.id = ?",
                    [id.into()])).await.map_err(|_| PostError::Internal)?.ok_or(PostError::Internal)?;
                return decode(row).map_err(|_| PostError::Internal);
            }
            Err(e) if e.to_string().contains("messages.id") => continue,
            Err(_) => return Err(PostError::Internal),
        }
    }
    Err(PostError::Internal)
}

pub enum HistoryError {
    Invalid,
    Missing,
    Internal,
}
pub async fn history(
    db: &DatabaseConnection,
    channel_id: i64,
    limit: u16,
    before: Option<&str>,
    key: &str,
) -> Result<History, HistoryError> {
    if !(1..=100).contains(&limit) {
        return Err(HistoryError::Invalid);
    }
    // Missing channels take precedence over bad cursors.
    if !channel_exists(db, channel_id)
        .await
        .map_err(|_| HistoryError::Internal)?
    {
        return Err(HistoryError::Missing);
    }
    let cursor = before
        .map(|value| Cursor::decode(value, channel_id, key).ok_or(HistoryError::Invalid))
        .transpose()?;
    let rows = if let Some(cursor) = cursor {
        db.query_all_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
            "SELECT m.id, m.channel_id, m.author_id, m.text, m.created_at, u.username FROM messages m JOIN users u ON u.id = m.author_id WHERE m.channel_id = ? AND (m.created_at < ? OR (m.created_at = ? AND m.id < ?)) ORDER BY m.created_at DESC, m.id DESC LIMIT ?",
            [channel_id.into(), cursor.created_at.clone().into(), cursor.created_at.into(), cursor.id.into(), (i64::from(limit) + 1).into()])).await
    } else {
        db.query_all_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
            "SELECT m.id, m.channel_id, m.author_id, m.text, m.created_at, u.username FROM messages m JOIN users u ON u.id = m.author_id WHERE m.channel_id = ? ORDER BY m.created_at DESC, m.id DESC LIMIT ?",
            [channel_id.into(), (i64::from(limit) + 1).into()])).await
    }.map_err(|_| HistoryError::Internal)?;
    let has_more = rows.len() > usize::from(limit);
    let items = rows
        .into_iter()
        .take(usize::from(limit))
        .map(decode)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| HistoryError::Internal)?;
    let next_cursor = if has_more {
        items.last().map(|message| Cursor::encode(message, key))
    } else {
        None
    };
    Ok(History { items, next_cursor })
}

#[utoipa::path(post, path = "/api/v1/channels/{channel_id}/messages", security(("bearer_auth" = [])),
    params(("channel_id" = String, Path, description = "Decimal-string channel ID")), request_body = CreateMessage,
    responses((status = 201, body = Message), (status = 400, body = crate::ErrorBody),
        (status = 401, body = crate::ErrorBody), (status = 404, body = crate::ErrorBody), (status = 500, body = crate::ErrorBody)))]
async fn post_route(
    db: web::Data<AppState>,
    req: HttpRequest,
    path: web::Path<String>,
    input: Result<web::Json<CreateMessage>, Error>,
) -> impl Responder {
    let Some(channel_id) = parse_channel(&path) else {
        return bad_request();
    };
    let Ok(input) = input else {
        return bad_request();
    };
    let identity = req
        .extensions()
        .get::<Identity>()
        .cloned()
        .expect("protected scope");
    match post(&db.db, channel_id, identity.user.id, &input.text).await {
        Ok(message) => HttpResponse::Created().json(message),
        Err(PostError::Invalid) => bad_request(),
        Err(PostError::Missing) => problem(StatusCode::NOT_FOUND, "not_found", "Channel not found"),
        Err(PostError::Internal) => internal(),
    }
}

#[utoipa::path(get, path = "/api/v1/channels/{channel_id}/messages", security(("bearer_auth" = [])),
    params(("channel_id" = String, Path, description = "Decimal-string channel ID"), HistoryQuery),
    responses((status = 200, body = History), (status = 400, body = crate::ErrorBody),
        (status = 401, body = crate::ErrorBody), (status = 404, body = crate::ErrorBody), (status = 500, body = crate::ErrorBody)))]
async fn history_route(
    db: web::Data<AppState>,
    path: web::Path<String>,
    query: Result<web::Query<HistoryQuery>, Error>,
) -> impl Responder {
    let Some(channel_id) = parse_channel(&path) else {
        return bad_request();
    };
    let Ok(query) = query else {
        return bad_request();
    };
    match history(
        &db.db,
        channel_id,
        query.limit.unwrap_or(50),
        query.before.as_deref(),
        &db.cursor_key,
    )
    .await
    {
        Ok(history) => HttpResponse::Ok().json(history),
        Err(HistoryError::Invalid) => bad_request(),
        Err(HistoryError::Missing) => {
            problem(StatusCode::NOT_FOUND, "not_found", "Channel not found")
        }
        Err(HistoryError::Internal) => internal(),
    }
}

pub fn routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/channels/{channel_id}/messages")
            .route(web::post().to(post_route))
            .route(web::get().to(history_route))
            .default_service(web::to(|| async {
                problem(
                    StatusCode::METHOD_NOT_ALLOWED,
                    "method_not_allowed",
                    "Method not allowed",
                )
            })),
    );
}
