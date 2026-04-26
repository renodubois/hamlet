//! Channel HTTP handlers: list, create, reorder.

use std::collections::{HashMap, HashSet};

use actix_web::{Responder, get, post, put, web};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, QueryOrder, Set};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::broadcast::{BroadcastEvent, Broadcaster};
use crate::entity;
use crate::error::AppError;
use crate::util::generate_id;

pub const CHANNEL_TYPE_TEXT: &str = "text";
pub const CHANNEL_TYPE_VOICE: &str = "voice";
const CHANNEL_NAME_MAX_LEN: usize = 128;

#[derive(Clone, Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    #[serde(default, rename = "type")]
    pub channel_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ReorderChannelsRequest {
    pub ids: Vec<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChannelResponse {
    pub id: i64,
    pub name: String,
    pub position: i64,
    #[serde(rename = "type")]
    pub channel_type: String,
}

impl From<entity::channel::Model> for ChannelResponse {
    fn from(c: entity::channel::Model) -> Self {
        Self {
            id: c.id,
            name: c.name,
            position: c.position,
            channel_type: c.channel_type,
        }
    }
}

#[get("/channels")]
async fn get_channels(db: web::Data<DatabaseConnection>) -> Result<impl Responder, AppError> {
    let channels = entity::channel::Entity::find()
        .order_by_asc(entity::channel::Column::Position)
        .order_by_asc(entity::channel::Column::Id)
        .all(db.get_ref())
        .await?;

    let resp: Vec<ChannelResponse> = channels.into_iter().map(ChannelResponse::from).collect();
    Ok(web::Json(resp))
}

#[post("/channel")]
async fn create_channel(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    body: web::Json<CreateChannelRequest>,
    _user: AuthUser,
) -> Result<impl Responder, AppError> {
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > CHANNEL_NAME_MAX_LEN {
        return Err(AppError::InvalidRequest);
    }
    let channel_type = match body.channel_type.as_deref() {
        None | Some(CHANNEL_TYPE_TEXT) => CHANNEL_TYPE_TEXT,
        Some(CHANNEL_TYPE_VOICE) => CHANNEL_TYPE_VOICE,
        Some(_) => return Err(AppError::InvalidRequest),
    };

    let max_position = entity::channel::Entity::find()
        .order_by_desc(entity::channel::Column::Position)
        .one(db.get_ref())
        .await?
        .map(|c| c.position);
    let next_position = max_position.map(|p| p + 1).unwrap_or(0);

    let new_channel = entity::channel::ActiveModel {
        id: Set(generate_id()),
        name: Set(name.to_owned()),
        position: Set(next_position),
        channel_type: Set(channel_type.to_owned()),
    };
    let inserted = new_channel.insert(db.get_ref()).await?;

    let resp = ChannelResponse::from(inserted);
    broadcaster
        .publish(&BroadcastEvent::ChannelCreated(resp.clone()))
        .await?;

    Ok(web::Json(resp))
}

#[put("/channels/order")]
async fn reorder_channels(
    db: web::Data<DatabaseConnection>,
    broadcaster: web::Data<Broadcaster>,
    body: web::Json<ReorderChannelsRequest>,
    _user: AuthUser,
) -> Result<impl Responder, AppError> {
    let existing = entity::channel::Entity::find().all(db.get_ref()).await?;

    // The request must reference exactly the full set of existing channels
    // with no duplicates or omissions — partial reorders are ambiguous.
    let existing_ids: HashSet<i64> = existing.iter().map(|c| c.id).collect();
    let request_ids_unique: HashSet<i64> = body.ids.iter().copied().collect();
    if body.ids.len() != existing.len() || request_ids_unique != existing_ids {
        return Err(AppError::InvalidRequest);
    }

    let mut by_id: HashMap<i64, entity::channel::Model> =
        existing.into_iter().map(|c| (c.id, c)).collect();

    let mut updated: Vec<entity::channel::Model> = Vec::with_capacity(body.ids.len());
    for (idx, id) in body.ids.iter().enumerate() {
        let model = by_id.remove(id).ok_or(AppError::InvalidRequest)?;
        let mut active: entity::channel::ActiveModel = model.into();
        active.position = Set(idx as i64);
        let saved = active.update(db.get_ref()).await?;
        updated.push(saved);
    }

    let channels: Vec<ChannelResponse> = updated.into_iter().map(ChannelResponse::from).collect();
    broadcaster
        .publish(&BroadcastEvent::ChannelsReordered(channels.clone()))
        .await?;

    Ok(web::Json(channels))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_channels)
        .service(create_channel)
        .service(reorder_channels);
}
