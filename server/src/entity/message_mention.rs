//! `SeaORM` Entity — semantic user mentions attached to messages.
//!
//! Message text stores durable `<@user_id>` markers. This table stores the
//! queryable message-to-user edges, one row per message/user pair, with the
//! first marker position preserved for predictable response hydration.

use sea_orm::entity::prelude::*;
use serde::Serialize;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "message_mention")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    pub message_id: i64,
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
