pub(crate) mod handlers;
mod operations;
mod types;

use crate::http::error::problem;
use actix_web::{http::StatusCode, web};

pub(crate) use handlers::{create_route, delete_route, list_route, rename_route};

pub(crate) fn routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/channels")
            .route(web::post().to(create_route))
            .route(web::get().to(list_route))
            .default_service(web::to(|| async {
                problem(
                    StatusCode::METHOD_NOT_ALLOWED,
                    "method_not_allowed",
                    "Method not allowed",
                )
            })),
    );
    cfg.service(
        web::resource("/channels/{id}")
            .route(web::patch().to(rename_route))
            .route(web::delete().to(delete_route))
            .default_service(web::to(|| async {
                problem(
                    StatusCode::METHOD_NOT_ALLOWED,
                    "method_not_allowed",
                    "Method not allowed",
                )
            })),
    );
}
