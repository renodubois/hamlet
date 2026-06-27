//! `SeaORM` Entity — immutable mention edges captured at message creation.
//!
//! Current message mentions live in `message_mention` and can change on edit.
//! This table preserves which users were mentioned when the message was first
//! created so unread notification badges can ignore edit-created mentions.

use sea_orm::entity::prelude::*;
use serde::Serialize;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "message_initial_mention")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub message_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i64,
    pub position: i32,
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
