use sea_orm::DbErr;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChannelType {
    Text,
}
impl ChannelType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
        }
    }
    pub(super) fn parse(value: &str) -> Result<Self, DbErr> {
        match value {
            "text" => Ok(Self::Text),
            _ => Err(DbErr::Custom("unknown channel type".into())),
        }
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
pub struct Channel {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ChannelType,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct ChannelList {
    pub items: Vec<Channel>,
}
