pub(super) use hamlet_protocol::{Channel as WireChannel, Message as WireMessage};
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct WireUser {
    pub id: String,
    pub username: String,
}
#[derive(Deserialize)]
pub(super) struct WireLogin {
    pub user: WireUser,
    pub access_token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Deserialize)]
pub(super) struct WireError {
    pub error: WireErrorInfo,
}
#[derive(Deserialize)]
pub(super) struct WireErrorInfo {
    pub code: String,
}

#[derive(Deserialize)]
pub(super) struct WireChannels {
    pub items: Vec<WireChannel>,
}
#[derive(Deserialize)]
pub(super) struct WireHistory {
    pub items: Vec<WireMessage>,
    pub next_cursor: Option<String>,
}
