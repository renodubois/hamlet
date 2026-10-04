use super::{
    operations::{self, CreateError},
    types::{Channel, ChannelList, CreateChannel},
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

#[utoipa::path(get, path = "/api/v1/channels", tag = "crate::channels", security(("bearer_auth" = [])),
    responses((status = 200, body = ChannelList), (status = 401, body = crate::http::error::ErrorBody), (status = 500, body = crate::http::error::ErrorBody)))]
pub(crate) async fn list_route(db: web::Data<AppState>) -> impl Responder {
    match operations::list(&db.db).await {
        Ok(channels) => HttpResponse::Ok().json(channels),
        Err(_) => internal(),
    }
}
