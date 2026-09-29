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
pub(super) struct WireChannel {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
}
#[derive(Deserialize)]
pub(super) struct WireHistory {
    pub items: Vec<WireMessage>,
    pub next_cursor: Option<String>,
}
#[derive(Deserialize)]
pub(super) struct WireMessage {
    pub id: String,
    pub channel_id: String,
    pub author: WireAuthor,
    pub text: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Deserialize)]
pub(super) struct WireAuthor {
    pub id: String,
    pub display_name: String,
}
