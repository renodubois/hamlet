use super::{
    operations::{self, HistoryError, PostError},
    types::{CreateMessage, History, HistoryQuery, Message},
};
use crate::{
    AppState,
    http::{
        auth::Identity,
        error::{bad_request, internal, problem},
    },
};
use actix_web::{Error, HttpMessage, HttpRequest, HttpResponse, Responder, http::StatusCode, web};

fn parse_channel(id: &str) -> Option<i64> {
    if id.len() != 15 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    id.parse().ok()
}
#[utoipa::path(post, path = "/api/v1/channels/{channel_id}/messages", tag = "crate::messages", security(("bearer_auth" = [])),
    params(("channel_id" = String, Path, description = "Decimal-string channel ID")), request_body = CreateMessage,
    responses((status = 201, body = Message), (status = 400, body = crate::http::error::ErrorBody),
        (status = 401, body = crate::http::error::ErrorBody), (status = 404, body = crate::http::error::ErrorBody), (status = 500, body = crate::http::error::ErrorBody)))]
pub(crate) async fn post_route(
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
    match operations::post(&db.db, channel_id, identity.user.id, &input.text).await {
        Ok(message) => HttpResponse::Created().json(message),
        Err(PostError::Invalid) => bad_request(),
        Err(PostError::Missing) => problem(StatusCode::NOT_FOUND, "not_found", "Channel not found"),
        Err(PostError::Internal) => internal(),
    }
}

#[utoipa::path(get, path = "/api/v1/channels/{channel_id}/messages", tag = "crate::messages", security(("bearer_auth" = [])),
    params(("channel_id" = String, Path, description = "Decimal-string channel ID"), HistoryQuery),
    responses((status = 200, body = History), (status = 400, body = crate::http::error::ErrorBody),
        (status = 401, body = crate::http::error::ErrorBody), (status = 404, body = crate::http::error::ErrorBody), (status = 500, body = crate::http::error::ErrorBody)))]
pub(crate) async fn history_route(
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
    match operations::history(
        &db.db,
        channel_id,
        query.limit.unwrap_or(50),
        query.before.as_deref(),
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
