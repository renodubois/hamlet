use crate::{AppState, Identity, bad_request, internal, new_id, problem};
use actix_web::{Error, HttpMessage, HttpRequest, HttpResponse, Responder, http::StatusCode, web};
use chrono::{DateTime, SecondsFormat, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, QueryResult, Statement};
use serde::{Deserialize, Serialize};

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

pub async fn history(db: &DatabaseConnection, channel_id: i64) -> Result<Option<History>, DbErr> {
    if !channel_exists(db, channel_id).await? {
        return Ok(None);
    }
    let rows = db.query_all_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
        "SELECT m.id, m.channel_id, m.author_id, m.text, m.created_at, u.username FROM messages m JOIN users u ON u.id = m.author_id WHERE m.channel_id = ? ORDER BY m.created_at DESC, m.id DESC",
        [channel_id.into()])).await?;
    Ok(Some(History {
        items: rows
            .into_iter()
            .map(decode)
            .collect::<Result<Vec<_>, _>>()?,
        next_cursor: None,
    }))
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
    params(("channel_id" = String, Path, description = "Decimal-string channel ID")),
    responses((status = 200, body = History), (status = 400, body = crate::ErrorBody),
        (status = 401, body = crate::ErrorBody), (status = 404, body = crate::ErrorBody), (status = 500, body = crate::ErrorBody)))]
async fn history_route(db: web::Data<AppState>, path: web::Path<String>) -> impl Responder {
    let Some(channel_id) = parse_channel(&path) else {
        return bad_request();
    };
    match history(&db.db, channel_id).await {
        Ok(Some(history)) => HttpResponse::Ok().json(history),
        Ok(None) => problem(StatusCode::NOT_FOUND, "not_found", "Channel not found"),
        Err(_) => internal(),
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
