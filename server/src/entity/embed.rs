//! `SeaORM` Entity — message embeds (link previews for URLs posted in
//! messages). One row per URL; messages with no URLs or no resolvable
//! metadata simply have zero rows.
//!
//! `embed_type` is the oEmbed response kind (`"link" | "photo" | "video" |
//! "rich"`). When we were able to extract an iframe from an oEmbed
//! provider's response, `iframe_url` holds the https src and `iframe_width`
//! / `iframe_height` carry the provider's preferred dimensions (used for
//! aspect-ratio sizing client-side). Link-type embeds leave the iframe
//! fields null.

use sea_orm::entity::prelude::*;
use serde::Serialize;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "embed")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    pub message_id: i64,
    #[sea_orm(column_type = "Text")]
    pub url: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub title: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub description: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub image_url: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub site_name: Option<String>,
    #[sea_orm(column_type = "Text")]
    pub embed_type: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub iframe_url: Option<String>,
    #[sea_orm(nullable)]
    pub iframe_width: Option<i32>,
    #[sea_orm(nullable)]
    pub iframe_height: Option<i32>,
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
