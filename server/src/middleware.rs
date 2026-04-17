use actix_web::{
    Error, HttpMessage,
    body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    middleware::Next,
    web,
};
use sea_orm::DatabaseConnection;

use crate::auth;

pub async fn require_auth<B: MessageBody>(
    req: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<EitherBody<B>>, Error> {
    let Some(db) = req.app_data::<web::Data<DatabaseConnection>>().cloned() else {
        let res = actix_web::HttpResponse::InternalServerError().finish();
        return Ok(req.into_response(res).map_into_right_body());
    };

    let token = req.cookie(auth::SESSION_COOKIE).map(|c| c.value().to_owned());

    let Some(token) = token else {
        let res = actix_web::HttpResponse::Unauthorized().finish();
        return Ok(req.into_response(res).map_into_right_body());
    };

    match auth::validate_session(db.get_ref(), &token).await {
        Ok(user) => {
            req.extensions_mut().insert(user);
            next.call(req).await.map(|res| res.map_into_left_body())
        }
        Err(_) => {
            let res = actix_web::HttpResponse::Unauthorized().finish();
            Ok(req.into_response(res).map_into_right_body())
        }
    }
}
