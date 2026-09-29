//! Existing private-view assertions remain until the view ownership tickets.
//! All network fixtures now use the same bound request adapter as production clients.

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
#[path = "polling.rs"]
mod polling;
#[path = "protected_binding.rs"]
mod protected_binding;
#[path = "saved_login.rs"]
mod saved_login;

use super::Hamlet;
use crate::api::{ApiError as AuthError, ApiFuture, HttpTransport};
use crate::persistence::{
    Outcome, Persistence,
    tests::{Controlled, Shared},
};
use crate::runtime::runtime;
use bound_auth::{BoundAuth, Request, RequestAdapter, Response, StatusCode};
use gpui_kit::TestSupportExt as _;
use gpui_kit::base::SelectableText;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, FocusHandle, Focusable as _, InteractiveElement as _, IntoElement,
    ListAlignment, ListState, MouseButton, ParentElement as _, Render, SharedString, Styled as _,
    TestAppContext, Window, div, list, px,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

// Preserve old fixture startup (no restore or automatic polling); real lifecycle tests use open.
impl Hamlet {
    fn new(window: &mut Window, cx: &mut Context<Self>, api: Arc<dyn RequestAdapter>) -> Self {
        let execution =
            crate::runtime::Execution::controlled(cx.background_executor().clone(), 1_800_000_000);
        Self::with_dependencies(
            window,
            cx,
            HttpTransport::with_adapter(api),
            crate::persistence::Config::default(),
            None,
            execution,
        )
    }
}

struct TestAuth;
impl RequestAdapter for TestAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, AuthError>> {
        BoundAuth.execute(request)
    }
}

// Fixture responses deliberately use wire representations, so decoding remains under API ownership.
fn message_json(message: crate::api::Message) -> serde_json::Value {
    serde_json::json!({"id":message.id, "channel_id":message.channel_id,
        "author":{"id":message.author_id,"display_name":message.author_name},
        "text":message.text, "created_at":message.created_at})
}
fn page_response(result: Result<crate::api::Page, AuthError>) -> Result<Response, AuthError> {
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
    result: Result<serde_json::Value, AuthError>,
    success: StatusCode,
) -> Result<Response, AuthError> {
    let (status, body) = match result {
        Ok(body) => (success, body.to_string()),
        Err(AuthError::Unavailable) => return Err(AuthError::Unavailable),
        Err(AuthError::InvalidResponse) => (success, "malformed".into()),
        Err(error) => (
            match error {
                AuthError::AlreadyInvalid | AuthError::InvalidCredentials => {
                    StatusCode::UNAUTHORIZED
                }
                AuthError::InvalidInput => StatusCode::BAD_REQUEST,
                AuthError::Conflict => StatusCode::CONFLICT,
                AuthError::NotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
            serde_json::json!({"error":{"code": match error {
                AuthError::InvalidInput => "bad_request",
                AuthError::Conflict => "conflict",
                AuthError::NotFound => "not_found",
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
    async_channel::Sender<Result<crate::api::Message, AuthError>>,
);
struct SendAuth(Arc<Mutex<Vec<Sent>>>);
impl RequestAdapter for SendAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, AuthError>> {
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

type PageReply = async_channel::Sender<Result<crate::api::Page, AuthError>>;
struct PagedAuth(std::sync::mpsc::Sender<(Option<String>, PageReply)>);
impl RequestAdapter for PagedAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, AuthError>> {
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
