//! `SeaORM` Entity — durable per-user text-channel read cursors.

use sea_orm::entity::prelude::*;
use serde::Serialize;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "user_channel_read_state")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel_id: i64,
    pub last_read_created_at: i64,
    pub last_read_message_id: i64,
    pub updated_at: i64,
    #[serde(skip)]
    #[sea_orm(
        belongs_to,
        from = "user_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    pub user: HasOne<super::user::Entity>,
    #[serde(skip)]
    #[sea_orm(
        belongs_to,
        from = "channel_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    pub channel: HasOne<super::channel::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
