//! Editable login state belongs to this view, never to the shell or session model.
use crate::session::SessionCoordinator;
use gpui_kit::base::Disableable;
use gpui_kit::base::input::{InputBaseState, InputEvent, InputMode, InputState};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::*;

pub(crate) struct LoginView {
    session: Entity<SessionCoordinator>,
    server: Entity<InputBaseState<InputMode>>,
    username: Entity<InputBaseState<InputMode>>,
    password: Entity<InputBaseState<InputMode>>,
    signup: bool,
    _submit_subscriptions: Vec<Subscription>,
    _server_subscription: Subscription,
    _session_subscription: Subscription,
}

impl LoginView {
    /// The application owns the session and notifies its entity after session updates.
    /// Dropping/recreating this view never cancels authentication or saved-login cleanup.
    pub fn new(
        session: Entity<SessionCoordinator>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let server =
            cx.new(|cx| InputState::new(window, cx).default_value(session.read(cx).server()));
        let username = cx.new(|cx| {
            InputState::new(window, cx).default_value(session.read(cx).initial_username())
        });
        let password = cx.new(|cx| InputState::new(window, cx).masked(true));
        let server_subscription = cx.subscribe_in(
            &server,
            window,
            |view: &mut Self, field, event, window, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = field.read(cx).text().to_string();
                    if view.session.read(cx).server() != value {
                        view.clear_sensitive(window, cx);
                        view.session.update(cx, |session, cx| {
                            session.change_server(value);
                            cx.notify();
                        });
                    }
                }
            },
        );
        let session_subscription =
            cx.observe_in(&session, window, |view: &mut Self, _, window, cx| {
                // Also runs while the view is retained but not rendered. No secret survives
                // acceptance, and externally selected servers hydrate the real control.
                let server = view.session.read(cx).server().to_owned();
                if *view.server.read(cx).text() != server {
                    view.clear_sensitive(window, cx);
                    view.server
                        .update(cx, |input, cx| input.set_value(server, window, cx));
                }
                if view.session.read(cx).active().is_some() {
                    view.clear_sensitive(window, cx);
                }
                cx.notify();
            });
        let submit_subscriptions = [&server, &username, &password]
            .into_iter()
            .map(|field| {
                cx.subscribe_in(field, window, |view: &mut Self, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        view.submit(window, cx);
                    }
                })
            })
            .collect();
        Self {
            session,
            server,
            username,
            password,
            signup: false,
            _submit_subscriptions: submit_subscriptions,
            _server_subscription: server_subscription,
            _session_subscription: session_subscription,
        }
    }

    /// Lifecycle cleanup, including controls retained while another screen is shown.
    /// This is not a memory-zeroization guarantee.
    pub fn clear_sensitive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.password
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn set_signup(&mut self, signup: bool, cx: &mut Context<Self>) {
        if !self.session.read(cx).pending() && self.signup != signup {
            self.session.update(cx, |session, cx| {
                session.cancel_pending();
                cx.notify();
            });
            self.signup = signup;
            cx.notify();
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.read(cx).pending() || self.session.read(cx).active().is_some() {
            return;
        }
        let server = self.server.read(cx).text().to_string();
        if self.session.read(cx).server() != server {
            self.clear_sensitive(window, cx);
            self.session.update(cx, |session, cx| {
                session.change_server(server);
                session.server_changed_feedback();
                cx.notify();
            });
            return;
        }
        let username = self.username.read(cx).text().to_string();
        let password = self.password.read(cx).text().to_string();
        self.session.update(cx, |session, cx| {
            session.submit(username, password, self.signup);
            cx.notify();
        });
    }
}

impl Render for LoginView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut form = div()
            .flex()
            .flex_col()
            .gap_3()
            .items_center()
            .w(px(400.0))
            .p_1();
        let session = self.session.read(cx);
        if session.active().is_some() {
            return form;
        }
        let pending = session.pending();
        let restoring = session.restore_pending();
        let feedback = session.feedback().map(str::to_owned);

        let submit_button = Button::new(if self.signup { "signup" } else { "login" })
            .disabled(pending)
            .label(if pending {
                if self.signup {
                    "Creating user…"
                } else {
                    "Signing in…"
                }
            } else if self.signup {
                "Create user"
            } else {
                "Log in"
            })
            .w_full()
            .on_click(cx.listener(|view, _, window, cx| view.submit(window, cx)));

        form = form
            .child(div().text_xl().child(if self.signup {
                "Sign up for Hamlet"
            } else {
                "Log in to Hamlet"
            }))
            .child(
                div().w_full().child("Server URL").child(
                    Input::new(&self.server)
                        .id("server-url")
                        .aria_label("Server URL"),
                ),
            )
            .child(
                div().w_full().child("Username").child(
                    Input::new(&self.username)
                        .id("username")
                        .aria_label("Username"),
                ),
            )
            .child(
                div().w_full().child("Password").child(
                    Input::new(&self.password)
                        .id("password")
                        .aria_label("Password"),
                ),
            )
            .child(if self.signup {
                submit_button.secondary()
            } else {
                submit_button.primary()
            })
            .child(if self.signup {
                Button::new("auth-mode")
                    .disabled(pending)
                    .label(if self.signup {
                        "Have a user? Log in"
                    } else {
                        "New user? Sign up"
                    })
                    .on_click(cx.listener(|view, _, _, cx| view.set_signup(!view.signup, cx)))
            } else {
                Button::new("auth-mode")
                    .disabled(pending)
                    .label("New user? Sign up")
                    .on_click(cx.listener(|view, _, _, cx| view.set_signup(!view.signup, cx)))
            });
        if let Some(feedback) = feedback {
            form = form.child(
                div()
                    .id("auth-feedback")
                    .aria_label(feedback.clone())
                    .test_support()
                    .child(feedback),
            );
        }
        form
    }
}
