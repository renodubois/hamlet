use super::{
    operations::{self, CreateError, DeleteError, RenameError},
    types::{Channel, ChannelList, CreateChannel, RenameChannel},
};
use crate::{
    AppState,
    http::error::{bad_request, internal, problem},
};
use actix_web::{Error, HttpResponse, Responder, http::StatusCode, web};

#[utoipa::path(post, path = "/api/v1/channels", tag = "crate::channels", security(("bearer_auth" = [])), request_body = CreateChannel,
    responses((status = 201, body = Channel), (status = 400, body = crate::http::error::ErrorBody),
        (status = 401, body = crate::http::error::ErrorBody), (status = 409, body = crate::http::error::ErrorBody), (status = 500, body = crate::http::error::ErrorBody)))]
pub(crate) async fn create_route(
    db: web::Data<AppState>,
    input: Result<web::Json<CreateChannel>, Error>,
) -> impl Responder {
    let Ok(input) = input else {
        return bad_request();
    };
    match operations::create(&db.db, &db.events, &input).await {
        Ok(channel) => HttpResponse::Created().json(channel),
        Err(CreateError::Invalid) => bad_request(),
        Err(CreateError::Duplicate) => {
            problem(StatusCode::CONFLICT, "conflict", "Channel already exists")
        }
        Err(CreateError::Internal) => internal(),
    }
}

#[utoipa::path(patch, path = "/api/v1/channels/{id}", tag = "crate::channels", security(("bearer_auth" = [])),
    params(("id" = String, Path, description = "Decimal-string channel ID")), request_body = RenameChannel,
    responses((status = 200, body = Channel), (status = 400, body = crate::http::error::ErrorBody),
        (status = 401, body = crate::http::error::ErrorBody), (status = 404, body = crate::http::error::ErrorBody),
        (status = 409, body = crate::http::error::ErrorBody), (status = 500, body = crate::http::error::ErrorBody)))]
pub(crate) async fn rename_route(
    db: web::Data<AppState>,
    path: web::Path<String>,
    input: Result<web::Json<RenameChannel>, Error>,
) -> impl Responder {
    if path.len() != 15 || !path.bytes().all(|b| b.is_ascii_digit()) {
        return bad_request();
    }
    let Ok(id) = path.parse::<i64>() else {
        return bad_request();
    };
    let Ok(input) = input else {
        return bad_request();
    };
    match operations::rename(&db.db, &db.events, id, &input).await {
        Ok(channel) => HttpResponse::Ok().json(channel),
        Err(RenameError::Invalid) => bad_request(),
        Err(RenameError::Missing) => {
            problem(StatusCode::NOT_FOUND, "not_found", "Channel not found")
        }
        Err(RenameError::Duplicate) => {
            problem(StatusCode::CONFLICT, "conflict", "Channel already exists")
        }
        Err(RenameError::Internal) => internal(),
    }
}

#[utoipa::path(delete, path = "/api/v1/channels/{id}", tag = "crate::channels", security(("bearer_auth" = [])),
    params(("id" = String, Path, description = "Decimal-string channel ID")),
    responses((status = 204, description = "Channel removed from normal use; conversation retained"),
        (status = 400, body = crate::http::error::ErrorBody), (status = 401, body = crate::http::error::ErrorBody),
        (status = 404, body = crate::http::error::ErrorBody), (status = 409, body = crate::http::error::ErrorBody),
        (status = 500, body = crate::http::error::ErrorBody)))]
pub(crate) async fn delete_route(
    db: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    if path.len() != 15 || !path.bytes().all(|b| b.is_ascii_digit()) {
        return bad_request();
    }
    let Ok(id) = path.parse::<i64>() else {
        return bad_request();
    };
    match operations::delete(&db.db, &db.events, id).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(DeleteError::Missing) => {
            problem(StatusCode::NOT_FOUND, "not_found", "Channel not found")
        }
        Err(DeleteError::LastChannel) => problem(
            StatusCode::CONFLICT,
            "conflict",
            "Cannot delete the last active channel",
        ),
        Err(DeleteError::Internal) => internal(),
    }
}

#[utoipa::path(get, path = "/api/v1/channels", tag = "crate::channels", security(("bearer_auth" = [])),
    responses((status = 200, body = ChannelList), (status = 401, body = crate::http::error::ErrorBody), (status = 500, body = crate::http::error::ErrorBody)))]
pub(crate) async fn list_route(db: web::Data<AppState>) -> impl Responder {
    match operations::list(&db.db).await {
        Ok(channels) => HttpResponse::Ok().json(channels),
        Err(_) => internal(),
    }
}
