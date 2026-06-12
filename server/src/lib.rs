//! Hamlet server library crate. Each module owns a focused concern; the
//! HTTP surface lives under `api`. `startup::configure_app` wires it all
//! together, and tests construct `AppDeps` and call `configure_app` to
//! exercise routes via `actix_web::test::init_service`.

pub mod api;
pub mod auth;
pub mod broadcast;
pub mod config;
pub mod database;
pub mod embeds;
pub mod entity;
pub mod error;
pub mod middleware;
mod photos;
pub mod reactions;
pub mod seed;
pub mod startup;
pub mod telemetry;
pub mod util;
pub mod voice;

// Stable surface for binaries and integration tests.
pub use api::attachments::AttachmentStorage;
pub use api::avatars::AvatarStorage;
pub use api::emoji::EmojiStorage;
pub use api::messages::EmbedFetcher;
pub use config::Config;
pub use database::connect_database;
pub use error::AppError;
pub use seed::seed_development_data;
pub use startup::{AppDeps, configure_app, deps_for_tests, start_server};
pub use util::{generate_id, now_unix_micros};
