pub(crate) mod handlers;
mod operations;
mod types;

use crate::http::error::problem;
use actix_web::{http::StatusCode, web};
use handlers::{history_route, post_route};

pub(crate) fn routes(cfg: &mut web::ServiceConfig) {
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
