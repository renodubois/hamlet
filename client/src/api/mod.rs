//! Canonical server communication owner. Callers use bound server/authenticated clients.

mod auth;
mod channels;
mod client;
mod error;
mod events;
mod messages;
mod types;
mod wire;

pub use client::{
    AuthenticatedClient, Authentication, HttpTransport, ServerClient, validate_server,
};
pub use error::ApiError;
#[allow(
    unused_imports,
    reason = "API stream is wired into conversation by #69"
)]
pub(crate) use events::{EventStream, LiveEvent, StreamError};
pub use types::{Channel, Message, Page, User};
pub type ApiFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) use super::client::{RequestAdapter, Response};
}

#[cfg(test)]
#[path = "tests/binding.rs"]
mod binding_tests;
#[cfg(test)]
#[path = "tests/http.rs"]
mod tests;
