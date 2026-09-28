mod http;
mod session;

use gpui_kit::base::Disableable;
use gpui_kit::base::input::{InputBaseState, InputEvent, InputMode, InputState};
use gpui_kit::component::Root;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::Input;
use gpui_kit::*;
use session::{AppSession, AuthApi, DEFAULT_SERVER_URL};
use std::{
    sync::{Arc, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("network runtime"))
}

struct Hamlet {
    session: AppSession,
    server: Entity<InputBaseState<InputMode>>,
    username: Entity<InputBaseState<InputMode>>,
    password: Entity<InputBaseState<InputMode>>,
    _server_subscription: Subscription,
    clear_password: bool,
}

impl Hamlet {
    fn new(window: &mut Window, cx: &mut Context<Self>, api: Arc<dyn AuthApi>) -> Self {
        let server = cx.new(|cx| InputState::new(window, cx).default_value(DEFAULT_SERVER_URL));
        let username = cx.new(|cx| InputState::new(window, cx));
        let password = cx.new(|cx| InputState::new(window, cx).masked(true));
        let subscription = cx.subscribe(&server, |view: &mut Self, field, event, cx| {
            if matches!(event, InputEvent::Change) {
                let value = field.read(cx).text().to_string();
                if view.session.server != value {
                    view.session.change_server(value);
                    view.clear_password = true;
                    cx.notify();
                }
            }
        });
        Self {
            session: AppSession::new(api),
            server,
            username,
            password,
            _server_subscription: subscription,
            clear_password: false,
        }
    }

    fn login_disabled(&self) -> bool {
        self.session.pending
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.session.pending {
            return;
        }
        let server = self.server.read(cx).text().to_string();
        if self.session.server != server || self.clear_password {
            self.session.change_server(server);
            self.session.feedback = Some("Server changed. Enter your password again.".into());
            cx.notify();
            return;
        }
        self.session.username = self.username.read(cx).text().to_string();
        self.session.password = self.password.read(cx).text().to_string();
        let Some(request) = self.session.submit() else {
            cx.notify();
            return;
        };
        let api = self.session.api.clone();
        let (send, receive) = async_channel::bounded(1);
        let server = request.server.clone();
        let username = request.username.clone();
        let password = request.password.clone();
        runtime().spawn(async move {
            let result = api.login(server, username, password).await;
            let _ = send.send(result).await;
        });
        cx.spawn(async move |weak, cx| {
            if let Ok(result) = receive.recv().await {
                let _ = weak.update_in(cx, |view, window, cx| {
                    let accepted = view.session.complete_login(request, result, now());
                    if let Some(session) = view.session.active.as_ref().filter(|_| accepted) {
                        let generation = view.session.session_generation().unwrap();
                        let expires_at = session.expires_at;
                        let (send, receive) = async_channel::bounded(1);
                        runtime().spawn(async move {
                            tokio::time::sleep(Duration::from_secs(
                                expires_at.saturating_sub(now()) as u64,
                            ))
                            .await;
                            let _ = send.send(()).await;
                        });
                        cx.spawn(async move |weak, cx| {
                            if receive.recv().await.is_ok() {
                                let _ = weak.update(cx, |view, cx| {
                                    if view.session.session_generation() == Some(generation) {
                                        view.session.expire(now());
                                        cx.notify();
                                    }
                                });
                            }
                        })
                        .detach();
                        view.password
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn logout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(revocation) = self.session.logout() {
            let api = self.session.api.clone();
            let (send, receive) = async_channel::bounded(1);
            runtime().spawn(async move {
                let result = api.logout(revocation.server, revocation.token).await;
                let _ = send.send((revocation.generation, result)).await;
            });
            cx.spawn(async move |weak, cx| {
                if let Ok((generation, result)) = receive.recv().await {
                    let _ = weak.update(cx, |view, cx| {
                        view.session.revocation_result(generation, result);
                        cx.notify();
                    });
                }
            })
            .detach();
        }
        self.password
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }
}

impl Render for Hamlet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.clear_password {
            self.password
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.clear_password = false;
        }
        let view = cx.entity().downgrade();
        let mut surface = div()
            .size_full()
            .bg(rgb(0xffffff))
            .flex()
            .flex_col()
            .justify_center()
            .items_center()
            .gap_3()
            .text_color(rgb(0x111111));
        if let Some(session) = &self.session.active {
            let label = format!(
                "Logged in as {} at {}",
                session.user.username, session.server
            );
            surface =
                surface
                    .child(
                        div()
                            .id("session-status")
                            .aria_label(label.clone())
                            .test_support()
                            .child(label),
                    )
                    .child(Button::new("logout").label("Log out").on_click(
                        move |_, window, cx| {
                            let _ = view.update(cx, |view, cx| view.logout(window, cx));
                        },
                    ));
        } else {
            let login_view = view.clone();
            surface = surface
                .child(div().text_xl().child("Log in to Hamlet"))
                .child(
                    div().w(px(360.)).child("Server URL").child(
                        Input::new(&self.server)
                            .id("server-url")
                            .aria_label("Server URL"),
                    ),
                )
                .child(
                    div().w(px(360.)).child("Username").child(
                        Input::new(&self.username)
                            .id("username")
                            .aria_label("Username"),
                    ),
                )
                .child(
                    div().w(px(360.)).child("Password").child(
                        Input::new(&self.password)
                            .id("password")
                            .aria_label("Password"),
                    ),
                )
                .child(
                    Button::new("login")
                        .disabled(self.login_disabled())
                        .label(if self.session.pending {
                            "Signing in…"
                        } else {
                            "Log in"
                        })
                        .on_click(move |_, _, cx| {
                            let _ = login_view.update(cx, |view, cx| view.submit(cx));
                        }),
                )
                .child(div().child(
                    "Session is memory-only: you will need to log in again after restarting.",
                ));
        }
        if let Some(feedback) = &self.session.feedback {
            surface = surface.child(
                div()
                    .id("auth-feedback")
                    .aria_label(feedback.clone())
                    .test_support()
                    .child(feedback.clone()),
            );
        }
        surface
    }
}

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);

            let bounds = Bounds::centered(None, size(px(500.), px(500.0)), cx);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    let view =
                        cx.new(|cx| Hamlet::new(window, cx, Arc::new(http::HttpAuth::new())));

                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
}

#[cfg(test)]
mod tests {
    use super::Hamlet;
    use crate::session::{ApiFuture, AuthApi, AuthError, Login, User};
    use gpui_kit::TestSupportExt as _;
    use gpui_kit::base::SelectableText;
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{
        AppContext as _, Context, FocusHandle, InteractiveElement as _, IntoElement, ListAlignment,
        ListState, MouseButton, ParentElement as _, Render, SharedString, Styled as _,
        TestAppContext, Window, div, list, px,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    // This is a real Kit/GPUI history surface, not a simulated scrolling model.
    struct HistoryProbe {
        list: ListState,
        messages: Vec<(String, SharedString)>,
        focus: FocusHandle,
    }

    impl HistoryProbe {
        fn new(cx: &mut Context<Self>) -> Self {
            let messages =
                (0..40)
                    .map(|ix| {
                        (format!("original-{ix}"), if ix % 2 == 0 {
                    "first line\nsecond line with enough words to wrap at a narrow width"
                } else {
                    "short line"
                }.into())
                    })
                    .collect();
            Self {
                list: ListState::new(40, ListAlignment::Top, px(0.)).measure_all(),
                messages,
                focus: cx.focus_handle(),
            }
        }

        fn prepend(&mut self, cx: &mut Context<Self>) {
            self.messages.splice(
                0..0,
                [
                    ("older-0".into(), "older\nfirst line".into()),
                    ("older-1".into(), "older short".into()),
                ],
            );
            self.list.splice(0..0, 2);
            cx.notify();
        }
    }

    impl Render for HistoryProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let messages = self.messages.clone();
            let focus = self.focus.clone();
            div().w(px(260.)).h(px(180.)).child(
                div()
                    .id("history")
                    .test_support()
                    .track_focus(&self.focus)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        window.focus(&focus, cx)
                    })
                    .size_full()
                    .child(
                        list(self.list.clone(), move |ix, _, _| {
                            let (id, text) = &messages[ix];
                            div()
                                .id(format!("message-{id}"))
                                .test_support()
                                .w_full()
                                .p_2()
                                .child(SelectableText::new(
                                    format!("selectable-{id}"),
                                    text.clone(),
                                ))
                                .into_any_element()
                        })
                        .size_full(),
                    ),
            )
        }
    }

    #[gpui_kit::test]
    fn varied_height_history_preserves_reader_on_prepend(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let probe = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = probe.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(HistoryProbe::new);
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<HistoryProbe> = probe.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            for _ in 0..8 {
                window.scroll(
                    "history",
                    gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
                        gpui_kit::px(0.),
                        gpui_kit::px(-90.),
                    )),
                    cx,
                );
            }
            let before = view.read(cx).list.logical_scroll_top();
            assert!(
                before.item_ix > 0,
                "the actual wheel must scroll variable-height rows"
            );
            let id = format!("message-original-{}", before.item_ix);
            let y = window.find(id.clone()).bounds().origin.y;
            view.update(cx, |view, cx| view.prepend(cx));
            window.render_frame(cx);
            let after = view.read(cx).list.logical_scroll_top();
            assert_eq!(after.item_ix, before.item_ix + 2);
            assert_eq!(after.offset_in_item, before.offset_in_item);
            assert_eq!(window.find(id).bounds().origin.y, y);
        });
    }

    #[gpui_kit::test]
    fn selectable_history_copies_line_breaks(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(HistoryProbe::new);
            Root::new(view, window, cx)
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            let bounds = window.find("message-original-0").bounds();
            window.drag(
                bounds.origin + gpui_kit::point(gpui_kit::px(2.), gpui_kit::px(12.)),
                bounds.origin
                    + gpui_kit::point(
                        bounds.size.width - gpui_kit::px(2.),
                        bounds.size.height - gpui_kit::px(2.),
                    ),
                cx,
            );
            assert!(gpui_kit::base::TextSelection::selected_text(window, cx).contains('\n'));
            window.press("ctrl-c", cx);
        });
        assert!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .unwrap()
                .contains('\n')
        );
    }

    struct TestAuth;
    impl AuthApi for TestAuth {
        fn login(
            &self,
            _: String,
            username: String,
            _: String,
        ) -> ApiFuture<Result<Login, AuthError>> {
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
    }

    struct PendingAuth(Arc<AtomicUsize>);
    impl AuthApi for PendingAuth {
        fn login(&self, _: String, _: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::pending())
        }
        fn logout(&self, _: String, _: String) -> ApiFuture<Result<(), AuthError>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[gpui_kit::test]
    fn login_validation_and_pending_button_are_visible_and_inert(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let calls = Arc::new(AtomicUsize::new(0));
        let probe = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = probe.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PendingAuth(calls.clone()))));
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = probe.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(!view.read(cx).login_disabled());
            window.click("login", cx);
            window.render_frame(cx);
            assert!(
                window
                    .find("auth-feedback")
                    .label()
                    .unwrap()
                    .contains("username and password")
            );
            assert!(!view.read(cx).login_disabled());
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
            window.render_frame(cx);
            assert_eq!(window.find("login").label(), Some("Signing in…"));
            assert!(view.read(cx).login_disabled());
            window.click("login", cx);
            view.update(cx, |view, cx| view.submit(cx)); // guard also covers non-pointer callers
            window.render_frame(cx);
            assert!(view.read(cx).session.pending);
            assert!(view.read(cx).login_disabled());
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while calls.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[gpui_kit::test]
    fn real_controls_login_and_logout(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|window, cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("server-url").value(),
                Some("http://127.0.0.1:8081")
            );
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
            window.render_frame(cx);
            assert_eq!(window.find("login").label(), Some("Signing in…"));
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find("session-status")
                    .label()
                    .unwrap()
                    .contains("Ada")
            );
            window.click("logout", cx);
            window.render_frame(cx);
            assert!(window.find("password").value().is_none_or(str::is_empty));
            assert_eq!(window.find("login").label(), Some("Log in"));
        });
    }
}
