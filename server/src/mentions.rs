//! Parsing, validation, and hydration helpers for durable user mention markers.

use std::collections::{HashMap, HashSet};

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};

use crate::api::users::PublicUserResponse;
use crate::entity;
use crate::error::AppError;

pub const MAX_UNIQUE_MENTIONS_PER_MESSAGE: usize = 50;
pub const MAX_SAFE_USER_ID: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedMentionMarkers {
    pub user_ids: Vec<i64>,
    pub has_invalid_id: bool,
}

/// Extract well-formed `<@digits>` user mention markers in first-appearance
/// order while treating malformed marker-looking prose as ordinary text.
///
/// Repeated mentions of the same numeric user id are returned once. Numeric
/// markers that cannot become a safe positive JavaScript-round-trippable id
/// set `has_invalid_id` so the request can be rejected by validation.
pub fn parse_mention_markers(text: &str) -> ParsedMentionMarkers {
    let bytes = text.as_bytes();
    let mut cursor = 0usize;
    let mut user_ids = Vec::new();
    let mut seen_user_ids = HashSet::new();
    let mut has_invalid_id = false;

    while let Some(relative_start) = text[cursor..].find("<@") {
        let marker_start = cursor + relative_start;
        let digits_start = marker_start + 2;
        let mut digits_end = digits_start;

        while digits_end < bytes.len() && bytes[digits_end].is_ascii_digit() {
            digits_end += 1;
        }

        let is_well_formed =
            digits_end > digits_start && digits_end < bytes.len() && bytes[digits_end] == b'>';
        if !is_well_formed {
            cursor = digits_start;
            continue;
        }

        let raw_id = &text[digits_start..digits_end];
        match parse_safe_user_id(raw_id) {
            Some(user_id) => {
                if seen_user_ids.insert(user_id) {
                    user_ids.push(user_id);
                }
            }
            None => {
                has_invalid_id = true;
            }
        }

        cursor = digits_end + 1;
    }

    ParsedMentionMarkers {
        user_ids,
        has_invalid_id,
    }
}

fn parse_safe_user_id(raw_id: &str) -> Option<i64> {
    let mut value = 0i64;
    for digit in raw_id.bytes() {
        let digit = i64::from(digit.checked_sub(b'0')?);
        value = value.checked_mul(10)?.checked_add(digit)?;
        if value > MAX_SAFE_USER_ID {
            return None;
        }
    }

    if value <= 0 { None } else { Some(value) }
}

/// Parse and validate a message body's mentions, returning public user DTOs in
/// first-appearance order. All referenced users are loaded with one batch query.
pub async fn validate_message_mentions(
    db: &DatabaseConnection,
    text: &str,
) -> Result<Vec<PublicUserResponse>, AppError> {
    let parsed = parse_mention_markers(text);
    if parsed.has_invalid_id || parsed.user_ids.len() > MAX_UNIQUE_MENTIONS_PER_MESSAGE {
        return Err(AppError::InvalidRequest);
    }
    load_public_users_in_order(db, &parsed.user_ids, true).await
}

/// Hydrate already-persisted mention rows for a batch of message ids without
/// per-message user lookups. Missing users are skipped defensively; normal
/// database constraints should prevent them.
pub async fn load_mentions_for_messages(
    db: &DatabaseConnection,
    message_ids: &[i64],
) -> Result<HashMap<i64, Vec<PublicUserResponse>>, AppError> {
    if message_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = entity::message_mention::Entity::find()
        .filter(entity::message_mention::Column::MessageId.is_in(message_ids.iter().copied()))
        .order_by_asc(entity::message_mention::Column::MessageId)
        .order_by_asc(entity::message_mention::Column::Position)
        .order_by_asc(entity::message_mention::Column::UserId)
        .all(db)
        .await?;

    let mut mentioned_user_ids: Vec<i64> = rows.iter().map(|row| row.user_id).collect();
    mentioned_user_ids.sort_unstable();
    mentioned_user_ids.dedup();
    let users = load_public_users_by_id(db, &mentioned_user_ids).await?;

    let mut out: HashMap<i64, Vec<PublicUserResponse>> = HashMap::new();
    for row in rows {
        if let Some(user) = users.get(&row.user_id) {
            out.entry(row.message_id).or_default().push(user.clone());
        }
    }
    Ok(out)
}

async fn load_public_users_in_order(
    db: &DatabaseConnection,
    user_ids: &[i64],
    require_all: bool,
) -> Result<Vec<PublicUserResponse>, AppError> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }

    let users = load_public_users_by_id(db, user_ids).await?;
    if require_all && users.len() != user_ids.len() {
        return Err(AppError::InvalidRequest);
    }

    let mut out = Vec::with_capacity(user_ids.len());
    for user_id in user_ids {
        match users.get(user_id) {
            Some(user) => out.push(user.clone()),
            None if require_all => return Err(AppError::InvalidRequest),
            None => {}
        }
    }
    Ok(out)
}

async fn load_public_users_by_id(
    db: &DatabaseConnection,
    user_ids: &[i64],
) -> Result<HashMap<i64, PublicUserResponse>, AppError> {
    if user_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = entity::user::Entity::find()
        .filter(entity::user::Column::Id.is_in(user_ids.iter().copied()))
        .all(db)
        .await?;
    Ok(rows
        .into_iter()
        .map(|user| (user.id, PublicUserResponse::from(user)))
        .collect())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn parser_extracts_mentions_in_first_appearance_order_and_deduplicates_users() {
        let parsed = parse_mention_markers("hi <@42> then <@7> then <@42> and <@0007>");

        assert_eq!(parsed.user_ids, vec![42, 7]);
        assert!(!parsed.has_invalid_id);
    }

    #[test]
    fn parser_treats_malformed_marker_like_text_as_ordinary_text() {
        let parsed =
            parse_mention_markers("<@> <@abc> <@-1> <@123abc> <@123 <@@456> <@789.0> normal");

        assert_eq!(parsed.user_ids, Vec::<i64>::new());
        assert!(!parsed.has_invalid_id);
    }

    #[test]
    fn parser_flags_well_formed_numeric_markers_with_unsafe_ids() {
        let parsed = parse_mention_markers("bad <@0> and <@9007199254740992>");

        assert_eq!(parsed.user_ids, Vec::<i64>::new());
        assert!(parsed.has_invalid_id);
    }

    #[test]
    fn parser_accepts_the_largest_js_safe_positive_id() {
        let parsed = parse_mention_markers("<@9007199254740991>");

        assert_eq!(parsed.user_ids, vec![MAX_SAFE_USER_ID]);
        assert!(!parsed.has_invalid_id);
    }
}
