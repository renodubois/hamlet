pub use hamlet_protocol::{Channel, ChannelType};
use sea_orm::DbErr;
use serde::{Deserialize, Serialize};

pub(super) fn parse_channel_type(value: &str) -> Result<ChannelType, DbErr> {
    match value {
        "text" => Ok(ChannelType::Text),
        _ => Err(DbErr::Custom("unknown channel type".into())),
    }
}
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateChannel {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ChannelType,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct ChannelList {
    pub items: Vec<Channel>,
}
