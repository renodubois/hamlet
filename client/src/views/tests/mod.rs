//! Cross-view scenarios use production startup, Kit Root and semantic controls.
//! Controlled HTTP responses still pass through the bound API's request/decoding path.

#[path = "authentication.rs"]
mod authentication;
#[path = "bound_auth.rs"]
mod bound_auth;
#[path = "channels.rs"]
mod channels;
#[path = "composer.rs"]
mod composer;
#[path = "execution.rs"]
mod execution;
#[path = "history.rs"]
mod history;
#[path = "journeys.rs"]
mod journeys;
#[path = "live_updates.rs"]
mod live_updates;
#[path = "protected_binding.rs"]
mod protected_binding;
#[path = "session_lifecycle.rs"]
mod session_lifecycle;
mod support;

use support::{open_controlled, open_with_streams};

use crate::api::{ApiError, ApiFuture, HttpTransport};
use crate::runtime::runtime;
use crate::storage::Persistence;
use bound_auth::{BoundAuth, Request, RequestAdapter, Response, StatusCode};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, Window, px};
use std::sync::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

// Fixture responses deliberately use wire representations, so decoding remains under API ownership.
fn message_json(message: crate::api::Message) -> serde_json::Value {
    serde_json::json!({"id":message.id, "channel_id":message.channel_id,
        "author":{"id":message.author_id,"display_name":message.author_name},
        "text":message.text, "created_at":message.created_at})
}
fn page_response(result: Result<crate::api::Page, ApiError>) -> Result<Response, ApiError> {
    wire_response(
        result.map(|page| {
            serde_json::json!({
                "items":page.items.into_iter().map(message_json).collect::<Vec<_>>(),
                "next_cursor":page.next_cursor
            })
        }),
        StatusCode::OK,
    )
}
fn wire_response(
    result: Result<serde_json::Value, ApiError>,
    success: StatusCode,
) -> Result<Response, ApiError> {
    let (status, body) = match result {
        Ok(body) => (success, body.to_string()),
        Err(ApiError::Unavailable) => return Err(ApiError::Unavailable),
        Err(ApiError::InvalidResponse) => (success, "malformed".into()),
        Err(error) => (
            match error {
                ApiError::AlreadyInvalid | ApiError::InvalidCredentials => StatusCode::UNAUTHORIZED,
                ApiError::InvalidInput => StatusCode::BAD_REQUEST,
                ApiError::Conflict => StatusCode::CONFLICT,
                ApiError::NotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
            serde_json::json!({"error":{"code": match error {
                ApiError::InvalidInput => "bad_request",
                ApiError::Conflict => "conflict",
                ApiError::NotFound => "not_found",
                _ => "unauthorized",
            }}})
            .to_string(),
        ),
    };
    Ok(Response::controlled(status, body))
}

type Sent = (
    String,
    String,
    async_channel::Sender<Result<crate::api::Message, ApiError>>,
);
struct SendAuth(Arc<Mutex<Vec<Sent>>>);
impl RequestAdapter for SendAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.method() != reqwest::Method::POST || !request.url().path().ends_with("/messages")
        {
            return BoundAuth.execute(request);
        }
        let id = request.url().path().split('/').nth(4).unwrap().to_owned();
        let body: serde_json::Value =
            serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
        let (tx, rx) = async_channel::bounded(1);
        self.0
            .lock()
            .unwrap()
            .push((id, body["text"].as_str().unwrap().into(), tx));
        Box::pin(async move {
            wire_response(
                rx.recv().await.unwrap().map(message_json),
                StatusCode::CREATED,
            )
        })
    }
}

type PageReply = async_channel::Sender<Result<crate::api::Page, ApiError>>;
struct PagedAuth(std::sync::mpsc::Sender<(Option<String>, PageReply)>);
impl RequestAdapter for PagedAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if !request.url().path().ends_with("/messages") {
            return BoundAuth.execute(request);
        }
        let before = request
            .url()
            .query_pairs()
            .find(|(key, _)| key == "before")
            .map(|(_, value)| value.into_owned());
        let tx = self.0.clone();
        Box::pin(async move {
            let (reply, rx) = async_channel::bounded(1);
            tx.send((before, reply)).unwrap();
            page_response(rx.recv().await.unwrap())
        })
    }
}

struct RaceAuth {
    pages: std::sync::mpsc::Sender<(Option<String>, PageReply)>,
    sends: Arc<Mutex<Vec<Sent>>>,
}
impl RequestAdapter for RaceAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.method() == reqwest::Method::POST {
            SendAuth(self.sends.clone()).execute(request)
        } else {
            PagedAuth(self.pages.clone()).execute(request)
        }
    }
}

// Read the real textarea's accessibility value without changing focus or selection.
// Clicking the background composer while a modal is open dismisses that modal.
fn composer_text(window: &mut Window, cx: &mut gpui_kit::App) -> String {
    window.render_frame(cx);
    let composer = window.find("composer");
    let inputs: Vec<_> = gpui_kit::base::test_support::snapshots(window)
        .into_iter()
        .filter(|control| control.path().starts_with(composer.path()) && control.value().is_some())
        .collect();
    assert_eq!(inputs.len(), 1, "composer must expose one textarea value");
    inputs[0].value().unwrap().to_owned()
}

fn advance(cx: &mut gpui_kit::VisualTestContext, seconds: u64) {
    cx.background_executor
        .advance_clock(Duration::from_secs(seconds));
    cx.run_until_parked();
}
