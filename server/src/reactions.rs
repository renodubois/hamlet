//! Message reaction domain helpers.
//!
//! This module owns native/custom emoji validation, idempotent add/remove
//! mutation, and batch summary loading so HTTP handlers do not need to know the
//! storage details.

use std::collections::{HashMap, HashSet};

use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};

use crate::api::emoji::emoji_url;
use crate::entity;
use crate::error::AppError;
use crate::util::{generate_id, now_unix_micros};

const NATIVE_KIND: &str = "native";
const CUSTOM_KIND: &str = "custom";

const SUPPORTED_NATIVE_EMOJIS: &[&str] = &[
    "😀", "😃", "😄", "😁", "😆", "😅", "😂", "🙂", "🙃", "😉", "😊", "😇", "❤️", "💛", "💚", "💙",
    "💜", "💔", "😍", "😘", "😋", "😛", "😜", "😎", "🤔", "😐", "🙄", "😮", "😴", "😢", "😭", "😠",
    "😱", "😷", "✨", "🔥", "💯", "👋", "✋", "👌", "👍", "👎", "👊", "✊", "👏", "🙌", "🙏", "💪",
    "🐶", "🐱", "🐭", "🐹", "🐰", "🐻", "🐼", "🐯", "🐮", "🐷", "🐸", "🐵", "🐝", "🐢", "🌲", "🌴",
    "🌵", "🌹", "🌻", "🌈", "🍎", "🍊", "🍋", "🍌", "🍉", "🍇", "🍓", "🍒", "🍍", "🍅", "🌽", "🍞",
    "🍔", "🍟", "🍕", "🌮", "🍿", "🍣", "🍜", "🍦", "🍩", "🎂", "☕", "🍺", "⚽", "🏀", "🏈", "⚾",
    "🎾", "🎮", "🎲", "🎯", "🎨", "🎤", "🎧", "🎸", "☀️", "☁️", "⚡", "❄️", "🌊", "🚗", "🚌", "🚲",
    "✈️", "🚀", "🏠", "🏢", "🏫", "⌚", "📱", "💻", "📷", "📺", "⏰", "💡", "🔑", "🔒", "🔨", "📎",
    "✂️", "📝", "📚", "📌", "✅", "❌", "❓", "❗",
];

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ReactionRequest {
    Native { emoji: String },
    Custom { emoji_id: i64 },
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ReactionSummary {
    Native {
        emoji: String,
        count: u64,
        me_reacted: bool,
        reactors: Vec<String>,
    },
    Custom {
        emoji_id: i64,
        name: String,
        image_url: String,
        animated: bool,
        deleted_at: Option<i64>,
        count: u64,
        me_reacted: bool,
        reactors: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReactorPreview {
    user_id: i64,
    name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NormalizedReaction {
    kind: String,
    emoji: String,
    key: String,
}

#[derive(Clone, Debug)]
struct SummaryAccumulator {
    kind: String,
    emoji: String,
    key: String,
    count: u64,
    me_reacted: bool,
    reactors: Vec<ReactorPreview>,
}

fn custom_reaction_key(emoji_id: i64) -> String {
    format!("{CUSTOM_KIND}:{emoji_id}")
}

fn parse_custom_reaction_key(key: &str) -> Option<i64> {
    key.strip_prefix("custom:")?.parse().ok()
}

fn validate_native_emoji(emoji: &str) -> Result<(), AppError> {
    if SUPPORTED_NATIVE_EMOJIS.contains(&emoji) {
        Ok(())
    } else {
        Err(AppError::InvalidRequest)
    }
}

async fn normalize_reaction(
    db: &DatabaseConnection,
    request: &ReactionRequest,
    require_active_custom: bool,
) -> Result<NormalizedReaction, AppError> {
    match request {
        ReactionRequest::Native { emoji } => {
            validate_native_emoji(emoji)?;
            Ok(NormalizedReaction {
                kind: NATIVE_KIND.to_owned(),
                emoji: emoji.clone(),
                key: format!("{NATIVE_KIND}:{emoji}"),
            })
        }
        ReactionRequest::Custom { emoji_id } => {
            if *emoji_id <= 0 {
                return Err(AppError::InvalidRequest);
            }

            if require_active_custom {
                let emoji = entity::emoji::Entity::find_by_id(*emoji_id)
                    .one(db)
                    .await?
                    .ok_or(AppError::InvalidRequest)?;
                if emoji.deleted_at.is_some() {
                    return Err(AppError::InvalidRequest);
                }
            }

            Ok(NormalizedReaction {
                kind: CUSTOM_KIND.to_owned(),
                emoji: emoji_id.to_string(),
                key: custom_reaction_key(*emoji_id),
            })
        }
    }
}

async fn validate_reactable_message(
    db: &DatabaseConnection,
    message_id: i64,
) -> Result<entity::message::Model, AppError> {
    let message = entity::message::Entity::find_by_id(message_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;

    if message.deleted_at.is_some() {
        return Err(AppError::NotFound);
    }

    let channel = entity::channel::Entity::find_by_id(message.channel_id)
        .one(db)
        .await?
        .ok_or(AppError::NoChannelFound)?;
    if channel.channel_type != "text" {
        return Err(AppError::InvalidRequest);
    }

    Ok(message)
}

pub async fn add_reaction(
    db: &DatabaseConnection,
    message_id: i64,
    user_id: i64,
    request: &ReactionRequest,
) -> Result<Vec<ReactionSummary>, AppError> {
    validate_reactable_message(db, message_id).await?;
    let normalized = normalize_reaction(db, request, true).await?;

    let existing = entity::message_reaction::Entity::find()
        .filter(entity::message_reaction::Column::MessageId.eq(message_id))
        .filter(entity::message_reaction::Column::UserId.eq(user_id))
        .filter(entity::message_reaction::Column::EmojiKey.eq(normalized.key.clone()))
        .one(db)
        .await?;

    if existing.is_none() {
        entity::message_reaction::ActiveModel {
            id: Set(generate_id()),
            message_id: Set(message_id),
            user_id: Set(user_id),
            emoji_kind: Set(normalized.kind),
            emoji: Set(normalized.emoji),
            emoji_key: Set(normalized.key),
            created_at: Set(now_unix_micros()),
        }
        .insert(db)
        .await?;
    }

    load_reaction_summaries(db, &[message_id], user_id)
        .await
        .map(|mut grouped| grouped.remove(&message_id).unwrap_or_default())
}

pub async fn remove_reaction(
    db: &DatabaseConnection,
    message_id: i64,
    user_id: i64,
    request: &ReactionRequest,
) -> Result<Vec<ReactionSummary>, AppError> {
    validate_reactable_message(db, message_id).await?;
    let normalized = normalize_reaction(db, request, false).await?;

    entity::message_reaction::Entity::delete_many()
        .filter(entity::message_reaction::Column::MessageId.eq(message_id))
        .filter(entity::message_reaction::Column::UserId.eq(user_id))
        .filter(entity::message_reaction::Column::EmojiKey.eq(normalized.key))
        .exec(db)
        .await?;

    load_reaction_summaries(db, &[message_id], user_id)
        .await
        .map(|mut grouped| grouped.remove(&message_id).unwrap_or_default())
}

async fn load_users_for_reactions(
    db: &DatabaseConnection,
    rows: &[entity::message_reaction::Model],
) -> Result<HashMap<i64, entity::user::Model>, AppError> {
    let user_ids: HashSet<i64> = rows.iter().map(|row| row.user_id).collect();

    if user_ids.is_empty() {
        return Ok(HashMap::new());
    }

    Ok(entity::user::Entity::find()
        .filter(entity::user::Column::Id.is_in(user_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|user| (user.id, user))
        .collect())
}

async fn load_custom_emojis_for_reactions(
    db: &DatabaseConnection,
    rows: &[entity::message_reaction::Model],
) -> Result<HashMap<i64, entity::emoji::Model>, AppError> {
    let emoji_ids: HashSet<i64> = rows
        .iter()
        .filter(|row| row.emoji_kind == CUSTOM_KIND)
        .filter_map(|row| parse_custom_reaction_key(&row.emoji_key))
        .collect();

    if emoji_ids.is_empty() {
        return Ok(HashMap::new());
    }

    Ok(entity::emoji::Entity::find()
        .filter(entity::emoji::Column::Id.is_in(emoji_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|emoji| (emoji.id, emoji))
        .collect())
}

fn user_reactor_name(user: &entity::user::Model) -> String {
    user.display_name
        .as_deref()
        .filter(|display_name| !display_name.trim().is_empty())
        .unwrap_or(&user.username)
        .to_owned()
}

fn capped_reactor_preview(reactors: &[ReactorPreview], current_user_id: i64) -> Vec<String> {
    const MAX_REACTOR_PREVIEW_NAMES: usize = 5;

    let mut names = Vec::with_capacity(MAX_REACTOR_PREVIEW_NAMES);
    if reactors
        .iter()
        .any(|reactor| reactor.user_id == current_user_id)
    {
        names.push("You".to_owned());
    }

    for reactor in reactors {
        if names.len() >= MAX_REACTOR_PREVIEW_NAMES {
            break;
        }
        if reactor.user_id != current_user_id {
            names.push(reactor.name.clone());
        }
    }

    names
}

pub async fn load_reaction_summaries(
    db: &DatabaseConnection,
    message_ids: &[i64],
    current_user_id: i64,
) -> Result<HashMap<i64, Vec<ReactionSummary>>, AppError> {
    if message_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = entity::message_reaction::Entity::find()
        .filter(entity::message_reaction::Column::MessageId.is_in(message_ids.iter().copied()))
        .order_by_asc(entity::message_reaction::Column::MessageId)
        .order_by_asc(entity::message_reaction::Column::CreatedAt)
        .order_by_asc(entity::message_reaction::Column::Id)
        .all(db)
        .await?;

    let users = load_users_for_reactions(db, &rows).await?;
    let custom_emojis = load_custom_emojis_for_reactions(db, &rows).await?;
    let mut out: HashMap<i64, Vec<SummaryAccumulator>> = HashMap::new();
    for row in rows {
        let reactor = ReactorPreview {
            user_id: row.user_id,
            name: users
                .get(&row.user_id)
                .map(user_reactor_name)
                .unwrap_or_else(|| "Unknown user".to_owned()),
        };
        let row_user_id = row.user_id;
        let summary_key = if row.emoji_kind == CUSTOM_KIND {
            row.emoji_key.clone()
        } else {
            format!("{}:{}", row.emoji_kind, row.emoji)
        };
        let summaries = out.entry(row.message_id).or_default();
        if let Some(summary) = summaries
            .iter_mut()
            .find(|summary| summary.kind == row.emoji_kind && summary.key == summary_key)
        {
            summary.count += 1;
            summary.me_reacted = summary.me_reacted || row_user_id == current_user_id;
            summary.reactors.push(reactor);
        } else {
            summaries.push(SummaryAccumulator {
                kind: row.emoji_kind,
                emoji: row.emoji,
                key: summary_key,
                count: 1,
                me_reacted: row_user_id == current_user_id,
                reactors: vec![reactor],
            });
        }
    }

    Ok(out
        .into_iter()
        .map(|(message_id, summaries)| {
            (
                message_id,
                summaries
                    .into_iter()
                    .filter_map(|summary| {
                        let reactors = capped_reactor_preview(&summary.reactors, current_user_id);
                        match summary.kind.as_str() {
                            NATIVE_KIND => Some(ReactionSummary::Native {
                                emoji: summary.emoji,
                                count: summary.count,
                                me_reacted: summary.me_reacted,
                                reactors,
                            }),
                            CUSTOM_KIND => {
                                let emoji_id = parse_custom_reaction_key(&summary.key)?;
                                let emoji = custom_emojis.get(&emoji_id)?;
                                Some(ReactionSummary::Custom {
                                    emoji_id,
                                    name: emoji.name.clone(),
                                    image_url: emoji_url(&emoji.image_path, emoji.updated_at),
                                    animated: emoji.animated,
                                    deleted_at: emoji.deleted_at,
                                    count: summary.count,
                                    me_reacted: summary.me_reacted,
                                    reactors,
                                })
                            }
                            _ => None,
                        }
                    })
                    .collect(),
            )
        })
        .collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn validates_supported_native_emoji_only() {
        assert!(validate_native_emoji("👍").is_ok());
        assert!(validate_native_emoji("🫠").is_err());
        assert!(validate_native_emoji("👍👍").is_err());
    }

    #[test]
    fn deserializes_discriminated_reaction_requests() {
        assert_eq!(
            serde_json::from_value::<ReactionRequest>(serde_json::json!({
                "kind": "native",
                "emoji": "👍"
            }))
            .unwrap(),
            ReactionRequest::Native {
                emoji: "👍".to_owned()
            }
        );
        assert_eq!(
            serde_json::from_value::<ReactionRequest>(serde_json::json!({
                "kind": "custom",
                "emoji_id": 42
            }))
            .unwrap(),
            ReactionRequest::Custom { emoji_id: 42 }
        );
        assert!(
            serde_json::from_value::<ReactionRequest>(serde_json::json!({
                "kind": "native",
                "emoji_id": 42
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<ReactionRequest>(serde_json::json!({
                "kind": "custom",
                "emoji": "👍"
            }))
            .is_err()
        );
    }
}
