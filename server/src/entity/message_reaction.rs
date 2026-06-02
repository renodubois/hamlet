//! `SeaORM` Entity — native/custom reactions attached to messages.
//!
//! The v1 native-reaction flow stores one row per user/message/emoji reaction.
//! `emoji_key` is the normalized identity used by the domain layer for
//! idempotent add/remove operations; for native emoji it is `native:<glyph>`.

use sea_orm::entity::prelude::*;
use serde::Serialize;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "message_reaction")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    pub message_id: i64,
    pub user_id: i64,
    #[sea_orm(column_type = "Text")]
    pub emoji_kind: String,
    #[sea_orm(column_type = "Text")]
    pub emoji: String,
    #[sea_orm(column_type = "Text")]
    pub emoji_key: String,
    pub created_at: i64,
    #[serde(skip)]
    #[sea_orm(
        belongs_to,
        from = "message_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    pub message: HasOne<super::message::Entity>,
    #[serde(skip)]
    #[sea_orm(
        belongs_to,
        from = "user_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    pub user: HasOne<super::user::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
