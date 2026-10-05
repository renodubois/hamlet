pub use hamlet_protocol::{Author, CreateMessage, History, Message};
use serde::Deserialize;

#[derive(Deserialize, utoipa::IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct HistoryQuery {
    /// Number of messages per page (1–100, default 50).
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<u16>,
    /// Opaque cursor from a previous response.
    pub before: Option<String>,
}
