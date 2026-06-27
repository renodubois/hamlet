//! Durable per-user channel read-state helpers.
//!
//! The public API stays intentionally compact: callers can ensure baseline rows
//! for migration/user/channel creation and derive authenticated snapshot
//! summaries from the authoritative cursor table.

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, EntityTrait, Statement};
use serde::Serialize;

use crate::api::channels::CHANNEL_TYPE_TEXT;
use crate::entity;
use crate::error::AppError;
use crate::util::now_unix_micros;

pub const EMPTY_CURSOR_CREATED_AT: i64 = 0;
pub const EMPTY_CURSOR_MESSAGE_ID: i64 = 0;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReadStateSummary {
    pub channel_id: i64,
    pub has_unread: bool,
    pub mention_count: i64,
    pub last_read_created_at: i64,
    pub last_read_message_id: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkReadResult {
    pub summary: ReadStateSummary,
    pub advanced: bool,
}

pub fn message_is_after_cursor(
    message_created_at: i64,
    message_id: i64,
    cursor_created_at: i64,
    cursor_message_id: i64,
) -> bool {
    message_created_at > cursor_created_at
        || (message_created_at == cursor_created_at && message_id > cursor_message_id)
}

pub async fn ensure_all_read_state_baselines<C>(db: &C) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    insert_missing_read_states(db, None, None).await
}

pub async fn ensure_user_read_state_baselines<C>(db: &C, user_id: i64) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    insert_missing_read_states(db, Some(user_id), None).await
}

pub async fn ensure_channel_read_state_baselines<C>(db: &C, channel_id: i64) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    insert_missing_read_states(db, None, Some(channel_id)).await
}

pub async fn read_state_snapshot(
    db: &DatabaseConnection,
    user_id: i64,
) -> Result<Vec<ReadStateSummary>, DbErr> {
    read_state_summaries(db, user_id, None).await
}

pub async fn read_state_summary_for_channel(
    db: &DatabaseConnection,
    user_id: i64,
    channel_id: i64,
) -> Result<Option<ReadStateSummary>, DbErr> {
    let mut summaries = read_state_summaries(db, user_id, Some(channel_id)).await?;
    Ok(summaries.pop())
}

pub async fn mark_channel_read(
    db: &DatabaseConnection,
    user_id: i64,
    channel_id: i64,
    last_visible_message_id: i64,
) -> Result<MarkReadResult, AppError> {
    let channel = entity::channel::Entity::find_by_id(channel_id)
        .one(db)
        .await?
        .ok_or(AppError::NoChannelFound)?;
    if channel.channel_type != CHANNEL_TYPE_TEXT {
        return Err(AppError::InvalidRequest);
    }

    let message = entity::message::Entity::find_by_id(last_visible_message_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    if message.channel_id != channel_id || message.parent_id.is_some() {
        return Err(AppError::InvalidRequest);
    }

    ensure_user_read_state_baselines(db, user_id).await?;
    let now = now_unix_micros();
    let sql = format!(
        r#"UPDATE "user_channel_read_state"
SET
    "last_read_created_at" = {created_at},
    "last_read_message_id" = {message_id},
    "updated_at" = {now}
WHERE "user_id" = {user_id}
  AND "channel_id" = {channel_id}
  AND (
      "last_read_created_at" < {created_at}
      OR (
          "last_read_created_at" = {created_at}
          AND "last_read_message_id" < {message_id}
      )
  )"#,
        created_at = message.created_at,
        message_id = message.id,
    );
    let result = db
        .execute_raw(Statement::from_string(DbBackend::Sqlite, sql))
        .await?;
    let summary = read_state_summary_for_channel(db, user_id, channel_id)
        .await?
        .ok_or_else(|| {
            AppError::Internal("missing read-state summary after mark-read".to_owned())
        })?;

    Ok(MarkReadResult {
        summary,
        advanced: result.rows_affected() > 0,
    })
}

async fn read_state_summaries(
    db: &DatabaseConnection,
    user_id: i64,
    channel_id: Option<i64>,
) -> Result<Vec<ReadStateSummary>, DbErr> {
    ensure_user_read_state_baselines(db, user_id).await?;
    let channel_filter = channel_id
        .map(|id| format!(r#"AND "c"."id" = {id}"#))
        .unwrap_or_default();

    let sql = format!(
        r#"SELECT
    "c"."id" AS "channel_id",
    "r"."last_read_created_at" AS "last_read_created_at",
    "r"."last_read_message_id" AS "last_read_message_id",
    "r"."updated_at" AS "updated_at",
    EXISTS (
        SELECT 1
        FROM "message" AS "m"
        WHERE "m"."channel_id" = "c"."id"
          AND "m"."parent_id" IS NULL
          AND "m"."deleted_at" IS NULL
          AND "m"."user_id" <> {user_id}
          AND (
              "m"."created_at" > "r"."last_read_created_at"
              OR (
                  "m"."created_at" = "r"."last_read_created_at"
                  AND "m"."id" > "r"."last_read_message_id"
              )
          )
        LIMIT 1
    ) AS "has_unread",
    (
        SELECT COUNT(DISTINCT "m"."id")
        FROM "message" AS "m"
        INNER JOIN "message_initial_mention" AS "initial"
            ON "initial"."message_id" = "m"."id"
           AND "initial"."user_id" = {user_id}
        INNER JOIN "message_mention" AS "mm"
            ON "mm"."message_id" = "m"."id"
           AND "mm"."user_id" = {user_id}
        WHERE "m"."channel_id" = "c"."id"
          AND "m"."parent_id" IS NULL
          AND "m"."deleted_at" IS NULL
          AND "m"."user_id" <> {user_id}
          AND (
              "m"."created_at" > "r"."last_read_created_at"
              OR (
                  "m"."created_at" = "r"."last_read_created_at"
                  AND "m"."id" > "r"."last_read_message_id"
              )
          )
    ) AS "mention_count"
FROM "channel" AS "c"
INNER JOIN "user_channel_read_state" AS "r"
    ON "r"."channel_id" = "c"."id"
   AND "r"."user_id" = {user_id}
WHERE "c"."channel_type" = '{CHANNEL_TYPE_TEXT}'
  {channel_filter}
ORDER BY "c"."position" ASC, "c"."id" ASC"#
    );

    let rows = db
        .query_all_raw(Statement::from_string(DbBackend::Sqlite, sql))
        .await?;

    rows.into_iter()
        .map(|row| {
            let has_unread: i64 = row.try_get("", "has_unread")?;
            Ok(ReadStateSummary {
                channel_id: row.try_get("", "channel_id")?,
                has_unread: has_unread != 0,
                mention_count: row.try_get("", "mention_count")?,
                last_read_created_at: row.try_get("", "last_read_created_at")?,
                last_read_message_id: row.try_get("", "last_read_message_id")?,
                updated_at: row.try_get("", "updated_at")?,
            })
        })
        .collect()
}

async fn insert_missing_read_states<C>(
    db: &C,
    user_id: Option<i64>,
    channel_id: Option<i64>,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let now = now_unix_micros();
    let user_filter = user_id
        .map(|id| format!(r#"AND "u"."id" = {id}"#))
        .unwrap_or_default();
    let channel_filter = channel_id
        .map(|id| format!(r#"AND "c"."id" = {id}"#))
        .unwrap_or_default();

    let sql = format!(
        r#"INSERT INTO "user_channel_read_state" (
    "user_id",
    "channel_id",
    "last_read_created_at",
    "last_read_message_id",
    "updated_at"
)
SELECT
    "u"."id",
    "c"."id",
    COALESCE((
        SELECT "m"."created_at"
        FROM "message" AS "m"
        WHERE "m"."channel_id" = "c"."id"
          AND "m"."parent_id" IS NULL
        ORDER BY "m"."created_at" DESC, "m"."id" DESC
        LIMIT 1
    ), {EMPTY_CURSOR_CREATED_AT}),
    COALESCE((
        SELECT "m"."id"
        FROM "message" AS "m"
        WHERE "m"."channel_id" = "c"."id"
          AND "m"."parent_id" IS NULL
        ORDER BY "m"."created_at" DESC, "m"."id" DESC
        LIMIT 1
    ), {EMPTY_CURSOR_MESSAGE_ID}),
    {now}
FROM "user" AS "u"
CROSS JOIN "channel" AS "c"
WHERE "c"."channel_type" = '{CHANNEL_TYPE_TEXT}'
  {user_filter}
  {channel_filter}
  AND NOT EXISTS (
      SELECT 1
      FROM "user_channel_read_state" AS "r"
      WHERE "r"."user_id" = "u"."id"
        AND "r"."channel_id" = "c"."id"
  )"#
    );

    db.execute_raw(Statement::from_string(DbBackend::Sqlite, sql))
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_comparison_uses_created_at_then_message_id() {
        assert!(message_is_after_cursor(20, 1, 10, 999));
        assert!(message_is_after_cursor(20, 11, 20, 10));
        assert!(!message_is_after_cursor(20, 10, 20, 10));
        assert!(!message_is_after_cursor(19, 999, 20, 10));
    }
}
