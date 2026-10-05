//! Client-facing server data, distinct from wire DTOs where display shapes differ.

pub use hamlet_protocol::User;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Channel {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    pub author_id: String,
    pub author_name: String,
    pub text: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    // Server order: newest first, with the server's tie breaker intact.
    pub items: Vec<Message>,
    pub next_cursor: Option<String>,
}
