//! Canonical server communication owner. Callers use bound server/authenticated clients.

mod auth;
mod channels;
mod client;
mod error;
mod events;
mod messages;
mod types;

pub use client::{
    AuthenticatedClient, Authentication, HttpTransport, ServerClient, validate_server,
};
pub use error::ApiError;
pub(crate) use events::{LiveEvent, StreamError};
pub use types::{Channel, Message, Page, User};
pub type ApiFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) use super::client::{RequestAdapter, Response};
    pub(crate) use super::events::{StreamAdapter, StreamResponse};
}

#[cfg(test)]
#[path = "tests/support/http.rs"]
mod http_support;

#[cfg(test)]
#[path = "tests/binding.rs"]
mod binding_tests;
#[cfg(test)]
#[path = "tests/http.rs"]
mod tests;
