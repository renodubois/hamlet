//! Runtime bootstrap helpers for durable application defaults.
//!
//! These helpers assume the database connection has already gone through the
//! normal schema initialization path. They should not create or migrate schema.

use sea_orm::{
    ActiveModelTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait, Set,
    TransactionError, TransactionTrait,
};

use crate::api::channels::{CHANNEL_TYPE_TEXT, CHANNEL_TYPE_VOICE};
use crate::entity;
use crate::util::generate_id;

/// Result of attempting to create the built-in starter channels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DefaultChannelBootstrapOutcome {
    /// The channel table was empty and the default text/voice channels were inserted.
    Created,
    /// At least one channel already existed, so the existing channel list was left untouched.
    SkippedExistingChannels,
}

/// Ensure a fresh workspace starts with a usable text and voice channel.
///
/// This intentionally only inserts defaults when the channel table is empty.
/// Once a workspace has any channels, the user's channel list is authoritative:
/// repeated startup must not duplicate `general` / `voice` or reintroduce them
/// after an operator customizes the list.
pub async fn bootstrap_default_channels(
    db: &DatabaseConnection,
) -> Result<DefaultChannelBootstrapOutcome, DbErr> {
    db.transaction(|txn| {
        Box::pin(async move {
            let existing_channels = entity::channel::Entity::find().count(txn).await?;
            if existing_channels > 0 {
                return Ok(DefaultChannelBootstrapOutcome::SkippedExistingChannels);
            }

            let general = entity::channel::ActiveModel {
                id: Set(generate_id()),
                name: Set("general".to_owned()),
                position: Set(0),
                channel_type: Set(CHANNEL_TYPE_TEXT.to_owned()),
            }
            .insert(txn)
            .await?;
            crate::read_state::ensure_channel_read_state_baselines(txn, general.id).await?;

            entity::channel::ActiveModel {
                id: Set(generate_id()),
                name: Set("voice".to_owned()),
                position: Set(1),
                channel_type: Set(CHANNEL_TYPE_VOICE.to_owned()),
            }
            .insert(txn)
            .await?;

            Ok(DefaultChannelBootstrapOutcome::Created)
        })
    })
    .await
    .map_err(|error| match error {
        TransactionError::Connection(error) | TransactionError::Transaction(error) => error,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::sync::atomic::{AtomicU64, Ordering};

    use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, QueryOrder, Set};

    use super::*;
    use crate::database::{connect_database_url, connect_initialized_database_url};

    #[actix_web::test]
    async fn fresh_database_gets_general_and_voice_channels() {
        let db = initialized_db().await;

        let outcome = bootstrap_default_channels(&db).await.unwrap();

        assert_eq!(outcome, DefaultChannelBootstrapOutcome::Created);
        let channels = ordered_channels(&db).await;
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].name, "general");
        assert_eq!(channels[0].position, 0);
        assert_eq!(channels[0].channel_type, CHANNEL_TYPE_TEXT);
        assert_eq!(channels[1].name, "voice");
        assert_eq!(channels[1].position, 1);
        assert_eq!(channels[1].channel_type, CHANNEL_TYPE_VOICE);
    }

    #[actix_web::test]
    async fn repeated_bootstrap_does_not_duplicate_defaults() {
        let db = initialized_db().await;

        assert_eq!(
            bootstrap_default_channels(&db).await.unwrap(),
            DefaultChannelBootstrapOutcome::Created
        );
        let first_run_channels = ordered_channels(&db).await;

        assert_eq!(
            bootstrap_default_channels(&db).await.unwrap(),
            DefaultChannelBootstrapOutcome::SkippedExistingChannels
        );
        let second_run_channels = ordered_channels(&db).await;

        assert_eq!(second_run_channels, first_run_channels);
    }

    #[actix_web::test]
    async fn existing_custom_channel_list_is_left_unchanged() {
        let db = initialized_db().await;
        let custom_channel = entity::channel::ActiveModel {
            id: Set(generate_id()),
            name: Set("lobby".to_owned()),
            position: Set(42),
            channel_type: Set(CHANNEL_TYPE_TEXT.to_owned()),
        }
        .insert(&db)
        .await
        .unwrap();

        assert_eq!(
            bootstrap_default_channels(&db).await.unwrap(),
            DefaultChannelBootstrapOutcome::SkippedExistingChannels
        );

        assert_eq!(ordered_channels(&db).await, vec![custom_channel]);
    }

    #[actix_web::test]
    async fn bootstrap_assumes_schema_was_already_initialized() {
        let db = connect_database_url(&unique_memory_url()).await.unwrap();

        assert!(bootstrap_default_channels(&db).await.is_err());
    }

    async fn initialized_db() -> DatabaseConnection {
        connect_initialized_database_url(&unique_memory_url())
            .await
            .unwrap()
    }

    async fn ordered_channels(db: &DatabaseConnection) -> Vec<entity::channel::Model> {
        entity::channel::Entity::find()
            .order_by_asc(entity::channel::Column::Position)
            .order_by_asc(entity::channel::Column::Id)
            .all(db)
            .await
            .unwrap()
    }

    fn unique_memory_url() -> String {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("sqlite:file:hamlet_bootstrap_test_{n}?mode=memory&cache=shared")
    }
}
