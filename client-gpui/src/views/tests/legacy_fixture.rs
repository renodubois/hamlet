//! Temporary fixture for mechanically relocated root suites, not a new testing seam.
//!
//! Suites remain descendants of app_shell so existing private assertions need no
//! visibility changes. Replace these fixtures/assertions at the owned interfaces
//! when session, conversation and child views migrate (see MIGRATION-48.md).

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
#[path = "saved_login.rs"]
mod saved_login;

use super::Hamlet;
use crate::persistence::{
    Outcome, Persistence,
    tests::{Controlled, Shared},
};
use crate::runtime::runtime;
use crate::session::{ApiFuture, AuthApi, AuthError, Login, User};
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

// Preserve the original headless constructor: no profile reads, provider, restore
// startup or poll timer. Both constructors use the same root initialization code.
impl Hamlet {
    fn new(window: &mut Window, cx: &mut Context<Self>, api: Arc<dyn AuthApi>) -> Self {
        let execution =
            crate::runtime::Execution::controlled(cx.background_executor().clone(), 1_800_000_000);
        Self::with_dependencies(
            window,
            cx,
            api,
            crate::persistence::Config::default(),
            None,
            execution,
        )
    }
}

struct TestAuth;
impl AuthApi for TestAuth {
    fn signup(
        &self,
        _: String,
        username: String,
        _: String,
    ) -> ApiFuture<Result<Login, AuthError>> {
        self.login(String::new(), username, String::new())
    }
    fn login(&self, _: String, username: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
        Box::pin(async move {
            Ok(Login {
                user: User {
                    id: "42".into(),
                    username,
                },
                token: "secret".into(),
                expires_at: 4_070_908_800,
            })
        })
    }
    fn logout(&self, _: String, _: String) -> ApiFuture<Result<(), AuthError>> {
        Box::pin(async { Ok(()) })
    }
    fn channels(
        &self,
        _: String,
        _: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Channel>, AuthError>> {
        Box::pin(async {
            Ok(vec![
                crate::conversation::Channel {
                    id: "000000000000001".into(),
                    name: "alpha".into(),
                },
                crate::conversation::Channel {
                    id: "000000000000002".into(),
                    name: "general".into(),
                },
            ])
        })
    }
    fn create_channel(
        &self,
        _: String,
        _: String,
        _: String,
    ) -> ApiFuture<Result<crate::conversation::Channel, AuthError>> {
        Box::pin(async { unreachable!() })
    }
    fn history(
        &self,
        _: String,
        _: String,
        id: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Message>, AuthError>> {
        Box::pin(async move {
            Ok(vec![crate::conversation::Message {
                id: id.clone(),
                channel_id: id.clone(),
                author_id: "42".into(),
                author_name: "Ada".into(),
                text: if id.ends_with('1') {
                    "first line\nsecond line"
                } else {
                    "other channel"
                }
                .into(),
                created_at: "2026-01-01T00:00:00Z".into(),
            }])
        })
    }
}

type Sent = (
    String,
    String,
    async_channel::Sender<Result<crate::conversation::Message, AuthError>>,
);
struct SendAuth(Arc<std::sync::Mutex<Vec<Sent>>>);
impl AuthApi for SendAuth {
    fn signup(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
        TestAuth.signup(s, u, p)
    }
    fn login(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
        TestAuth.login(s, u, p)
    }
    fn logout(&self, s: String, t: String) -> ApiFuture<Result<(), AuthError>> {
        TestAuth.logout(s, t)
    }
    fn channels(
        &self,
        s: String,
        t: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Channel>, AuthError>> {
        TestAuth.channels(s, t)
    }
    fn create_channel(
        &self,
        s: String,
        t: String,
        n: String,
    ) -> ApiFuture<Result<crate::conversation::Channel, AuthError>> {
        TestAuth.create_channel(s, t, n)
    }
    fn history(
        &self,
        s: String,
        t: String,
        id: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Message>, AuthError>> {
        TestAuth.history(s, t, id)
    }
    fn send_message(
        &self,
        _: String,
        _: String,
        id: String,
        text: String,
    ) -> ApiFuture<Result<crate::conversation::Message, AuthError>> {
        let (tx, rx) = async_channel::bounded(1);
        self.0.lock().unwrap().push((id, text, tx));
        Box::pin(async move { rx.recv().await.unwrap() })
    }
}

type PageReply = async_channel::Sender<Result<crate::conversation::Page, AuthError>>;
struct PagedAuth(std::sync::mpsc::Sender<(Option<String>, PageReply)>);
impl AuthApi for PagedAuth {
    fn signup(
        &self,
        server: String,
        user: String,
        password: String,
    ) -> ApiFuture<Result<Login, AuthError>> {
        TestAuth.signup(server, user, password)
    }
    fn login(
        &self,
        server: String,
        user: String,
        password: String,
    ) -> ApiFuture<Result<Login, AuthError>> {
        TestAuth.login(server, user, password)
    }
    fn logout(&self, server: String, token: String) -> ApiFuture<Result<(), AuthError>> {
        TestAuth.logout(server, token)
    }
    fn channels(
        &self,
        server: String,
        token: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Channel>, AuthError>> {
        TestAuth.channels(server, token)
    }
    fn create_channel(
        &self,
        server: String,
        token: String,
        name: String,
    ) -> ApiFuture<Result<crate::conversation::Channel, AuthError>> {
        TestAuth.create_channel(server, token, name)
    }
    fn history(
        &self,
        _: String,
        _: String,
        _: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Message>, AuthError>> {
        Box::pin(async { unreachable!() })
    }
    fn history_page(
        &self,
        _: String,
        _: String,
        _: String,
        before: Option<String>,
    ) -> ApiFuture<Result<crate::conversation::Page, AuthError>> {
        let tx = self.0.clone();
        Box::pin(async move {
            let (reply, rx) = async_channel::bounded(1);
            tx.send((before, reply)).unwrap();
            rx.recv().await.unwrap()
        })
    }
}
