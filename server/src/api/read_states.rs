//! Authenticated read-state HTTP handlers.

use actix_web::{Responder, get, put, web};
use sea_orm::DatabaseConnection;
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::broadcast::{BroadcastEvent, Broadcaster};
use crate::error::AppError;
use crate::read_state;

#[derive(Clone, Debug, Deserialize)]
pub struct MarkReadRequest {
    pub last_visible_message_id: i64,
}

#[get("/read-states")]
async fn get_read_states(
    db: web::Data<DatabaseConnection>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let summaries = read_state::read_state_snapshot(db.get_ref(), user.id).await?;
    Ok(web::Json(summaries))
}

#[put("/channels/{channel_id}/read-state")]
async fn mark_channel_read(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    path: web::Path<i64>,
    body: web::Json<MarkReadRequest>,
    user: AuthUser,
) -> Result<impl Responder, AppError> {
    let result = read_state::mark_channel_read(
        db.get_ref(),
        user.id,
        path.into_inner(),
        body.last_visible_message_id,
    )
    .await?;
    if result.advanced {
        broadcaster
            .publish_to_user(
                user.id,
                &BroadcastEvent::ReadStateUpdated(result.summary.clone()),
            )
            .await?;
    }
    Ok(web::Json(result.summary))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_read_states).service(mark_channel_read);
}
