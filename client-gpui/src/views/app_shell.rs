//! Temporary monolithic application shell, mechanically relocated for #48.
//!
//! Session workflows are owned by the application-lifetime coordinator (#54).
//! Saved-login workflows belong to session (#55), and LoginView owns the form (#56).
//! Conversation workflows and authenticated controls await their own tickets.

use super::login::LoginView;

use crate::api::HttpTransport;
use crate::conversation::polling::{Polling, Resource};
use crate::conversation::{self, Conversation, Load, Older, ReadRequest};
use crate::runtime::{Execution, Work};
use crate::session::{self, Lifecycle, SessionCoordinator, StorageRetry};
use crate::storage::{Config, Persistence};
use crate::theme;
use gpui_kit::base::SelectableText;
use gpui_kit::prelude::FluentBuilder as _;

use gpui_kit::base::Disableable;
use gpui_kit::base::input::{
    InputBaseState, InputEvent, InputMode, InputState, TextareaMode, TextareaState,
};
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::*;
use std::time::{Duration, Instant};

// Conversation workflow policy stays here until its coordinator extraction.
const READ_SEND_DEADLINE: Duration = Duration::from_secs(9);

/// Create the legacy root and start restoration and polling with supplied execution.
pub(crate) fn open(
    window: &mut Window,
    cx: &mut App,
    api: HttpTransport,
    config: Config,
    persistence: Option<Persistence>,
    execution: Execution,
) -> Entity<Hamlet> {
    let view =
        cx.new(|cx| Hamlet::with_dependencies(window, cx, api, config, persistence, execution));
    view.update(cx, |view, cx| view.start_poll_timer(cx));
    view
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

pub(crate) struct Hamlet {
    session: Entity<SessionCoordinator>,
    login: Entity<LoginView>,
    _session_subscription: Subscription,
    conversation: Conversation,
    history_focus: FocusHandle,
    _window_activation: Subscription,
    polling: Polling,
    poll_clock: Instant,
    history_list: ListState,
    history_task: Option<Work>,
    execution: Execution,
    history_serial: u64,
    channel_name: Entity<InputBaseState<InputMode>>,
    composer: Entity<InputBaseState<TextareaMode>>,
    _composer_subscription: Subscription,
    syncing_composer: bool,
}

impl Hamlet {
    fn with_dependencies(
        window: &mut Window,
        cx: &mut Context<Self>,
        api: HttpTransport,
        config: Config,
        persistence: Option<Persistence>,
        execution: Execution,
    ) -> Self {
        let session =
            cx.new(|_| SessionCoordinator::new(api, execution.clone(), config, persistence));
        let login = cx.new(|cx| LoginView::new(session.clone(), window, cx));
        let session_subscription =
            cx.observe_in(&session, window, |view: &mut Self, _, window, cx| {
                view.session_changed(window, cx);
                cx.notify();
            });
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
        // Native platform activation, independent of which input has keyboard focus.
        let window_activation = cx.observe_window_activation(window, |view, window, cx| {
            view.set_window_focused(window.is_window_active(), cx);
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
        let updates = session.read(cx).updates();
        cx.spawn(async move |weak, cx| {
            while let Ok(update) = updates.recv().await {
                if weak
                    .update_in(cx, |view, window, cx| {
                        view.session.update(cx, |session, cx| {
                            session.apply(update);
                            cx.notify();
                        });
                        view.session_changed(window, cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            session,
            login,
            _session_subscription: session_subscription,
            conversation: Conversation::default(),
            history_focus: cx.focus_handle(),
            _window_activation: window_activation,
            polling: Polling::default(),
            poll_clock: execution.now(),
            execution,
            history_list,
            history_task: None,
            history_serial: 0,
            channel_name,
            composer,
            _composer_subscription: composer_subscription,
            syncing_composer: false,
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
        let Some(request) = self.conversation.send(self.session.read(cx)) else {
            cx.notify();
            return;
        };
        let Some(api) = self.session.read(cx).client_for(request.generation) else {
            return;
        };
        let work = request.clone();
        // Timeout never proves non-delivery and never replays a write.
        let receive = self.execution.bounded(READ_SEND_DEADLINE, async move {
            api.send_message(work.channel_id, work.text).await
        });
        cx.spawn(async move |weak, cx| {
            if let Ok(result) = receive.recv().await {
                let result = result.unwrap_or(Err(session::AuthError::Unavailable));
                let _ = weak.update_in(cx, |view, window, cx| {
                    view.finish_send(request, result, window, cx)
                });
            }
        })
        .detach();
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
        let outcome = self.session.update(cx, |session, cx| {
            let outcome = self.conversation.complete_send(
                session,
                &request,
                result,
                self.execution.unix_seconds(),
            );
            cx.notify();
            outcome
        });
        self.session_changed(window, cx);
        if self.session.read(cx).active().is_some()
            && self.conversation.selected.as_deref() == Some(&id)
        {
            if outcome == conversation::SendOutcome::Confirmed {
                self.sync_composer(window, cx);
                if let Some(next) = self.conversation.reconcile_confirmed(self.session.read(cx)) {
                    self.cancel_history();
                    self.load_history(next, cx);
                }
            } else if outcome == conversation::SendOutcome::Uncertain
                && let Some(request) = self.conversation.reconcile_uncertain(self.session.read(cx))
            {
                self.cancel_history();
                self.load_history(request, cx);
            }
        }
        cx.notify();
    }

    fn enter_authenticated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.read(cx).active().is_some() {
            self.sync_composer(window, cx);
            self.polling = Polling::default();
            self.polling
                .focus(window.is_window_active(), self.poll_time());
            self.load_channels(cx);
        }
    }

    fn start_poll_timer(&mut self, cx: &mut Context<Self>) {
        let execution = self.execution.clone();
        cx.spawn(async move |weak, cx| {
            loop {
                execution.sleep(Duration::from_secs(1)).await;
                if weak
                    .update(cx, |view, cx| view.poll_at(view.poll_time(), cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_time(&self) -> Duration {
        self.execution.now().duration_since(self.poll_clock)
    }

    fn set_window_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        if self.polling.focus(focused, self.poll_time()) {
            self.poll_at(self.poll_time(), cx);
            cx.notify();
        }
    }

    fn poll_at(&mut self, time: Duration, cx: &mut Context<Self>) {
        if self.session.read(cx).active().is_none() {
            return;
        }
        let mut dispatched = false;
        if self.polling.due(Resource::Channels, time)
            && let Some(request) = self.conversation.refresh_channels(self.session.read(cx))
        {
            self.polling.started(Resource::Channels);
            self.dispatch_channels(request, cx);
            dispatched = true;
        }
        if self.polling.due(Resource::History, time)
            && let Some(id) = self.conversation.selected.as_deref()
            && !matches!(self.conversation.older.get(id), Some(Older::Loading))
            && let Some(request) = self.conversation.refresh_history(self.session.read(cx))
        {
            self.polling.started(Resource::History);
            self.load_history(request, cx);
            dispatched = true;
        }
        if dispatched {
            cx.notify();
        }
    }

    fn load_channels(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.conversation.start(self.session.read(cx)) {
            self.polling.started(Resource::Channels);
            self.dispatch_channels(request, cx);
        }
    }

    fn refresh_channels(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.conversation.refresh_channels(self.session.read(cx)) {
            self.polling.started(Resource::Channels);
            self.dispatch_channels(request, cx);
            cx.notify();
        }
    }

    fn dispatch_channels(&mut self, request: ReadRequest, cx: &mut Context<Self>) {
        let Some(api) = self.session.read(cx).client_for(request.generation) else {
            return;
        };
        let receive = self
            .execution
            .bounded(READ_SEND_DEADLINE, async move { api.channels().await });
        cx.spawn(async move |weak, cx| {
            if let Ok(result) = receive.recv().await {
                let result = result.unwrap_or(Err(session::AuthError::Unavailable));
                let _ = weak.update_in(cx, |view, window, cx| {
                    let current = view.conversation.is_current_channels(&request)
                        && view.session.read(cx).session_generation() == Some(request.generation);
                    let succeeded = result.is_ok();
                    let previous = view.conversation.selected.clone();
                    view.store_composer(cx);
                    let next = view.session.update(cx, |session, cx| {
                        let next = view
                            .conversation
                            .complete_channels(session, &request, result);
                        cx.notify();
                        next
                    });
                    view.session_changed(window, cx);
                    if view.session.read(cx).active().is_some() && current {
                        let recovered =
                            view.polling
                                .completed(Resource::Channels, succeeded, view.poll_time());
                        if recovered {
                            view.polling
                                .recover_other(Resource::Channels, view.poll_time());
                        }
                    }
                    if previous != view.conversation.selected {
                        view.polling.reset_history(view.poll_time());
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
        if let Some(request) = self.conversation.request_older(self.session.read(cx)) {
            self.load_history(request, cx);
            cx.notify();
        }
    }

    fn refresh_history(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.conversation.refresh_history(self.session.read(cx)) {
            self.polling.started(Resource::History);
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
        if let Some(request) = self.conversation.retry_older(self.session.read(cx)) {
            self.load_history(request, cx);
            cx.notify();
        }
    }

    fn load_history(&mut self, request: ReadRequest, cx: &mut Context<Self>) {
        self.history_serial = self.history_serial.wrapping_add(1);
        let serial = self.history_serial;
        let Some(api) = self.session.read(cx).client_for(request.generation) else {
            return;
        };
        let work = request.clone();
        let (task, receive) = self
            .execution
            .start_bounded(READ_SEND_DEADLINE, async move {
                api.history_page(work.channel_id.unwrap(), work.before)
                    .await
            });
        self.history_task = Some(task);
        cx.spawn(async move |weak, cx| {
            if let Ok(result) = receive.recv().await {
                let result = result.unwrap_or(Err(session::AuthError::Unavailable));
                let _ = weak.update_in(cx, |view, window, cx| {
                    view.finish_history(serial, request, result, window, cx)
                });
            }
        })
        .detach();
    }

    fn finish_history(
        &mut self,
        serial: u64,
        request: ReadRequest,
        result: Result<conversation::Page, session::AuthError>,
        window: &mut Window,
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
        let current =
            selected && self.session.read(cx).session_generation() == Some(request.generation);
        let is_older = request.before.is_some() && !request.is_catchup();
        let outcome = self.session.update(cx, |session, cx| {
            let outcome = self
                .conversation
                .complete_history(session, &request, result);
            cx.notify();
            outcome
        });
        self.session_changed(window, cx);
        if selected && self.session.read(cx).active().is_some() {
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
        }
        self.history_task = None;
        if self.session.read(cx).active().is_none() {
            self.polling = Polling::default();
        } else if current && !is_older && outcome.next.is_none() {
            let complete = read_succeeded
                && !matches!(
                    request
                        .channel_id
                        .as_ref()
                        .and_then(|id| self.conversation.refreshing.get(id)),
                    Some(conversation::Refresh::Incomplete(_))
                );
            let recovered = self
                .polling
                .completed(Resource::History, complete, self.poll_time());
            if recovered {
                self.polling
                    .recover_other(Resource::History, self.poll_time());
            }
        }
        if let Some(next) = outcome.next {
            self.load_history(next, cx);
        }
        // A failed or disconnected catch-up needs a *manual* retry; never spin on
        // repeated read failures while a confirmed/uncertain write waits for continuity.
        if let Some(next) = self
            .conversation
            .after_history_read(self.session.read(cx), read_succeeded)
        {
            self.cancel_history();
            self.load_history(next, cx);
        }
        cx.notify();
    }

    fn create_channel(&mut self, cx: &mut Context<Self>) {
        let name = self.channel_name.read(cx).text().to_string();
        let Some(request) = self.conversation.create(self.session.read(cx), &name) else {
            cx.notify();
            return;
        };
        let Some(api) = self.session.read(cx).client_for(request.generation) else {
            return;
        };
        let work = request.clone();
        let receive = self
            .execution
            .spawn(async move { api.create_channel(work.name).await });
        cx.spawn(async move |weak, cx| {
            if let Ok(result) = receive.recv().await {
                let _ = weak.update_in(cx, |view, window, cx| {
                    view.store_composer(cx);
                    let (confirmed, history) = view.session.update(cx, |session, cx| {
                        let outcome = view.conversation.complete_create(
                            session,
                            &request,
                            result,
                            view.execution.unix_seconds(),
                        );
                        cx.notify();
                        outcome
                    });
                    view.session_changed(window, cx);
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
            self.polling.reset_history(self.poll_time());
            self.cancel_history();
            self.history_list.reset(0);
        }
        if let Some(request) = self.conversation.select(self.session.read(cx), id) {
            self.sync_composer(window, cx);
            self.load_history(request, cx);
        } else {
            self.sync_composer(window, cx);
            self.restore_cached_history();
        }
        cx.notify();
    }

    fn logout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.logout();
            cx.notify();
        });
        self.session_changed(window, cx);
        cx.notify();
    }

    /// Synchronous local activity cleanup before screen removal; session owns storage work.
    fn session_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lifecycle = self
            .session
            .update(cx, |session, _| session.take_lifecycle());
        if lifecycle.is_some() {
            self.login
                .update(cx, |login, cx| login.clear_sensitive(window, cx));
        }
        match lifecycle {
            Some(Lifecycle::Authenticated) => {
                self.enter_authenticated(window, cx);
            }
            Some(Lifecycle::Invalidated | Lifecycle::ServerChanged) => {
                self.cancel_history();
                self.conversation.clear();
                self.polling = Polling::default();
                self.history_list.reset(0);
                self.sync_composer(window, cx);
            }
            None => {}
        }
    }
}

impl Render for Hamlet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let mut surface = div()
            .size_full()
            .bg(rgb(theme::BACKGROUND))
            .flex()
            .flex_col()
            .gap_3()
            .text_color(rgb(theme::TEXT));
        if let Some(session) = self.session.read(cx).active() {
            let logout_view = view.clone();
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
                            let _ = logout_view.update(cx, |view, cx| view.logout(window, cx));
                        }),
                )
                .child(
                    div()
                        .id("connection-status")
                        .aria_label(self.polling.status())
                        .test_support()
                        .child(self.polling.status()),
                )
                .child(self.conversation_panes(cx));
        } else {
            surface = surface
                .justify_center()
                .items_center()
                .child(self.login.clone());
        }
        if let Some(action) = self.session.read(cx).storage_retry() {
            let retry = view.clone();
            surface = surface.child(
                Button::new("retry-storage")
                    .label(match action {
                        StorageRetry::Deletion => "Retry saved-login deletion",
                        StorageRetry::Restoration => "Retry saved-login restoration",
                    })
                    .on_click(move |_, _, cx| {
                        let _ = retry.update(cx, |view, cx| {
                            view.session.update(cx, |session, cx| {
                                session.retry_storage();
                                cx.notify();
                            });
                            cx.notify();
                        });
                    }),
            );
        }
        if let Some(feedback) = self.session.read(cx).storage_feedback() {
            surface = surface.child(
                div()
                    .id("storage-status")
                    .aria_label(feedback.to_owned())
                    .test_support()
                    .child(feedback.to_owned()),
            );
        }
        if self.session.read(cx).active().is_some()
            && let Some(feedback) = self.session.read(cx).feedback()
        {
            surface = surface.child(
                div()
                    .id("auth-feedback")
                    .aria_label(feedback.to_owned())
                    .test_support()
                    .child(feedback.to_owned()),
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
                            .icon(theme::send_icon())
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

// Descendant test modules retain legacy private access without exporting view internals.
#[cfg(test)]
#[path = "tests/legacy_fixture.rs"]
mod tests;
