//! HTTP-facing handlers, grouped by resource. Each sub-module owns its
//! request/response DTOs, its handlers, and a `configure(cfg)` function
//! that registers its routes. `crate::startup::configure_app` wires them
//! together with the right auth scopes.

pub mod attachments;
pub mod auth;
pub mod avatars;
pub mod channels;
pub mod emoji;
pub mod messages;
pub mod voice;
