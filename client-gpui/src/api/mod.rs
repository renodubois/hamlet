//! Canonical server communication owner. New callers use bound server/authenticated clients.
//! `legacy` is a temporary forwarding facade, not a second HTTP implementation.

mod auth;
mod channels;
mod client;
mod error;
pub(crate) mod legacy;
mod messages;
mod types;
mod wire;

pub use client::{
    AuthenticatedClient, Authentication, HttpTransport, ServerClient, validate_server,
};
pub use error::ApiError;
pub use types::{Channel, Message, Page, User};
pub type ApiFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

// Retire with legacy callers; canonical types and failures are API-owned.
pub use legacy::HttpAuth;
#[cfg(test)]
use {ApiError as AuthError, legacy::AuthApi};

#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) use super::client::{RequestAdapter, Response};
}

#[cfg(test)]
mod binding_tests;
#[cfg(test)]
mod tests;
