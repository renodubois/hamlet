use super::{
    operations::{self, LoginError, SignupError},
    types::{AuthResponse, Credentials, User},
};
use crate::{
    AppState,
    http::{
        auth::Identity,
        error::{bad_request, internal, problem, unauthorized},
    },
};
use actix_web::{Error, HttpMessage, HttpResponse, Responder, http::StatusCode, web};

#[utoipa::path(post, path = "/api/v1/auth/signup", tag = "crate", request_body = Credentials,
    responses((status = 201, body = AuthResponse), (status = 400, body = crate::http::error::ErrorBody),
        (status = 409, body = crate::http::error::ErrorBody), (status = 500, body = crate::http::error::ErrorBody)))]
pub(crate) async fn signup(
    db: web::Data<AppState>,
    input: Result<web::Json<Credentials>, Error>,
) -> impl Responder {
    let Ok(input) = input else {
        return bad_request();
    };
    match operations::register(&db.db, &input.username, &input.password).await {
        Ok(response) => HttpResponse::Created().json(response),
        Err(SignupError::Invalid) => bad_request(),
        Err(SignupError::Duplicate) => {
            problem(StatusCode::CONFLICT, "conflict", "Username already exists")
        }
        Err(SignupError::Internal) => internal(),
    }
}

#[utoipa::path(post, path = "/api/v1/auth/login", tag = "crate", request_body = Credentials,
    responses((status = 200, body = AuthResponse), (status = 400, body = crate::http::error::ErrorBody),
        (status = 401, body = crate::http::error::ErrorBody), (status = 500, body = crate::http::error::ErrorBody)))]
pub(crate) async fn login(
    db: web::Data<AppState>,
    input: Result<web::Json<Credentials>, Error>,
) -> impl Responder {
    let Ok(input) = input else {
        return bad_request();
    };
    match operations::login(&db.db, &input.username, &input.password).await {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(LoginError::Invalid) => unauthorized(),
        Err(LoginError::Internal) => internal(),
    }
}

#[utoipa::path(post, path = "/api/v1/auth/logout", tag = "crate", security(("bearer_auth" = [])),
    responses((status = 204), (status = 401, body = crate::http::error::ErrorBody), (status = 500, body = crate::http::error::ErrorBody)))]
pub(crate) async fn logout(db: web::Data<AppState>, req: actix_web::HttpRequest) -> impl Responder {
    let identity = req
        .extensions()
        .get::<Identity>()
        .cloned()
        .expect("protected scope");
    match operations::logout(&db.db, &identity.token_digest).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(_) => internal(),
    }
}

#[utoipa::path(get, path = "/api/v1/me", tag = "crate", security(("bearer_auth" = [])),
    responses((status = 200, body = User), (status = 401, body = crate::http::error::ErrorBody),
        (status = 500, body = crate::http::error::ErrorBody)))]
pub(crate) async fn me(req: actix_web::HttpRequest) -> impl Responder {
    let identity = req
        .extensions()
        .get::<Identity>()
        .cloned()
        .expect("protected scope");
    HttpResponse::Ok().json(User {
        id: identity.user.id.to_string(),
        username: identity.user.username,
    })
}
