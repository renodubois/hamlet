//! Login ownership tests use real Kit controls and the owned session interface.
use crate::api::HttpTransport;
use crate::api::test_support::{RequestAdapter, Response};
use crate::api::{ApiError, ApiFuture};
use crate::runtime::Execution;
use crate::session::SessionCoordinator;
use crate::storage::{Config, Selection};
use crate::views::login::LoginView;
use gpui_kit::component::Root;
use gpui_kit::component::button::Button;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, Window, div,
};
use std::sync::{Arc, Mutex};

struct Submission {
    origin: String,
    path: String,
    body: serde_json::Value,
    reply: async_channel::Sender<Result<Response, ApiError>>,
}
#[derive(Clone, Default)]
struct AuthReplies(Arc<Mutex<Vec<Submission>>>);
impl RequestAdapter for AuthReplies {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.url().path().ends_with("/logout") {
            return Box::pin(async {
                Ok(Response::controlled(reqwest::StatusCode::NO_CONTENT, ""))
            });
        }
        let (reply, response) = async_channel::bounded(1);
        self.0.lock().unwrap().push(Submission {
            origin: request.url().origin().ascii_serialization(),
            path: request.url().path().to_owned(),
            body: serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap(),
            reply,
        });
        Box::pin(async move { response.recv().await.unwrap() })
    }
}

// A composition fixture, not a replacement authentication workflow. Tests deliver opaque
// outcomes through the same owned session interface and never read LoginView fields.
struct LoginHost {
    session: Entity<SessionCoordinator>,
    login: Entity<LoginView>,
    visible: bool,
}
impl Render for LoginHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut host = div().size_full().flex().flex_col().gap_3();
        if self.visible {
            host = host.child(self.login.clone());
        }
        host.child(
            Button::new("toggle-form")
                .label("Toggle form")
                .on_click(cx.listener(|host, _, _, cx| {
                    host.visible = !host.visible;
                    cx.notify();
                })),
        )
        .child(
            Button::new("recreate-form")
                .label("Recreate form")
                .on_click(cx.listener(|host, _, window, cx| {
                    host.login = cx.new(|cx| LoginView::new(host.session.clone(), window, cx));
                    host.visible = true;
                    cx.notify();
                })),
        )
    }
}

fn accepted(signup: bool) -> Response {
    Response::controlled(
        if signup {
            reqwest::StatusCode::CREATED
        } else {
            reqwest::StatusCode::OK
        },
        r#"{"user":{"id":"42","username":"Ada"},"access_token":"synthetic-login-view","expires_at":"2099-01-01T00:00:00Z"}"#,
    )
}

#[gpui_kit::test]
fn retained_hidden_login_and_signup_controls_are_cleared_on_acceptance(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for signup in [false, true] {
        let api = AuthReplies::default();
        let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
        let session = cx.new(|_| {
            SessionCoordinator::new(
                HttpTransport::with_adapter(Arc::new(api.clone())),
                execution,
                Config::default(),
                None,
            )
        });
        let updates = session.read_with(cx, |session, _| session.updates());
        let (_, cx) = cx.add_window_view(|window, cx| {
            let login = cx.new(|cx| LoginView::new(session.clone(), window, cx));
            let host = cx.new(|_| LoginHost {
                session: session.clone(),
                login,
                visible: true,
            });
            Root::new(host, window, cx)
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            if signup {
                window.click("auth-mode", cx);
            }
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("synthetic password", cx);
            window.click(if signup { "signup" } else { "login" }, cx);
            window.click("toggle-form", cx);
            window.render_frame(cx);
            assert!(window.try_find("password").is_none());
        });
        cx.run_until_parked();
        {
            let submissions = api.0.lock().unwrap();
            assert_eq!(submissions.len(), 1);
            assert_eq!(
                submissions[0].path,
                if signup {
                    "/api/v1/auth/signup"
                } else {
                    "/api/v1/auth/login"
                }
            );
            assert_eq!(submissions[0].body["password"], "synthetic password");
            submissions[0].reply.try_send(Ok(accepted(signup))).unwrap();
        }
        cx.run_until_parked();
        session.update(cx, |session, cx| {
            session.apply(updates.try_recv().unwrap());
            cx.notify();
        });
        cx.run_until_parked();
        assert!(session.read_with(cx, |session, _| session.active().is_some()));
        session.update(cx, |session, cx| {
            session.logout();
            cx.notify();
        });
        cx.update(|window, cx| {
            window.click("toggle-form", cx);
            window.render_frame(cx);
            assert_eq!(window.find("username").value(), Some("Ada"));
            assert!(window.find("password").value().is_none_or(str::is_empty));
            // A real submission proves there is no hidden password to reuse.
            window.click(if signup { "signup" } else { "login" }, cx);
            window.render_frame(cx);
            assert!(window.find("auth-feedback").label().is_some());
        });
        cx.run_until_parked();
        assert_eq!(api.0.lock().unwrap().len(), 1);
    }
}

#[gpui_kit::test]
fn recreated_login_hydrates_pending_and_failure_then_owns_new_server_edits(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let api = AuthReplies::default();
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let session = cx.new(|_| {
        SessionCoordinator::new(
            HttpTransport::with_adapter(Arc::new(api.clone())),
            execution,
            Config::default(),
            None,
        )
    });
    let updates = session.read_with(cx, |session, _| session.updates());
    let (_, cx) = cx.add_window_view(|window, cx| {
        let login = cx.new(|cx| LoginView::new(session.clone(), window, cx));
        let host = cx.new(|_| LoginHost {
            session: session.clone(),
            login,
            visible: true,
        });
        Root::new(host, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("discard this password", cx);
        window.click("login", cx);
        window.click("recreate-form", cx);
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Signing in…"));
        assert!(window.find("password").value().is_none_or(str::is_empty));
        assert!(window.find("username").value().is_none_or(str::is_empty));
        window.click("login", cx);
    });
    cx.run_until_parked();
    {
        let submissions = api.0.lock().unwrap();
        assert_eq!(submissions.len(), 1);
        submissions[0]
            .reply
            .try_send(Ok(Response::controlled(
                reqwest::StatusCode::UNAUTHORIZED,
                r#"{"error":{"code":"unauthorized"}}"#,
            )))
            .unwrap();
    }
    cx.run_until_parked();
    session.update(cx, |session, cx| {
        session.apply(updates.try_recv().unwrap());
        cx.notify();
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Log in"));
        assert!(
            window
                .find("auth-feedback")
                .label()
                .unwrap()
                .contains("Incorrect username or password")
        );
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("old server password", cx);
        window.click("server-url", cx);
        window.press("ctrl-a", cx);
        window.input("https://new.example", cx);
        window.render_frame(cx);
        assert!(window.find("password").value().is_none_or(str::is_empty));
        window.click("login", cx); // No password from the removed form or old server.
        window.click("password", cx);
        window.input("second server password", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    {
        let submissions = api.0.lock().unwrap();
        assert_eq!(submissions.len(), 2);
        assert_eq!(submissions[1].origin, "https://new.example");
        assert_eq!(submissions[1].body["password"], "second server password");
        submissions[1].reply.try_send(Ok(accepted(false))).unwrap();
    }
    cx.run_until_parked();
    session.update(cx, |session, cx| {
        session.apply(updates.try_recv().unwrap());
        cx.notify();
    });
    cx.run_until_parked();
    assert!(session.read_with(cx, |session, _| session.active().is_some()));
    session.update(cx, |session, cx| {
        session.logout();
        cx.notify();
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("password").value().is_none_or(str::is_empty));
        window.click("password", cx);
        window.input("remove before submission", cx);
        window.click("recreate-form", cx);
        window.render_frame(cx);
        assert!(window.find("password").value().is_none_or(str::is_empty));
        assert_eq!(
            window.find("server-url").value(),
            Some("https://new.example")
        );
    });
}

#[gpui_kit::test]
fn login_rejection_keeps_editable_password_for_deliberate_retry(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let api = AuthReplies::default();
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let shell = crate::views::app_shell::open(
            window,
            cx,
            HttpTransport::with_adapter(Arc::new(api.clone())),
            Config::default(),
            None,
            execution,
        );
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("recoverable password", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    api.0.lock().unwrap()[0]
        .reply
        .try_send(Err(ApiError::InvalidCredentials))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Log in"));
        assert_eq!(
            window.find("auth-feedback").label(),
            Some("Incorrect username or password.")
        );
        window.click("username", cx);
        window.press("ctrl-a", cx);
        window.input("Ada_2", cx);
        window.click("login", cx);
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Signing in…"));
    });
    cx.run_until_parked();
    let submissions = api.0.lock().unwrap();
    assert_eq!(submissions.len(), 2);
    assert_eq!(submissions[1].body["username"], "Ada_2");
    assert_eq!(submissions[1].body["password"], "recoverable password");
}

#[gpui_kit::test]
fn login_view_hydrates_saved_identity_without_retaining_a_password(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let session = cx.new(|_| {
            SessionCoordinator::new(
                HttpTransport::new(),
                execution,
                Config {
                    server: Some("https://saved.example/".into()),
                    saved: Some(Selection {
                        server: "https://saved.example/".into(),
                        user: crate::api::User {
                            id: "42".into(),
                            username: "Ada".into(),
                        },
                        expires_at: 1,
                    }),
                    pending_deletions: vec![],
                },
                None,
            )
        });
        let login = cx.new(|cx| LoginView::new(session, window, cx));
        Root::new(login, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("server-url").value(),
            Some("https://saved.example/")
        );
        assert_eq!(window.find("username").value(), Some("Ada"));
        assert!(window.find("password").value().is_none_or(str::is_empty));
        assert_eq!(window.find("login").label(), Some("Log in"));
        window.click("server-url", cx);
        window.press("tab", cx);
        assert_eq!(window.find("username").focused(), Some(true));
    });
}
