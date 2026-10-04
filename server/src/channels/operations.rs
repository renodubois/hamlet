use super::types::{Channel, ChannelList, CreateChannel};
use crate::{
    live_updates::{EventHub, PreparedEvent},
    new_id,
};
use hamlet_protocol::Event;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};

#[cfg(test)]
#[path = "tests/publication.rs"]
mod tests;

fn valid(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b' ' || b == b'-' || b == b'_')
}

pub(super) enum CreateError {
    Invalid,
    Duplicate,
    Internal,
}

pub(super) async fn create(
    db: &DatabaseConnection,
    events: &EventHub,
    input: &CreateChannel,
) -> Result<Channel, CreateError> {
    let db = db.clone();
    let events = events.clone();
    let input = CreateChannel {
        name: input.name.clone(),
        kind: input.kind,
    };
    // A reset HTTP connection can drop its handler while SQLite's worker commits.
    // Keep this channel write and its synchronous notification in one owned task.
    #[cfg(test)]
    let control = tests::CONTROL.try_with(Clone::clone).ok();
    tokio::spawn(async move {
        #[cfg(test)]
        if let Some(control) = control {
            return tests::CONTROL
                .scope(control, create_owned(&db, &events, &input))
                .await;
        }
        create_owned(&db, &events, &input).await
    })
    .await
    .unwrap_or(Err(CreateError::Internal))
}

// Supervise the uncertain write interval even if the HTTP waiter is gone or the
// owned task unwinds/is aborted. Unexpected failure halts delivery until restart,
// including fresh streams: SQLite may still finish after an aborted await.
struct PendingChannelWrite<'a>(Option<&'a EventHub>);

impl Drop for PendingChannelWrite<'_> {
    fn drop(&mut self) {
        if let Some(events) = self.0 {
            events.interrupt();
        }
    }
}

async fn create_owned(
    db: &DatabaseConnection,
    events: &EventHub,
    input: &CreateChannel,
) -> Result<Channel, CreateError> {
    let name = input.name.trim();
    if !valid(name) {
        return Err(CreateError::Invalid);
    }

    // Retry up to 5 times if the ID collides
    for _ in 0..5 {
        #[cfg(not(test))]
        let id = new_id();
        #[cfg(test)]
        let id = tests::next_id();
        let channel = Channel {
            id: id.to_string(),
            name: name.into(),
            kind: input.kind,
        };
        let event = PreparedEvent::new(&Event::ChannelCreated {
            channel: channel.clone(),
        })
        .map_err(|_| CreateError::Internal)?;
        let mut pending = PendingChannelWrite(Some(events));
        match db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO channels (id, name, name_key, type) VALUES (?, ?, ?, ?)",
                [
                    id.into(),
                    name.to_owned().into(),
                    name.to_ascii_lowercase().into(),
                    input.kind.as_str().into(),
                ],
            ))
            .await
        {
            Ok(_) => {
                #[cfg(test)]
                tests::after_insert();
                events.notify(event);
                pending.0 = None;
                return Ok(channel);
            }
            Err(error) => {
                pending.0 = None;
                let msg = error.to_string();
                if msg.contains("channels.name_key") {
                    return Err(CreateError::Duplicate);
                }
                if msg.contains("channels.id") {
                    continue;
                }
                return Err(CreateError::Internal);
            }
        }
    }
    Err(CreateError::Internal)
}

pub(super) async fn list(db: &DatabaseConnection) -> Result<ChannelList, DbErr> {
    let rows = db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT id, name, type FROM channels ORDER BY name_key ASC, id ASC",
        ))
        .await?;
    let items = rows
        .into_iter()
        .map(|row| -> Result<Channel, DbErr> {
            Ok(Channel {
                id: row.try_get::<i64>("", "id")?.to_string(),
                name: row.try_get("", "name")?,
                kind: super::types::parse_channel_type(&row.try_get::<String>("", "type")?)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ChannelList { items })
}
