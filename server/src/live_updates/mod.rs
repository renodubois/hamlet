use crate::{AppState, http::error::problem};
use actix_web::{HttpMessage, HttpRequest, HttpResponse, http::StatusCode, web};
mod hub;
mod stream;
pub use hub::{EventHub, PreparedEvent, Subscription};

pub(crate) fn routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/events")
            .route(web::get().to(events))
            .default_service(web::to(|| async {
                problem(
                    StatusCode::METHOD_NOT_ALLOWED,
                    "method_not_allowed",
                    "Method not allowed",
                )
            })),
    );
}

/// Fresh all-channel subscription: ready with {}, then change with tagged Event JSON;
/// heartbeat comments keep idle connections alive. No IDs or replay; Last-Event-ID
/// is ignored. Reconnect with ready/read/buffer reconciliation. EOF is not an
/// authoritative authentication rejection. See llm-docs/server/LIVE-UPDATES.md.
#[utoipa::path(get, path = "/api/v1/events", tag = "crate::live_updates",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "UTF-8 SSE: ready {}, change Event JSON, heartbeat comments. No replay.", body = String, content_type = "text/event-stream"),
        (status = 401, body = crate::http::error::ErrorBody),
        (status = 405, body = crate::http::error::ErrorBody),
        (status = 500, body = crate::http::error::ErrorBody)
    ))]
pub(crate) async fn events(state: web::Data<AppState>, request: HttpRequest) -> HttpResponse {
    let identity = request
        .extensions()
        .get::<crate::http::auth::Identity>()
        .cloned()
        .expect("protected route");
    let subscription =
        stream::LiveStream::new(state.events.subscribe(), state.db.clone(), identity.session);
    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Content-Encoding", "identity"))
        .insert_header(("Cache-Control", "no-cache, no-transform"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(futures_util::stream::unfold(
            subscription,
            |mut subscription| async move {
                subscription
                    .next_frame()
                    .await
                    .map(|frame| (Ok::<_, actix_web::Error>(frame), subscription))
            },
        ))
}
