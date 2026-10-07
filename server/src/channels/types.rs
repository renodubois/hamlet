pub use hamlet_protocol::{Channel, ChannelList, ChannelType, CreateChannel, RenameChannel};
use sea_orm::DbErr;

pub(super) fn parse_channel_type(value: &str) -> Result<ChannelType, DbErr> {
    match value {
        "text" => Ok(ChannelType::Text),
        _ => Err(DbErr::Custom("unknown channel type".into())),
    }
}
