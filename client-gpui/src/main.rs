mod conversation;
mod http;
mod session;

use conversation::{Conversation, Load, Older, ReadRequest};
use gpui_kit::base::SelectableText;
use gpui_kit::prelude::FluentBuilder as _;

use gpui_kit::base::Disableable;
use gpui_kit::base::input::{
    InputBaseState, InputEvent, InputMode, InputState, TextareaMode, TextareaState,
};
use gpui_kit::component::Root;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::*;
use session::{AppSession, AuthApi, DEFAULT_SERVER_URL};
use std::{
    sync::{Arc, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// Presentation tokens and bundled icon mapping live here; pane layout is in render().
mod theme {
    pub const BACKGROUND: u32 = 0xf6f7fb;
    pub const SIDEBAR: u32 = 0xe9edf5;
    pub const SELECTED: u32 = 0xcddbf5;
    pub const TEXT: u32 = 0x182338;
    pub const MUTED: u32 = 0x586477;
    pub fn channel_icon() -> gpui_kit::assets::IconName {
        gpui_kit::assets::IconName::Hash
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn hint_history_row_heights(list: &ListState) {
    // GPUI replaces these estimates as variable-height rows are measured.
    list.clone().with_uniform_item_height(px(64.));
}

// Both slices are chronological stable IDs. Splice each new gap at its real index;
// ListState then adjusts the visible anchor for insertions preceding it.
fn reconcile_history_list(list: &ListState, before: &[String], after: &[String]) {
    let mut old = 0;
    let mut new = 0;
    while old < before.len() {
        let start = new;
        while new < after.len() && after[new] != before[old] {
            new += 1;
        }
        if new == after.len() {
            // Unexpected reorder/removal: do not leave list indices pointing at wrong rows.
            list.splice(0..list.item_count(), after.len());
            return;
        }
        if new > start {
            list.splice(start..start, new - start);
        }
        old += 1;
        new += 1;
    }
    if new < after.len() {
        list.splice(new..new, after.len() - new);
    }
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("network runtime"))
}

struct Hamlet {
    session: AppSession,
    conversation: Conversation,
    history_focus: FocusHandle,
    history_list: ListState,
    history_task: Option<tokio::task::JoinHandle<()>>,
    history_serial: u64,
    server: Entity<InputBaseState<InputMode>>,
    username: Entity<InputBaseState<InputMode>>,
    password: Entity<InputBaseState<InputMode>>,
    channel_name: Entity<InputBaseState<InputMode>>,
    composer: Entity<InputBaseState<TextareaMode>>,
    _composer_subscription: Subscription,
    syncing_composer: bool,
    _server_subscription: Subscription,
    clear_password: bool,
    signup: bool,
}

impl Hamlet {
    fn new(window: &mut Window, cx: &mut Context<Self>, api: Arc<dyn AuthApi>) -> Self {
        let server = cx.new(|cx| InputState::new(window, cx).default_value(DEFAULT_SERVER_URL));
        let username = cx.new(|cx| InputState::new(window, cx));
        let password = cx.new(|cx| InputState::new(window, cx).masked(true));
        let channel_name = cx.new(|cx| InputState::new(window, cx));
        let composer = cx.new(|cx| TextareaState::new(window, cx).submit_on_enter(true));
        let composer_subscription = cx.subscribe(&composer, |view: &mut Self, field, event, cx| {
            if view.syncing_composer {
                return;
            }
            match event {
                InputEvent::Change => {
                    if let Some(id) = view.conversation.selected.clone() {
                        view.conversation
                            .set_draft(&id, field.read(cx).text().to_string());
                        cx.notify();
                    }
                }
                InputEvent::PressEnter {
                    shift: false,
                    secondary: false,
                } => {
                    // submit_on_enter emits without modifying the textarea, including when
                    // the caret is in the middle or the draft ends with an intentional newline.
                    if let Some(id) = view.conversation.selected.clone() {
                        view.conversation
                            .set_draft(&id, field.read(cx).text().to_string());
                    }
                    view.dispatch_send(cx);
                }
                _ => {}
            }
        });
        let subscription = cx.subscribe(&server, |view: &mut Self, field, event, cx| {
            if matches!(event, InputEvent::Change) {
                let value = field.read(cx).text().to_string();
                if view.session.server != value {
                    view.session.change_server(value);
                    view.cancel_history();
                    view.conversation.clear();
                    view.history_list.reset(0);
                    view.clear_password = true;
                    cx.notify();
                }
            }
        });
        let history_list = ListState::new(0, ListAlignment::Top, px(0.));
        let weak = cx.entity().downgrade();
        history_list.set_scroll_handler(move |event, _, cx| {
            if event.is_scrolled {
                let _ = weak.update(cx, |view, cx| {
                    if event.count > 0 && event.visible_range.start <= 1 {
                        view.request_older(cx);
                    }
                    cx.notify(); // re-evaluate the jump button as the reader moves
                });
            }
        });
        Self {
            session: AppSession::new(api),
            conversation: Conversation::default(),
            history_focus: cx.focus_handle(),
            history_list,
            history_task: None,
            history_serial: 0,
            server,
            username,
            password,
            channel_name,
            composer,
            _composer_subscription: composer_subscription,
            syncing_composer: false,
            _server_subscription: subscription,
            clear_password: false,
            signup: false,
        }
    }

    fn store_composer(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.conversation.selected.clone()
            && !self.conversation.send_pending.contains_key(&id)
        {
            self.conversation
                .set_draft(&id, self.composer.read(cx).text().to_string());
        }
    }

    fn sync_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self
            .conversation
            .selected
            .as_deref()
            .map(|id| self.conversation.draft(id))
            .unwrap_or("")
            .to_owned();
        self.syncing_composer = true;
        self.composer
            .update(cx, |input, cx| input.set_value(&text, window, cx));
        self.syncing_composer = false;
    }

    fn send_message(&mut self, cx: &mut Context<Self>) {
        self.store_composer(cx);
        self.dispatch_send(cx);
    }

    fn dispatch_send(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.conversation.send(&self.session) else {
            cx.notify();
            return;
        };
        let api = self.session.api.clone();
        let work = request.clone();
        // Timeout also bounds controlled adapters; aborting a timed-out future never replays it.
        #[cfg(test)]
        {
            let timeout = cx.background_executor().timer(Duration::from_secs(9));
            cx.spawn(async move |weak, cx| {
            let result = tokio::select! {
                result = api.send_message(work.server, work.token, work.channel_id, work.text) => result,
                _ = timeout => Err(session::AuthError::Unavailable),
            };
            let _ = weak.update_in(cx, |view, window, cx| {
                view.finish_send(request, result, window, cx)
            });
        })
        .detach();
        }
        #[cfg(not(test))]
        {
            let (send, receive) = async_channel::bounded(1);
            runtime().spawn(async move {
                let result = tokio::time::timeout(
                    Duration::from_secs(9),
                    api.send_message(work.server, work.token, work.channel_id, work.text),
                )
                .await
                .unwrap_or(Err(session::AuthError::Unavailable));
                let _ = send.send(result).await;
            });
            cx.spawn(async move |weak, cx| {
                if let Ok(result) = receive.recv().await {
                    let _ = weak.update_in(cx, |view, window, cx| {
                        view.finish_send(request, result, window, cx)
                    });
                }
            })
            .detach();
        }
        cx.notify();
    }

    fn finish_send(
        &mut self,
        request: conversation::SendRequest,
        result: Result<conversation::Message, session::AuthError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = request.channel_id.clone();
        let outcome = self
            .conversation
            .complete_send(&mut self.session, &request, result, now());
        if self.session.active.is_none() {
            self.cancel_history();
            self.history_list.reset(0);
            self.sync_composer(window, cx);
        } else if self.conversation.selected.as_deref() == Some(&id) {
            if outcome == conversation::SendOutcome::Confirmed {
                self.sync_composer(window, cx);
                if let Some(next) = self.conversation.reconcile_confirmed(&self.session) {
                    self.cancel_history();
                    self.load_history(next, cx);
                }
            } else if outcome == conversation::SendOutcome::Uncertain
                && let Some(request) = self.conversation.reconcile_uncertain(&self.session)
            {
                self.cancel_history();
                self.load_history(request, cx);
            }
        }
        cx.notify();
    }

    fn login_disabled(&self) -> bool {
        self.session.pending
    }

    fn set_signup(&mut self, signup: bool, cx: &mut Context<Self>) {
        if !self.session.pending && self.signup != signup {
            self.session.cancel_pending();
            self.signup = signup;
            cx.notify();
        }
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
        let signup = self.signup;
        let Some(request) = (if signup {
            self.session.submit_signup()
        } else {
            self.session.submit()
        }) else {
            cx.notify();
            return;
        };
        let api = self.session.api.clone();
        let (send, receive) = async_channel::bounded(1);
        let server = request.server.clone();
        let username = request.username.clone();
        let password = request.password.clone();
        runtime().spawn(async move {
            let result = if signup {
                api.signup(server, username, password).await
            } else {
                api.login(server, username, password).await
            };
            let _ = send.send(result).await;
        });
        cx.spawn(async move |weak, cx| {
            if let Ok(result) = receive.recv().await {
                let _ = weak.update_in(cx, |view, window, cx| {
                    let accepted = if signup {
                        view.session.complete_signup(request, result, now())
                    } else {
                        view.session.complete_login(request, result, now())
                    };
                    if accepted && view.session.active.is_some() {
                        view.enter_authenticated(window, cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn enter_authenticated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.session.active.as_ref() {
            let generation = self.session.session_generation().unwrap();
            let expires_at = session.expires_at;
            let (send, receive) = async_channel::bounded(1);
            runtime().spawn(async move {
                tokio::time::sleep(Duration::from_secs(expires_at.saturating_sub(now()) as u64))
                    .await;
                let _ = send.send(()).await;
            });
            cx.spawn(async move |weak, cx| {
                if receive.recv().await.is_ok() {
                    let _ = weak.update(cx, |view, cx| {
                        if view.session.session_generation() == Some(generation) {
                            view.session.expire(now());
                            if view.session.active.is_none() {
                                view.cancel_history();
                                view.conversation.clear();
                                view.history_list.reset(0);
                                // The next login restores an empty composer.
                            }
                            cx.notify();
                        }
                    });
                }
            })
            .detach();
            self.password
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.sync_composer(window, cx);
            self.load_channels(cx);
        }
    }

    fn load_channels(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.conversation.start(&self.session) {
            self.dispatch_channels(request, cx);
        }
    }

    fn refresh_channels(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.conversation.refresh_channels(&self.session) {
            self.dispatch_channels(request, cx);
            cx.notify();
        }
    }

    fn dispatch_channels(&mut self, request: ReadRequest, cx: &mut Context<Self>) {
        let api = self.session.api.clone();
        let (send, receive) = async_channel::bounded(1);
        let work = request.clone();
        runtime().spawn(async move {
            let result = api.channels(work.server, work.token).await;
            let _ = send.send(result).await;
        });
        cx.spawn(async move |weak, cx| {
            if let Ok(result) = receive.recv().await {
                let _ = weak.update_in(cx, |view, window, cx| {
                    let previous = view.conversation.selected.clone();
                    view.store_composer(cx);
                    let next =
                        view.conversation
                            .complete_channels(&mut view.session, &request, result);
                    if previous != view.conversation.selected {
                        view.sync_composer(window, cx);
                        view.cancel_history();
                        view.history_list.reset(0);
                    }
                    if let Some(next) = next {
                        view.load_history(next, cx);
                    } else if previous != view.conversation.selected {
                        view.restore_cached_history();
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn cancel_history(&mut self) {
        self.history_serial = self.history_serial.wrapping_add(1);
        if let Some(task) = self.history_task.take() {
            task.abort();
        }
    }

    fn request_older(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.conversation.request_older(&self.session) {
            self.load_history(request, cx);
            cx.notify();
        }
    }

    fn refresh_history(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.conversation.refresh_history(&self.session) {
            self.cancel_history();
            self.load_history(request, cx);
            cx.notify();
        }
    }

    fn jump_latest(&mut self, cx: &mut Context<Self>) {
        self.history_list.scroll_to_end();
        cx.notify();
    }

    fn retry_older(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.conversation.retry_older(&self.session) {
            self.load_history(request, cx);
            cx.notify();
        }
    }

    fn load_history(&mut self, request: ReadRequest, cx: &mut Context<Self>) {
        self.history_serial = self.history_serial.wrapping_add(1);
        let serial = self.history_serial;
        let api = self.session.api.clone();
        let work = request.clone();
        // The headless scheduler is single-threaded: deliver controlled fixture outcomes on
        // its own executor, without a foreign thread waking it during a GPUI frame.
        #[cfg(test)]
        cx.spawn(async move |weak, cx| {
            let result = api
                .history_page(
                    work.server,
                    work.token,
                    work.channel_id.unwrap(),
                    work.before,
                )
                .await;
            let _ = weak.update(cx, |view, cx| {
                view.finish_history(serial, request, result, cx)
            });
        })
        .detach();
        #[cfg(not(test))]
        {
            let (send, receive) = async_channel::bounded(1);
            self.history_task = Some(runtime().spawn(async move {
                let result = api
                    .history_page(
                        work.server,
                        work.token,
                        work.channel_id.unwrap(),
                        work.before,
                    )
                    .await;
                let _ = send.send(result).await;
            }));
            cx.spawn(async move |weak, cx| {
                if let Ok(result) = receive.recv().await {
                    let _ = weak.update(cx, |view, cx| {
                        view.finish_history(serial, request, result, cx)
                    });
                }
            })
            .detach();
        }
    }

    fn finish_history(
        &mut self,
        serial: u64,
        request: ReadRequest,
        result: Result<conversation::Page, session::AuthError>,
        cx: &mut Context<Self>,
    ) {
        if self.history_serial != serial {
            return;
        }
        let selected = self.conversation.selected == request.channel_id;
        let before = request.channel_id.as_ref().and_then(|id| {
            if selected {
                match self.conversation.history.get(id) {
                    Some(Load::Ready(messages)) => Some(
                        messages
                            .iter()
                            .rev()
                            .map(|m| m.id.clone())
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                }
            } else {
                None
            }
        });
        let follow = self.history_list.is_scrolled_to_end() != Some(false);
        let read_succeeded = result.is_ok();
        let outcome = self
            .conversation
            .complete_history(&mut self.session, &request, result);
        if selected && self.session.active.is_some() {
            if let Some(before) = before {
                if let Some(Load::Ready(messages)) = self
                    .conversation
                    .history
                    .get(request.channel_id.as_ref().unwrap())
                {
                    let after: Vec<_> = messages.iter().rev().map(|m| m.id.clone()).collect();
                    reconcile_history_list(&self.history_list, &before, &after);
                    if outcome.added > 0 {
                        hint_history_row_heights(&self.history_list);
                        if !outcome.prepend && follow && before.last() != after.last() {
                            self.history_list.scroll_to_end();
                        }
                    }
                }
            } else if let Some(Load::Ready(messages)) = self
                .conversation
                .history
                .get(request.channel_id.as_ref().unwrap())
            {
                self.history_list
                    .splice(0..self.history_list.item_count(), messages.len());
                hint_history_row_heights(&self.history_list);
                self.history_list.scroll_to_end();
            }
        } else if self.session.active.is_none() {
            self.history_list.reset(0);
        }
        self.history_task = None;
        if let Some(next) = outcome.next {
            self.load_history(next, cx);
        }
        // A failed or disconnected catch-up needs a *manual* retry; never spin on
        // repeated read failures while a confirmed/uncertain write waits for continuity.
        if let Some(next) = self
            .conversation
            .after_history_read(&self.session, read_succeeded)
        {
            self.cancel_history();
            self.load_history(next, cx);
        }
        cx.notify();
    }

    fn create_channel(&mut self, cx: &mut Context<Self>) {
        let name = self.channel_name.read(cx).text().to_string();
        let Some(request) = self.conversation.create(&self.session, &name) else {
            cx.notify();
            return;
        };
        let api = self.session.api.clone();
        let (send, receive) = async_channel::bounded(1);
        let work = request.clone();
        runtime().spawn(async move {
            let result = api.create_channel(work.server, work.token, work.name).await;
            let _ = send.send(result).await;
        });
        cx.spawn(async move |weak, cx| {
            if let Ok(result) = receive.recv().await {
                let _ = weak.update_in(cx, |view, window, cx| {
                    view.store_composer(cx);
                    let (confirmed, history) = view.conversation.complete_create(
                        &mut view.session,
                        &request,
                        result,
                        now(),
                    );
                    if confirmed
                        && view.channel_name.read(cx).text().to_string().trim() == request.name
                    {
                        view.channel_name
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    if let Some(history) = history {
                        view.cancel_history();
                        view.history_list.reset(0);
                        view.sync_composer(window, cx);
                        view.load_history(history, cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn restore_cached_history(&mut self) {
        if self.history_list.item_count() != 0 {
            return;
        }
        let Some(id) = self.conversation.selected.as_deref() else {
            return;
        };
        if let Some(Load::Ready(messages)) = self.conversation.history.get(id) {
            self.history_list.splice(0..0, messages.len());
            hint_history_row_heights(&self.history_list);
            self.history_list.scroll_to_end();
        }
    }

    fn select_channel(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.store_composer(cx);
        if self.conversation.selected.as_deref() != Some(id) {
            self.cancel_history();
            self.history_list.reset(0);
        }
        if let Some(request) = self.conversation.select(&self.session, id) {
            self.sync_composer(window, cx);
            self.load_history(request, cx);
        } else {
            self.sync_composer(window, cx);
            self.restore_cached_history();
        }
        cx.notify();
    }

    fn logout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_history();
        self.history_list.reset(0);
        self.conversation.clear();
        self.sync_composer(window, cx);
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
        // Protected rejection and the expiry timer can clear the model without a
        // Window; wipe the hidden textarea on the next frame, before another login.
        if self.session.active.is_none() && self.composer.read(cx).text().len() != 0 {
            self.sync_composer(window, cx);
        }
        let view = cx.entity().downgrade();
        let mut surface = div()
            .size_full()
            .bg(rgb(theme::BACKGROUND))
            .flex()
            .flex_col()
            .gap_3()
            .text_color(rgb(theme::TEXT));
        if let Some(session) = &self.session.active {
            let label = format!(
                "Logged in as {} at {}",
                session.user.username, session.server
            );
            surface = surface
                .child(
                    div()
                        .id("session-status")
                        .aria_label(label.clone())
                        .test_support()
                        .child(label),
                )
                .child(
                    Button::new("logout")
                        .label("Log out")
                        .on_click(move |_, window, cx| {
                            let _ = view.update(cx, |view, cx| view.logout(window, cx));
                        }),
                )
                .child(self.conversation_panes(cx));
        } else {
            let login_view = view.clone();
            let toggle_view = view.clone();
            surface = surface
                .justify_center()
                .items_center()
                .child(div().text_xl().child(if self.signup {
                    "Sign up for Hamlet"
                } else {
                    "Log in to Hamlet"
                }))
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
                    Button::new(if self.signup { "signup" } else { "login" })
                        .disabled(self.login_disabled())
                        .label(if self.session.pending {
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
                        .on_click(move |_, _, cx| {
                            let _ = login_view.update(cx, |view, cx| view.submit(cx));
                        }),
                )
                .child(
                    Button::new("auth-mode")
                        .disabled(self.session.pending)
                        .label(if self.signup {
                            "Have a user? Log in"
                        } else {
                            "New user? Sign up"
                        })
                        .on_click(move |_, _, cx| {
                            let _ = toggle_view
                                .update(cx, |view, cx| view.set_signup(!view.signup, cx));
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

impl Hamlet {
    fn conversation_panes(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let create_view = view.clone();
        let refresh_channels_view = view.clone();
        let mut sidebar = div()
            .id("channels")
            .test_support()
            .w(px(220.))
            .h_full()
            .bg(rgb(theme::SIDEBAR))
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .overflow_y_scroll()
            .child(div().font_weight(FontWeight::BOLD).child("Text channels"))
            .child(
                Button::new("refresh-channels")
                    .label(
                        if self.conversation.channels.is_some()
                            && self.conversation.channel_refreshing()
                        {
                            "Refreshing channels…"
                        } else {
                            "Refresh channels"
                        },
                    )
                    .disabled(self.conversation.channel_refreshing())
                    .on_click(move |_, _, cx| {
                        let _ =
                            refresh_channels_view.update(cx, |view, cx| view.refresh_channels(cx));
                    }),
            )
            .child(
                div().child("Channel name").child(
                    Input::new(&self.channel_name)
                        .id("channel-name")
                        .aria_label("Channel name"),
                ),
            )
            .child(
                Button::new("create-channel")
                    .disabled(
                        self.conversation.create_pending
                            || !matches!(self.conversation.channels, Some(Load::Ready(_))),
                    )
                    .label(if self.conversation.create_pending {
                        "Creating channel…"
                    } else {
                        "Create text channel"
                    })
                    .on_click(move |_, _, cx| {
                        let _ = create_view.update(cx, |view, cx| view.create_channel(cx));
                    }),
            );
        if let Some(feedback) = &self.conversation.create_feedback {
            sidebar = sidebar.child(
                div()
                    .id("channel-feedback")
                    .aria_label(feedback.clone())
                    .test_support()
                    .child(feedback.clone()),
            );
        }
        if let Some(error) = &self.conversation.channel_error {
            sidebar = sidebar.child(
                div()
                    .id("channels-refresh-error")
                    .aria_label(error.clone())
                    .test_support()
                    .child(format!("Channel refresh: {error}")),
            );
        }
        match &self.conversation.channels {
            None | Some(Load::Loading) => sidebar = sidebar.child("Loading channels…"),
            Some(Load::Failed(error)) => sidebar = sidebar.child(format!("Channels: {error}")),
            Some(Load::Ready(channels)) if channels.is_empty() => {
                sidebar = sidebar.child("No text channels yet.")
            }
            Some(Load::Ready(channels)) => {
                for channel in channels {
                    let id = channel.id.clone();
                    let selected = self.conversation.selected.as_deref() == Some(&id);
                    let label = format!("# {}", channel.name);
                    let target = view.clone();
                    sidebar = sidebar.child(
                        Button::new(format!("channel-{id}"))
                            .label(label)
                            .icon(theme::channel_icon())
                            .on_click(move |_, window, cx| {
                                let _ = target
                                    .update(cx, |view, cx| view.select_channel(&id, window, cx));
                            })
                            .when(selected, |button| button.bg(rgb(theme::SELECTED))),
                    );
                }
            }
        }
        let selected = self.conversation.selected.as_deref();
        let focus = self.history_focus.clone();
        let list_for_wheel = self.history_list.clone();
        let mut history = div()
            .id("history-pane")
            .test_support()
            .track_focus(&self.history_focus)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx)
            })
            .on_scroll_wheel(move |_, _, _| {
                // GPUI invalidates all height hints when the list first learns its width.
                // Re-seed offscreen estimates after that layout, without measuring all rows.
                if list_for_wheel.is_scrolled_to_end().is_none() {
                    hint_history_row_heights(&list_for_wheel);
                }
            })
            .flex_1()
            .h_full()
            .min_w_0()
            .p_4()
            .flex()
            .flex_col()
            .gap_2();
        if let Some(id) = selected {
            let name = match &self.conversation.channels {
                Some(Load::Ready(channels)) => channels
                    .iter()
                    .find(|channel| channel.id == id)
                    .map(|channel| channel.name.as_str())
                    .unwrap_or("Channel"),
                _ => "Channel",
            };
            history = history.child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::BOLD)
                    .child(format!("# {name}")),
            );
            let refreshing = matches!(
                self.conversation.refreshing.get(id),
                Some(conversation::Refresh::Running)
            );
            let refresh_view = view.clone();
            history = history.child(
                Button::new("refresh-history")
                    .label(if refreshing {
                        "Refreshing conversation…"
                    } else {
                        "Refresh conversation"
                    })
                    .disabled(
                        refreshing
                            || matches!(self.conversation.history.get(id), Some(Load::Loading)),
                    )
                    .on_click(move |_, _, cx| {
                        let _ = refresh_view.update(cx, |view, cx| view.refresh_history(cx));
                    }),
            );
            if let Some(conversation::Refresh::Incomplete(error)) =
                self.conversation.refreshing.get(id)
            {
                history = history.child(
                    div()
                        .id("catchup-incomplete")
                        .aria_label(error.clone())
                        .test_support()
                        .child(format!("Catch-up incomplete: {error}")),
                );
            }
            match self.conversation.history.get(id) {
                None | Some(Load::Loading) => history = history.child("Loading conversation…"),
                Some(Load::Failed(error)) => {
                    history = history.child(format!("Conversation: {error}"))
                }
                Some(Load::Ready(messages)) if messages.is_empty() => {
                    history = history.child("No messages in this channel yet.")
                }
                Some(Load::Ready(messages)) => {
                    match self.conversation.older.get(id) {
                        Some(Older::Loading) => history = history.child("Loading older messages…"),
                        Some(Older::Failed(error)) => {
                            let target = view.clone();
                            history = history.child(
                                div().child(format!("Older messages: {error}")).child(
                                    Button::new("retry-older")
                                        .label("Retry older messages")
                                        .on_click(move |_, _, cx| {
                                            let _ =
                                                target.update(cx, |view, cx| view.retry_older(cx));
                                        }),
                                ),
                            );
                        }
                        Some(Older::Exhausted) => history = history.child("Start of conversation."),
                        _ => {}
                    }
                    let jump = view.clone();
                    history = history.child(
                        Button::new("jump-latest")
                            .label("Jump to latest")
                            .disabled(self.history_list.is_scrolled_to_end() != Some(false))
                            .on_click(move |_, _, cx| {
                                let _ = jump.update(cx, |view, cx| view.jump_latest(cx));
                            }),
                    );
                    // Server order is newest-first. Reverse for a chronological scroll surface;
                    // ListState::splice preserves the reader's anchor across variable heights.
                    let rows = messages.iter().rev().cloned().collect::<Vec<_>>();
                    history = history.child(
                        div().id("history").test_support().flex_1().min_h_0().child(
                            list(self.history_list.clone(), move |ix, _, _| {
                                let message = &rows[ix];
                                div()
                                    .id(format!("message-{}", message.id))
                                    .aria_label(message.text.clone())
                                    .test_support()
                                    .w_full()
                                    .p_2()
                                    .flex()
                                    .flex_col()
                                    .child(div().text_color(rgb(theme::MUTED)).child(format!(
                                        "{} · {}",
                                        message.author_name, message.created_at
                                    )))
                                    .child(
                                        div()
                                            .id(format!("message-text-{}", message.id))
                                            .test_support()
                                            .child(SelectableText::new(
                                                format!("text-{}", message.id),
                                                message.text.clone(),
                                            )),
                                    )
                                    .into_any_element()
                            })
                            .size_full(),
                        ),
                    );
                }
            }
            let sending = self.conversation.send_pending.contains_key(id);
            let composer_focus = self.composer.read(cx).focus_handle(cx);
            let send_view = view.clone();
            history = history.child(
                div()
                    .id("composer-panel")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .id("composer")
                            .test_support()
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                window.focus(&composer_focus, cx);
                                cx.stop_propagation();
                            })
                            .child(
                                Textarea::new(&self.composer)
                                    .aria_label(format!("Message to {name}"))
                                    .readonly(sending)
                                    .h(px(90.)),
                            ),
                    )
                    .child(
                        Button::new("send-message")
                            .icon(gpui_kit::assets::IconName::Send)
                            .tooltip("Send message (Enter); Shift+Enter adds a line")
                            .label(if sending {
                                "Sending…"
                            } else {
                                "Send message"
                            })
                            .disabled(sending)
                            .on_click(move |_, _, cx| {
                                let _ = send_view.update(cx, |view, cx| view.send_message(cx));
                            }),
                    )
                    .when_some(self.conversation.send_feedback.get(id), |pane, feedback| {
                        pane.child(
                            div()
                                .id("send-feedback")
                                .aria_label(feedback.clone())
                                .test_support()
                                .child(feedback.clone()),
                        )
                    }),
            );
        } else if !matches!(self.conversation.channels, Some(Load::Ready(ref items)) if items.is_empty())
        {
            history = history.child("Select a text channel to read its conversation.");
        }
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(sidebar)
            .child(history)
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
        AppContext as _, Context, FocusHandle, Focusable as _, InteractiveElement as _,
        IntoElement, ListAlignment, ListState, MouseButton, ParentElement as _, Render,
        SharedString, Styled as _, TestAppContext, Window, div, list, px,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
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
        fn signup(
            &self,
            _: String,
            username: String,
            _: String,
        ) -> ApiFuture<Result<Login, AuthError>> {
            self.login(String::new(), username, String::new())
        }
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

    #[gpui_kit::test]
    fn composer_uses_real_textarea_keyboard_button_focus_and_channel_drafts(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let calls = Arc::new(std::sync::Mutex::new(Vec::<Sent>::new()));
        let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = saved.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(SendAuth(calls.clone()))));
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            assert!(
                view.read(cx)
                    .composer
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
            );
            window.input("first", cx);
            window.press("shift-enter", cx);
            window.input("second", cx);
            assert_eq!(
                view.read(cx).composer.read(cx).text().to_string(),
                "first\nsecond"
            );
            window.click("channel-000000000000002", cx);
            window.render_frame(cx);
            window.click("composer", cx);
            window.input("other", cx);
            window.click("channel-000000000000001", cx);
            window.render_frame(cx);
            assert_eq!(
                view.read(cx).composer.read(cx).text().to_string(),
                "first\nsecond"
            );
            window.click("composer", cx);
            assert!(
                view.read(cx)
                    .composer
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
            );
            window.dispatch_action(
                Box::new(gpui_kit::base::input::Enter {
                    secondary: false,
                    shift: false,
                }),
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("send-message").label(), Some("Sending…"));
            assert_eq!(
                view.read(cx).composer.read(cx).text().to_string(),
                "first\nsecond"
            );
            window.input("blocked", cx);
            assert_eq!(
                view.read(cx).composer.read(cx).text().to_string(),
                "first\nsecond"
            );
            window.click("send-message", cx);
            window.dispatch_action(
                Box::new(gpui_kit::base::input::Enter {
                    secondary: false,
                    shift: false,
                }),
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(calls.lock().unwrap().len(), 1);
        let (_, text, sender) = calls.lock().unwrap().remove(0);
        assert_eq!(text, "first\nsecond");
        sender.try_send(Err(AuthError::InvalidInput)).unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(
                view.read(cx).composer.read(cx).text().to_string(),
                "first\nsecond"
            );
            assert!(
                window
                    .find("send-feedback")
                    .label()
                    .unwrap()
                    .contains("rejected")
            );
            window.click("channel-000000000000002", cx);
            window.render_frame(cx);
            assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "other");
            window.click("send-message", cx);
        });
        cx.run_until_parked();
        assert_eq!(calls.lock().unwrap().len(), 1);
        let (_, text, sender) = calls.lock().unwrap().remove(0);
        assert_eq!(text, "other");
        sender
            .try_send(Ok(crate::conversation::Message {
                id: "000000000000003".into(),
                channel_id: "000000000000002".into(),
                author_id: "42".into(),
                author_name: "Ada".into(),
                text: "other".into(),
                created_at: "2027-01-01T00:00:00Z".into(),
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "");
            assert_eq!(
                window.find("message-000000000000003").label(),
                Some("other")
            );
            window.click("channel-000000000000001", cx);
            window.render_frame(cx);
            assert_eq!(window.find("send-message").label(), Some("Send message"));
            assert_eq!(
                view.read(cx).composer.read(cx).text().to_string(),
                "first\nsecond"
            );
            window.click("logout", cx);
            assert!(view.read(cx).conversation.drafts.is_empty());
        });
    }

    #[gpui_kit::test]
    fn expiry_wipes_hidden_composer_before_a_new_login(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let stored = std::rc::Rc::new(std::cell::RefCell::new(None));
        let saved = stored.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
            *saved.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = stored.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            window.input("secret draft", cx);
            view.update(cx, |view, _| {
                view.session.expire(i64::MAX);
                view.conversation.clear();
            });
            window.render_frame(cx);
            assert!(view.read(cx).session.active.is_none());
            assert!(view.read(cx).conversation.drafts.is_empty());
            assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "");
        });
    }

    #[gpui_kit::test]
    fn enter_at_mid_caret_and_after_shift_enter_sends_unchanged_text(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let calls = Arc::new(std::sync::Mutex::new(Vec::<Sent>::new()));
        let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = saved.clone();
        let sender = calls.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(SendAuth(sender))));
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            window.input("middle", cx);
            view.update(cx, |v, cx| {
                v.composer.update(cx, |input, cx| {
                    input.set_cursor_position(
                        gpui_kit::base::input::Position {
                            line: 0,
                            character: 3,
                        },
                        window,
                        cx,
                    );
                });
            });
            assert_eq!(
                view.read(cx).composer.read(cx).cursor_position().character,
                3
            );
            assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "middle");
            window.dispatch_event(
                gpui_kit::PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
                    keystroke: gpui_kit::Keystroke::parse("enter").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
        });
        cx.run_until_parked();
        let (_, text, reply) = calls.lock().unwrap().remove(0);
        assert_eq!(text, "middle");
        reply.try_send(Err(AuthError::InvalidInput)).unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                v.composer
                    .update(cx, |input, cx| input.set_value("tail", window, cx));
                v.conversation.set_draft("000000000000001", "tail".into());
            });
            window.render_frame(cx);
            window.click("composer", cx);
            window.press("shift-enter", cx);
            assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "tail\n");
            window.dispatch_event(
                gpui_kit::PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
                    keystroke: gpui_kit::Keystroke::parse("enter").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
        });
        cx.run_until_parked();
        let (_, text, reply) = calls.lock().unwrap().remove(0);
        assert_eq!(text, "tail\n");
        reply.try_send(Err(AuthError::InvalidInput)).unwrap();
        cx.run_until_parked();
        cx.update(|_window, cx| {
            assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "tail\n");
        });
    }

    #[gpui_kit::test]
    fn uncertain_send_retains_draft_refreshes_only_selected_channel_and_never_replays(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let calls = Arc::new(std::sync::Mutex::new(Vec::<Sent>::new()));
        let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = saved.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(SendAuth(calls.clone()))));
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            window.input("maybe published", cx);
            window.click("send-message", cx);
        });
        cx.run_until_parked();
        assert_eq!(calls.lock().unwrap().len(), 1);
        let (_, _, sender) = calls.lock().unwrap().remove(0);
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("channel-000000000000002", cx);
            window.render_frame(cx);
            assert_eq!(window.find("send-message").label(), Some("Send message"));
        });
        sender.try_send(Err(AuthError::Unavailable)).unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                view.read(cx)
                    .conversation
                    .uncertain
                    .contains("000000000000001")
            );
            assert!(view.read(cx).conversation.refreshing.is_empty());
            window.click("channel-000000000000001", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(
                view.read(cx).composer.read(cx).text().to_string(),
                "maybe published"
            );
            assert!(
                window
                    .find("send-feedback")
                    .label()
                    .unwrap()
                    .contains("may already")
            );
            assert!(
                !view
                    .read(cx)
                    .conversation
                    .uncertain
                    .contains("000000000000001")
            );
            assert!(
                calls.lock().unwrap().is_empty(),
                "refresh must not replay the write"
            );
        });
    }

    #[gpui_kit::test]
    fn stalled_send_times_out_and_late_completion_cannot_clear_the_draft(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let calls = Arc::new(std::sync::Mutex::new(Vec::<Sent>::new()));
        let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = saved.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(SendAuth(calls.clone()))));
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            window.input("timeout draft", cx);
            window.click("send-message", cx);
        });
        cx.run_until_parked();
        assert_eq!(calls.lock().unwrap().len(), 1);
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(10));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("send-message").label(), Some("Send message"));
            assert_eq!(
                view.read(cx).composer.read(cx).text().to_string(),
                "timeout draft"
            );
            assert!(
                window
                    .find("send-feedback")
                    .label()
                    .unwrap()
                    .contains("may already")
            );
            assert!(view.read(cx).conversation.send_pending.is_empty());
        });
        // The fixture's future was dropped rather than retried, so delivery is impossible.
        let (_, _, sender) = calls.lock().unwrap().remove(0);
        assert!(sender.try_send(Err(AuthError::AlreadyInvalid)).is_err());
        assert!(view.read_with(cx, |v, _| v.session.active.is_some()));
    }

    struct PendingAuth(Arc<AtomicUsize>);
    impl AuthApi for PendingAuth {
        fn signup(&self, _: String, _: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::pending())
        }
        fn login(&self, _: String, _: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::pending())
        }
        fn logout(&self, _: String, _: String) -> ApiFuture<Result<(), AuthError>> {
            Box::pin(async { Ok(()) })
        }
        fn channels(
            &self,
            _: String,
            _: String,
        ) -> ApiFuture<Result<Vec<crate::conversation::Channel>, AuthError>> {
            Box::pin(async { Ok(vec![]) })
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
            _: String,
        ) -> ApiFuture<Result<Vec<crate::conversation::Message>, AuthError>> {
            Box::pin(async { Ok(vec![]) })
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

    struct SignupReject {
        error: AuthError,
        submissions: Arc<std::sync::Mutex<Vec<(String, String)>>>,
    }
    impl AuthApi for SignupReject {
        fn signup(
            &self,
            _: String,
            username: String,
            password: String,
        ) -> ApiFuture<Result<Login, AuthError>> {
            self.submissions.lock().unwrap().push((username, password));
            let error = self.error.clone();
            Box::pin(async move { Err(error) })
        }
        fn login(&self, _: String, _: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
            Box::pin(async { unreachable!() })
        }
        fn logout(&self, _: String, _: String) -> ApiFuture<Result<(), AuthError>> {
            Box::pin(async { Ok(()) })
        }
        fn channels(
            &self,
            _: String,
            _: String,
        ) -> ApiFuture<Result<Vec<crate::conversation::Channel>, AuthError>> {
            Box::pin(async { unreachable!() })
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
            _: String,
        ) -> ApiFuture<Result<Vec<crate::conversation::Message>, AuthError>> {
            Box::pin(async { unreachable!() })
        }
    }

    #[gpui_kit::test]
    fn signup_rejection_keeps_form_and_reports_uncertainty(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        for (error, expected) in [
            (AuthError::Conflict, "already exists"),
            (AuthError::Unavailable, "may have succeeded"),
        ] {
            let submissions = Arc::new(std::sync::Mutex::new(Vec::new()));
            let captured = submissions.clone();
            let (_, cx) = cx.add_window_view(|window, cx| {
                let view = cx.new(|cx| {
                    Hamlet::new(
                        window,
                        cx,
                        Arc::new(SignupReject {
                            error,
                            submissions: captured,
                        }),
                    )
                });
                Root::new(view, window, cx)
            });
            cx.update(|window, cx| {
                window.render_frame(cx);
                window.click("auth-mode", cx);
                window.click("username", cx);
                window.input("Alice_1", cx);
                window.click("password", cx);
                window.input("long password", cx);
                window.click("signup", cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.render_frame(cx);
                assert!(
                    window
                        .find("auth-feedback")
                        .label()
                        .unwrap()
                        .contains(expected)
                );
                assert_eq!(window.find("username").value(), Some("Alice_1"));
                assert_eq!(window.find("signup").label(), Some("Create user"));
                // A deliberate correction submits through the same controls without retyping
                // the password; the outward request proves recoverable input survived.
                window.click("username", cx);
                window.press("ctrl-a", cx);
                window.input("Alice_2", cx);
                window.click("signup", cx);
            });
            cx.run_until_parked();
            assert_eq!(
                submissions.lock().unwrap().as_slice(),
                &[
                    ("Alice_1".into(), "long password".into()),
                    ("Alice_2".into(), "long password".into())
                ]
            );
        }
    }

    #[gpui_kit::test]
    fn signup_form_supports_keyboard_focus(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
            Root::new(view, window, cx)
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("auth-mode", cx);
            window.click("server-url", cx);
            assert_eq!(window.find("server-url").focused(), Some(true));
            window.press("tab", cx);
            assert_eq!(window.find("username").focused(), Some(true));
            window.input("Alice", cx);
            assert_eq!(window.find("username").value(), Some("Alice"));
        });
    }

    #[gpui_kit::test]
    fn signup_controls_validate_and_enter_conversation(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|window, cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("auth-mode", cx);
            window.render_frame(cx);
            assert_eq!(window.find("signup").label(), Some("Create user"));
            window.click("username", cx);
            window.input("bad!", cx);
            window.click("password", cx);
            window.input("short", cx);
            window.click("signup", cx);
            window.render_frame(cx);
            assert!(
                window
                    .find("auth-feedback")
                    .label()
                    .unwrap()
                    .contains("3–32")
            );
            window.click("username", cx);
            window.press("ctrl-a", cx);
            window.input("Alice_1", cx);
            window.click("signup", cx);
            window.render_frame(cx);
            assert!(
                window
                    .find("auth-feedback")
                    .label()
                    .unwrap()
                    .contains("8–256")
            );
            window.click("password", cx);
            window.press("ctrl-a", cx);
            window.input("long password", cx);
            window.click("signup", cx);
            window.render_frame(cx);
            assert_eq!(window.find("signup").label(), Some("Creating user…"));
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find("session-status")
                    .label()
                    .unwrap()
                    .contains("Alice_1")
            );
            assert_eq!(
                window.find("message-000000000000001").label(),
                Some("first line\nsecond line")
            );
            window.click("logout", cx);
            window.render_frame(cx);
            assert!(window.find("password").value().is_none_or(str::is_empty));
        });
    }

    #[gpui_kit::test]
    fn pending_signup_is_inert_and_preserves_editable_inputs(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let calls = Arc::new(AtomicUsize::new(0));
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PendingAuth(calls.clone()))));
            Root::new(view, window, cx)
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("auth-mode", cx);
            window.click("username", cx);
            window.input("Alice", cx);
            window.click("password", cx);
            window.input("long password", cx);
            window.click("signup", cx);
            window.render_frame(cx);
            window.click("signup", cx);
            window.render_frame(cx);
            assert_eq!(window.find("signup").label(), Some("Creating user…"));
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while calls.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        cx.update(|window, cx| {
            window.click("auth-mode", cx);
            window.render_frame(cx);
            assert_eq!(window.find("signup").label(), Some("Creating user…"));
            assert_eq!(window.find("username").value(), Some("Alice"));
        });
    }

    #[gpui_kit::test]
    fn real_controls_read_selected_conversation(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|window, cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find("channel-000000000000001")
                    .label()
                    .unwrap()
                    .contains("alpha")
            );
            assert_eq!(
                window.find("message-000000000000001").label(),
                Some("first line\nsecond line")
            );
        });
        cx.update(|window, cx| {
            window.click("channel-000000000000002", cx);
            window.render_frame(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.find("message-000000000000002").bounds().size.height > px(0.));
            window.click("logout", cx);
            window.render_frame(cx);
            assert_eq!(window.find("login").label(), Some("Log in"));
        });
    }

    struct CreateAuth {
        results: Arc<
            std::sync::Mutex<
                std::collections::VecDeque<Result<crate::conversation::Channel, AuthError>>,
            >,
        >,
        calls: Arc<AtomicUsize>,
        empty: bool,
    }
    impl AuthApi for CreateAuth {
        fn signup(
            &self,
            _: String,
            username: String,
            _: String,
        ) -> ApiFuture<Result<Login, AuthError>> {
            TestAuth.login(String::new(), username, String::new())
        }
        fn login(
            &self,
            _: String,
            username: String,
            _: String,
        ) -> ApiFuture<Result<Login, AuthError>> {
            TestAuth.login(String::new(), username, String::new())
        }
        fn logout(&self, _: String, _: String) -> ApiFuture<Result<(), AuthError>> {
            Box::pin(async { Ok(()) })
        }
        fn channels(
            &self,
            _: String,
            _: String,
        ) -> ApiFuture<Result<Vec<crate::conversation::Channel>, AuthError>> {
            let empty = self.empty;
            Box::pin(async move {
                if empty {
                    Ok(vec![])
                } else {
                    Ok(vec![
                        crate::conversation::Channel {
                            id: "1".into(),
                            name: "alpha".into(),
                        },
                        crate::conversation::Channel {
                            id: "2".into(),
                            name: "zebra".into(),
                        },
                    ])
                }
            })
        }
        fn create_channel(
            &self,
            _: String,
            _: String,
            _: String,
        ) -> ApiFuture<Result<crate::conversation::Channel, AuthError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let result = self.results.lock().unwrap().pop_front().unwrap();
            Box::pin(async move { result })
        }
        fn history(
            &self,
            _: String,
            _: String,
            _: String,
        ) -> ApiFuture<Result<Vec<crate::conversation::Message>, AuthError>> {
            Box::pin(async { Ok(vec![]) })
        }
    }

    #[gpui_kit::test]
    fn create_controls_confirm_order_selection_and_empty_history(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        for empty in [false, true] {
            let results = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from([
                Ok(crate::conversation::Channel {
                    id: "3".into(),
                    name: "Middle".into(),
                }),
            ])));
            let calls = Arc::new(AtomicUsize::new(0));
            let probe = std::rc::Rc::new(std::cell::RefCell::new(None));
            let stored = probe.clone();
            let (_, cx) = cx.add_window_view(|window, cx| {
                let view = cx.new(|cx| {
                    Hamlet::new(
                        window,
                        cx,
                        Arc::new(CreateAuth {
                            results: results.clone(),
                            calls: calls.clone(),
                            empty,
                        }),
                    )
                });
                *stored.borrow_mut() = Some(view.clone());
                Root::new(view, window, cx)
            });
            let view: gpui_kit::Entity<Hamlet> = probe.borrow().as_ref().unwrap().clone();
            cx.update(|window, cx| {
                window.render_frame(cx);
                window.click("username", cx);
                window.input("Ada", cx);
                window.click("password", cx);
                window.input("pass", cx);
                window.click("login", cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.render_frame(cx);
                window.click("channel-name", cx);
                window.input("  Middle  ", cx);
                window.click("create-channel", cx);
                window.render_frame(cx);
                assert_eq!(
                    window.find("create-channel").label(),
                    Some("Creating channel…")
                );
                assert_eq!(window.find("channel-name").value(), Some("  Middle  "));
                assert_eq!(
                    view.read(cx).conversation.selected.as_deref(),
                    if empty { None } else { Some("1") }
                );
                window.click("create-channel", cx);
                assert_eq!(
                    view.read(cx).conversation.selected.as_deref(),
                    if empty { None } else { Some("1") }
                );
            });
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.render_frame(cx);
                assert_eq!(window.find("channel-name").value(), Some(""));
                assert_eq!(view.read(cx).conversation.selected.as_deref(), Some("3"));
                assert_eq!(window.find("channel-3").label(), Some("# Middle"));
                assert_eq!(
                    view.read(cx)
                        .conversation
                        .channels
                        .as_ref()
                        .and_then(|list| match list {
                            crate::conversation::Load::Ready(items) =>
                                Some(items.iter().map(|c| c.id.as_str()).collect::<Vec<_>>()),
                            _ => None,
                        }),
                    Some(if empty {
                        vec!["3"]
                    } else {
                        vec!["1", "3", "2"]
                    })
                );
                assert!(
                    view.read(cx).conversation.history.get("3")
                        == Some(&crate::conversation::Load::Ready(vec![]))
                );
            });
        }
    }

    #[gpui_kit::test]
    fn create_controls_keep_input_on_errors_without_replay(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let results = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from([
            Err(AuthError::Conflict),
            Err(AuthError::Unavailable),
        ])));
        let calls = Arc::new(AtomicUsize::new(0));
        let probe = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = probe.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                Hamlet::new(
                    window,
                    cx,
                    Arc::new(CreateAuth {
                        results: results.clone(),
                        calls: calls.clone(),
                        empty: true,
                    }),
                )
            });
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = probe.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("channel-name", cx);
            window.input("bad!", cx);
            window.click("create-channel", cx);
            window.render_frame(cx);
            assert!(
                window
                    .find("channel-feedback")
                    .label()
                    .unwrap()
                    .contains("1–64")
            );
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            window.click("channel-name", cx);
            window.press("ctrl-a", cx);
            window.input("Duplicate", cx);
            window.click("create-channel", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find("channel-feedback")
                    .label()
                    .unwrap()
                    .contains("already exists")
            );
            assert_eq!(window.find("channel-name").value(), Some("Duplicate"));
            window.click("create-channel", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find("channel-feedback")
                    .label()
                    .unwrap()
                    .contains("may have succeeded")
            );
            assert_eq!(window.find("channel-name").value(), Some("Duplicate"));
            assert_eq!(calls.load(Ordering::SeqCst), 2); // no automatic replay
            window.click("logout", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(view.read(cx).conversation.channels.is_none());
            assert!(view.read(cx).conversation.selected.is_none());
            assert_eq!(window.find("login").label(), Some("Log in"));
        });
    }

    #[gpui_kit::test]
    fn create_completion_after_logout_cannot_navigate(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let results = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from([
            Ok(crate::conversation::Channel {
                id: "3".into(),
                name: "Late".into(),
            }),
        ])));
        let probe = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = probe.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                Hamlet::new(
                    window,
                    cx,
                    Arc::new(CreateAuth {
                        results,
                        calls: Arc::new(AtomicUsize::new(0)),
                        empty: true,
                    }),
                )
            });
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = probe.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("channel-name", cx);
            window.input("Late", cx);
            window.click("create-channel", cx);
            window.click("logout", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("login").label(), Some("Log in"));
            assert!(view.read(cx).conversation.channels.is_none());
            assert!(view.read(cx).conversation.selected.is_none());
        });
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

    #[gpui_kit::test]
    fn production_wheel_requests_older_and_keeps_reader_at_same_viewport_y(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let (tx, requests) = std::sync::mpsc::channel();
        let stored = std::rc::Rc::new(std::cell::RefCell::new(None));
        let saved = stored.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PagedAuth(tx))));
            *saved.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = stored.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        let (cursor, reply) = requests
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        assert_eq!(cursor, None);
        let message = |ix: i32| crate::conversation::Message {
            id: ix.to_string(),
            channel_id: "000000000000001".into(),
            author_id: "42".into(),
            author_name: "Ada".into(),
            text: if ix % 2 == 0 {
                "a long line that wraps at narrow widths\nwith another line".into()
            } else {
                "short".into()
            },
            created_at: "same".into(),
        };
        reply
            .send_blocking(Ok(crate::conversation::Page {
                items: (1..=40).rev().map(message).collect(),
                next_cursor: Some("server cursor only".into()),
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(view.read(cx).history_list.logical_scroll_top().item_ix > 1);
            // Find the threshold by scrolling the actual list, not by fixing screen coordinates.
            for _ in 0..100 {
                if view.read(cx).history_list.logical_scroll_top().item_ix <= 1 {
                    break;
                }
                window.scroll(
                    "history",
                    gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(90.))),
                    cx,
                );
                window.render_frame(cx);
            }
            assert!(
                view.read(cx).history_list.logical_scroll_top().item_ix <= 1,
                "offset={:?} end={:?}",
                view.read(cx).history_list.logical_scroll_top(),
                view.read(cx).history_list.is_scrolled_to_end()
            );
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(90.))),
                cx,
            );
        });
        cx.run_until_parked();
        let (cursor, reply) = requests
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        assert_eq!(cursor.as_deref(), Some("server cursor only"));
        let anchor = std::rc::Rc::new(std::cell::RefCell::new(None));
        let saved_anchor = anchor.clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            // Repeated wheel input cannot start a second in-flight request.
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(90.))),
                cx,
            );
            window.render_frame(cx);
            assert!(requests.try_recv().is_err());
            let ix = view.read(cx).history_list.logical_scroll_top().item_ix;
            let id = format!("message-{}", ix + 1);
            *saved_anchor.borrow_mut() = Some((id.clone(), window.find(id).bounds().origin.y, ix));
        });
        reply.send_blocking(Err(AuthError::Unavailable)).unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("retry-older").label(),
                Some("Retry older messages")
            );
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(90.))),
                cx,
            );
            window.render_frame(cx);
            assert!(
                requests.try_recv().is_err(),
                "failed pages must not retry on wheel input"
            );
            window.click("retry-older", cx);
        });
        cx.run_until_parked();
        let (cursor, retry) = requests
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        assert_eq!(cursor.as_deref(), Some("server cursor only"));
        cx.update(|window, cx| {
            window.render_frame(cx);
            let ix = view.read(cx).history_list.logical_scroll_top().item_ix;
            let id = format!("message-{}", ix + 1);
            *anchor.borrow_mut() = Some((id.clone(), window.find(id).bounds().origin.y, ix));
        });
        retry
            .send_blocking(Ok(crate::conversation::Page {
                items: vec![message(1), message(0), message(-1)],
                next_cursor: None,
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            let (id, y, ix) = anchor.borrow().clone().unwrap();
            assert_eq!(
                view.read(cx).history_list.logical_scroll_top().item_ix,
                ix + 2
            );
            assert_eq!(window.find(id).bounds().origin.y, y);
            assert!(requests.try_recv().is_err());
        });
    }

    #[gpui_kit::test]
    fn confirmed_middle_insertion_keeps_reader_anchor(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (tx, requests) = std::sync::mpsc::channel();
        let stored = std::rc::Rc::new(std::cell::RefCell::new(None));
        let saved = stored.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PagedAuth(tx))));
            *saved.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = stored.borrow().as_ref().unwrap().clone();
        let m = |ix: i32| crate::conversation::Message {
            id: ix.to_string(),
            channel_id: "000000000000001".into(),
            author_id: "42".into(),
            author_name: "Ada".into(),
            text: format!("message {ix}"),
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let original: Vec<_> = (1..=40).rev().filter(|&ix| ix != 20).map(m).collect();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        let (_, initial) = requests
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        initial
            .send_blocking(Ok(crate::conversation::Page {
                items: original.clone(),
                next_cursor: None,
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                v.history_list.scroll_to(gpui_kit::ListOffset {
                    item_ix: 23,
                    offset_in_item: px(0.),
                });
                v.conversation.set_draft("000000000000001", "twenty".into());
                let send = v.conversation.send(&v.session).unwrap();
                assert_eq!(
                    v.conversation
                        .complete_send(&mut v.session, &send, Ok(m(20)), 0),
                    crate::conversation::SendOutcome::Confirmed
                );
                let read = v.conversation.reconcile_confirmed(&v.session).unwrap();
                v.load_history(read, cx);
            });
            window.render_frame(cx);
            assert_eq!(view.read(cx).history_list.logical_scroll_top().item_ix, 23);
            assert!(window.find("message-25").bounds().size.height > px(0.));
        });
        cx.run_until_parked();
        let (_, refresh) = requests
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        let y = cx.update(|window, cx| {
            window.render_frame(cx);
            window.find("message-25").bounds().origin.y
        });
        refresh
            .send_blocking(Ok(crate::conversation::Page {
                items: original,
                next_cursor: None,
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(view.read(cx).history_list.logical_scroll_top().item_ix, 24);
            assert_eq!(window.find("message-25").bounds().origin.y, y);
            assert_eq!(view.read(cx).history_list.item_count(), 40);
            let crate::conversation::Load::Ready(messages) = view
                .read(cx)
                .conversation
                .history
                .get("000000000000001")
                .unwrap()
            else {
                panic!("history not ready")
            };
            assert_eq!(messages.iter().filter(|m| m.id == "20").count(), 1);
        });
    }

    #[gpui_kit::test]
    fn refresh_channels_control_keeps_selected_conversation(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|window, cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("channel-000000000000002", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("message-000000000000002").label(),
                Some("other channel")
            );
            window.click("refresh-channels", cx);
            window.render_frame(cx);
            assert_eq!(
                window.find("refresh-channels").label(),
                Some("Refreshing channels…")
            );
            window.click("refresh-channels", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("refresh-channels").label(),
                Some("Refresh channels")
            );
            assert_eq!(
                window.find("message-000000000000002").label(),
                Some("other channel")
            );
            window.click("refresh-history", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("message-000000000000002").label(),
                Some("other channel")
            );
        });
    }

    struct RemovingChannel(Arc<AtomicBool>);
    impl AuthApi for RemovingChannel {
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
            let removed = self.0.load(Ordering::SeqCst);
            Box::pin(async move {
                let mut list = TestAuth.channels(server, token).await?;
                if removed {
                    list.remove(0);
                }
                Ok(list)
            })
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
            server: String,
            token: String,
            id: String,
        ) -> ApiFuture<Result<Vec<crate::conversation::Message>, AuthError>> {
            TestAuth.history(server, token, id)
        }
    }

    #[gpui_kit::test]
    fn channel_discovery_shows_cached_fallback_after_selected_channel_disappears(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let remove = Arc::new(AtomicBool::new(false));
        let removed = remove.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(RemovingChannel(removed))));
            Root::new(view, window, cx)
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("channel-000000000000002", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("message-000000000000002").label(),
                Some("other channel")
            );
            window.click("channel-000000000000001", cx);
        });
        cx.run_until_parked();
        remove.store(true, Ordering::SeqCst);
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("refresh-channels", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("message-000000000000002").label(),
                Some("other channel")
            );
        });
    }

    #[gpui_kit::test]
    fn refresh_controls_preserve_reader_and_jump_follows_later_messages(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (tx, requests) = std::sync::mpsc::channel();
        let stored = std::rc::Rc::new(std::cell::RefCell::new(None));
        let saved = stored.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PagedAuth(tx))));
            *saved.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = stored.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        let (_, reply) = requests
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        let m = |ix: i32| crate::conversation::Message {
            id: ix.to_string(),
            channel_id: "000000000000001".into(),
            author_id: "42".into(),
            author_name: "Ada".into(),
            text: if ix % 2 == 0 {
                "multi-line\nwith wrapped content that varies height"
            } else {
                "short"
            }
            .into(),
            created_at: "same".into(),
        };
        reply
            .send_blocking(Ok(crate::conversation::Page {
                items: (1..=40).rev().map(m).collect(),
                next_cursor: Some("older".into()),
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            for _ in 0..4 {
                window.scroll(
                    "history",
                    gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(90.))),
                    cx,
                );
                window.render_frame(cx);
            }
            assert_eq!(view.read(cx).history_list.is_scrolled_to_end(), Some(false));
            window.click("refresh-history", cx);
            window.render_frame(cx);
            assert_eq!(
                window.find("refresh-history").label(),
                Some("Refreshing conversation…")
            );
            window.click("refresh-history", cx);
        });
        cx.run_until_parked();
        let (before, first) = requests
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        assert_eq!(before, None);
        assert!(requests.try_recv().is_err());
        first
            .send_blocking(Ok(crate::conversation::Page {
                items: (61..=70).rev().map(m).collect(),
                next_cursor: Some("server opaque".into()),
            }))
            .unwrap();
        cx.run_until_parked();
        let (before, second) = requests
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        assert_eq!(before.as_deref(), Some("server opaque"));
        let anchor = std::rc::Rc::new(std::cell::RefCell::new(None));
        let saved_anchor = anchor.clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                view.read(cx)
                    .conversation
                    .refreshing
                    .contains_key("000000000000001")
            );
            let ix = view.read(cx).history_list.logical_scroll_top().item_ix;
            let id = format!("message-{}", ix + 1);
            *saved_anchor.borrow_mut() = Some((id.clone(), window.find(id).bounds().origin.y));
        });
        second
            .send_blocking(Ok(crate::conversation::Page {
                items: (39..=60).rev().map(m).collect(),
                next_cursor: Some("unneeded".into()),
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            let (id, y) = anchor.borrow().clone().unwrap();
            assert_eq!(window.find(id).bounds().origin.y, y);
            assert_eq!(view.read(cx).history_list.is_scrolled_to_end(), Some(false));
            window.click("jump-latest", cx);
            window.render_frame(cx);
            assert_eq!(view.read(cx).history_list.is_scrolled_to_end(), Some(true));
            assert!(window.find("message-70").bounds().size.height > px(0.));
            window.click("refresh-history", cx);
        });
        cx.run_until_parked();
        let (_, latest) = requests
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        latest
            .send_blocking(Ok(crate::conversation::Page {
                items: [71, 70, 69].map(m).to_vec(),
                next_cursor: Some("still older".into()),
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(view.read(cx).history_list.is_scrolled_to_end(), Some(true));
            assert!(window.find("message-71").bounds().size.height > px(0.));
        });
    }

    #[gpui_kit::test]
    fn production_message_is_selectable_and_copyable(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
            Root::new(view, window, cx)
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            let bounds = window.find("message-text-000000000000001").bounds();
            window.drag(
                bounds.origin + gpui_kit::point(px(2.), px(2.)),
                bounds.origin
                    + gpui_kit::point(bounds.size.width - px(2.), bounds.size.height - px(2.)),
                cx,
            );
            assert_eq!(
                gpui_kit::base::TextSelection::selected_text(window, cx),
                "first line\nsecond line"
            );
            window.press("ctrl-c", cx);
        });
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("first line\nsecond line")
        );
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
