//! Canonical server communication owner. Callers use bound server/authenticated clients.

mod auth;
mod channels;
mod client;
mod error;
mod messages;
mod types;
mod wire;

pub use client::{
    AuthenticatedClient, Authentication, HttpTransport, ServerClient, validate_server,
};
pub use error::ApiError;
pub use types::{Channel, Message, Page, User};
pub type ApiFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) use super::client::{RequestAdapter, Response};
}

#[cfg(test)]
mod binding_tests;
#[cfg(test)]
mod tests;
