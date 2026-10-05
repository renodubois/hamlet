use super::types::{Author, History, Message};
use crate::{
    http::auth::UserIdentity,
    live_updates::{EventHub, PreparedEvent},
    new_id,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, SecondsFormat, Utc};
use hamlet_protocol::Event;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, QueryResult, Statement};
use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "tests/publication.rs"]
mod tests;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    created_at: String,
    id: i64,
}
impl Cursor {
    fn encode(message: &Message) -> String {
        let payload = serde_json::to_vec(&Self {
            created_at: message
                .created_at
                .to_rfc3339_opts(SecondsFormat::Nanos, true),
            id: message.id.parse().expect("issued message ID"),
        })
        .expect("cursor serialization");
        URL_SAFE_NO_PAD.encode(payload)
    }
    fn decode(value: &str) -> Option<Self> {
        if value.len() > 512 || value.is_empty() {
            return None;
        }
        let bytes = URL_SAFE_NO_PAD.decode(value).ok()?;
        let cursor: Self = serde_json::from_slice(&bytes).ok()?;
        if !(100_000_000_000_000..=999_999_999_999_999).contains(&cursor.id)
            || DateTime::parse_from_rfc3339(&cursor.created_at)
                .ok()?
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Nanos, true)
                != cursor.created_at
        {
            return None;
        }
        Some(cursor)
    }
}

async fn channel_exists(db: &DatabaseConnection, id: i64) -> Result<bool, DbErr> {
    Ok(db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT id FROM channels WHERE id = ?",
            [id.into()],
        ))
        .await?
        .is_some())
}
fn decode(row: QueryResult) -> Result<Message, DbErr> {
    let created: String = row.try_get("", "created_at")?;
    Ok(Message {
        id: row.try_get::<i64>("", "id")?.to_string(),
        channel_id: row.try_get::<i64>("", "channel_id")?.to_string(),
        author: Author {
            id: row.try_get::<i64>("", "author_id")?.to_string(),
            display_name: row.try_get("", "username")?,
        },
        text: row.try_get("", "text")?,
        created_at: DateTime::parse_from_rfc3339(&created)
            .map_err(|_| DbErr::Custom("invalid message timestamp".into()))?
            .with_timezone(&Utc),
    })
}

pub(super) enum PostError {
    Invalid,
    Missing,
    Internal,
}
pub(super) async fn post(
    db: &DatabaseConnection,
    events: &EventHub,
    channel_id: i64,
    author: &UserIdentity,
    text: &str,
) -> Result<Message, PostError> {
    if text.trim().is_empty() || text.chars().count() > 4000 {
        return Err(PostError::Invalid);
    }
    if !channel_exists(db, channel_id)
        .await
        .map_err(|_| PostError::Internal)?
    {
        return Err(PostError::Missing);
    }
    for _ in 0..5 {
        #[cfg(not(test))]
        let id = new_id();
        #[cfg(test)]
        let id = tests::next_id();
        #[cfg(not(test))]
        let created_at = Utc::now();
        #[cfg(test)]
        let created_at = tests::now();
        let message = Message {
            id: id.to_string(),
            channel_id: channel_id.to_string(),
            author: Author {
                id: author.id.to_string(),
                display_name: author.username.clone(),
            },
            text: text.to_owned(),
            created_at,
        };
        let event = PreparedEvent::new(&Event::MessageCreated {
            message: message.clone(),
        })
        .map_err(|_| PostError::Internal)?;
        let created = message
            .created_at
            .to_rfc3339_opts(SecondsFormat::Nanos, true);
        match db.execute_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
            "INSERT INTO messages (id, channel_id, author_id, text, created_at) VALUES (?, ?, ?, ?, ?)",
            [id.into(), channel_id.into(), author.id.into(), text.to_owned().into(), created.into()])).await {
            Ok(_) => {
                #[cfg(test)]
                tests::after_insert();
                events.notify(event);
                return Ok(message);
            }
            Err(error) => {
                if error.to_string().contains("messages.id") {
                    continue;
                }
                return Err(PostError::Internal);
            }
        }
    }
    Err(PostError::Internal)
}

pub(super) enum HistoryError {
    Invalid,
    Missing,
    Internal,
}
pub(super) async fn history(
    db: &DatabaseConnection,
    channel_id: i64,
    limit: u16,
    before: Option<&str>,
) -> Result<History, HistoryError> {
    if !(1..=100).contains(&limit) {
        return Err(HistoryError::Invalid);
    }
    // Missing channels take precedence over bad cursors.
    if !channel_exists(db, channel_id)
        .await
        .map_err(|_| HistoryError::Internal)?
    {
        return Err(HistoryError::Missing);
    }
    let cursor = before
        .map(|value| Cursor::decode(value).ok_or(HistoryError::Invalid))
        .transpose()?;
    let rows = if let Some(cursor) = cursor {
        db.query_all_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
            "SELECT m.id, m.channel_id, m.author_id, m.text, m.created_at, u.username FROM messages m JOIN users u ON u.id = m.author_id WHERE m.channel_id = ? AND (m.created_at < ? OR (m.created_at = ? AND m.id < ?)) ORDER BY m.created_at DESC, m.id DESC LIMIT ?",
            [channel_id.into(), cursor.created_at.clone().into(), cursor.created_at.into(), cursor.id.into(), (i64::from(limit) + 1).into()])).await
    } else {
        db.query_all_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
            "SELECT m.id, m.channel_id, m.author_id, m.text, m.created_at, u.username FROM messages m JOIN users u ON u.id = m.author_id WHERE m.channel_id = ? ORDER BY m.created_at DESC, m.id DESC LIMIT ?",
            [channel_id.into(), (i64::from(limit) + 1).into()])).await
    }.map_err(|_| HistoryError::Internal)?;
    let has_more = rows.len() > usize::from(limit);
    let items = rows
        .into_iter()
        .take(usize::from(limit))
        .map(decode)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| HistoryError::Internal)?;
    let next_cursor = if has_more {
        items.last().map(Cursor::encode)
    } else {
        None
    };
    Ok(History { items, next_cursor })
}
