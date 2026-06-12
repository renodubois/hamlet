//! `SeaORM` Entity — durable metadata for photo attachments that belong to
//! messages. The actual bytes live in private storage and are exposed through
//! authenticated attachment routes; this table stores only safe metadata and
//! relative storage paths for the full-size and thumbnail variants.

use sea_orm::entity::prelude::*;
use serde::Serialize;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "message_attachment")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    pub message_id: i64,
    pub position: i32,
    #[sea_orm(column_type = "Text")]
    pub content_type: String,
    pub byte_size: i64,
    pub width: i32,
    pub height: i32,
    #[sea_orm(column_type = "Text")]
    pub storage_path: String,
    #[sea_orm(column_type = "Text")]
    pub thumbnail_content_type: String,
    pub thumbnail_byte_size: i64,
    pub thumbnail_width: i32,
    pub thumbnail_height: i32,
    #[sea_orm(column_type = "Text")]
    pub thumbnail_storage_path: String,
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
}

impl ActiveModelBehavior for ActiveModel {}
