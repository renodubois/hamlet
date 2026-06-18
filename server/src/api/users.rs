//! Authenticated public user directory search.
//!
//! This module exposes the narrow user DTO that mention autocomplete and
//! mention hydration can share without leaking private account state.

use actix_web::{Responder, get, web};
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::{Deserialize, Serialize};

use crate::api::avatars::avatar_url;
use crate::auth::AuthUser;
use crate::entity;
use crate::error::AppError;

pub const USER_SEARCH_DEFAULT_LIMIT: u64 = 20;
pub const USER_SEARCH_MAX_LIMIT: u64 = 50;
const USER_SEARCH_MIN_LIMIT: u64 = 1;

#[derive(Clone, Debug, Deserialize)]
pub struct UserSearchQuery {
    #[serde(default, alias = "query")]
    pub q: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PublicUserResponse {
    pub id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl From<entity::user::Model> for PublicUserResponse {
    fn from(user: entity::user::Model) -> Self {
        Self {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            avatar_url: avatar_url(user.avatar_path.as_deref(), user.avatar_updated_at),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum UserSearchRank {
    Exact,
    Prefix,
    Substring,
    Fuzzy,
    EmptyQuery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum UserSearchFieldRank {
    Username,
    DisplayName,
    EmptyQuery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct UserSearchScore {
    rank: UserSearchRank,
    field: UserSearchFieldRank,
}

struct ScoredUser {
    score: UserSearchScore,
    username_key: String,
    id: i64,
    user: PublicUserResponse,
}

#[get("/users")]
async fn search_users(
    db: web::Data<DatabaseConnection>,
    _user: AuthUser,
    query: web::Query<UserSearchQuery>,
) -> Result<impl Responder, AppError> {
    let needle = normalize_search_query(query.q.as_deref().unwrap_or_default());
    let limit = bounded_search_limit(query.limit);

    let users = entity::user::Entity::find().all(db.get_ref()).await?;
    let mut matches: Vec<ScoredUser> = users
        .into_iter()
        .filter_map(|user| {
            let score = score_user(&user, &needle)?;
            let username_key = user.username.to_lowercase();
            Some(ScoredUser {
                score,
                username_key,
                id: user.id,
                user: PublicUserResponse::from(user),
            })
        })
        .collect();

    matches.sort_by(|a, b| {
        a.score
            .cmp(&b.score)
            .then_with(|| a.username_key.cmp(&b.username_key))
            .then_with(|| a.id.cmp(&b.id))
    });

    let users = matches
        .into_iter()
        .take(limit)
        .map(|matched| matched.user)
        .collect::<Vec<_>>();

    Ok(web::Json(users))
}

fn normalize_search_query(query: &str) -> String {
    query.trim().to_lowercase()
}

fn bounded_search_limit(limit: Option<u64>) -> usize {
    let bounded = limit
        .unwrap_or(USER_SEARCH_DEFAULT_LIMIT)
        .clamp(USER_SEARCH_MIN_LIMIT, USER_SEARCH_MAX_LIMIT);
    bounded as usize
}

fn score_user(user: &entity::user::Model, needle: &str) -> Option<UserSearchScore> {
    if needle.is_empty() {
        return Some(UserSearchScore {
            rank: UserSearchRank::EmptyQuery,
            field: UserSearchFieldRank::EmptyQuery,
        });
    }

    let username = user.username.to_lowercase();
    let username_score = score_text(&username, needle).map(|rank| UserSearchScore {
        rank,
        field: UserSearchFieldRank::Username,
    });
    let display_name_score = user
        .display_name
        .as_deref()
        .map(str::to_lowercase)
        .and_then(|display_name| score_text(&display_name, needle))
        .map(|rank| UserSearchScore {
            rank,
            field: UserSearchFieldRank::DisplayName,
        });

    [username_score, display_name_score]
        .into_iter()
        .flatten()
        .min()
}

fn score_text(candidate: &str, needle: &str) -> Option<UserSearchRank> {
    if candidate == needle {
        Some(UserSearchRank::Exact)
    } else if candidate.starts_with(needle) {
        Some(UserSearchRank::Prefix)
    } else if candidate.contains(needle) {
        Some(UserSearchRank::Substring)
    } else if fuzzy_match(candidate, needle) {
        Some(UserSearchRank::Fuzzy)
    } else {
        None
    }
}

fn fuzzy_match(candidate: &str, needle: &str) -> bool {
    let mut remaining = needle.chars();
    let Some(mut current) = remaining.next() else {
        return true;
    };

    for c in candidate.chars() {
        if c == current {
            let Some(next) = remaining.next() else {
                return true;
            };
            current = next;
        }
    }

    false
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(search_users);
}
