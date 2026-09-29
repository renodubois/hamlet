//! Temporary monolithic application shell, mechanically relocated for #48.
//!
//! Feature workflows and private view state remain here until their ownership tickets.
//! See ../../MIGRATION-48.md for the compatibility inventory and removal gates.

use crate::conversation::polling::{Polling, Resource};
use crate::conversation::{self, Conversation, Load, Older, ReadRequest};
use crate::persistence::{self, Outcome, Persistence, Selection};
use crate::runtime::{Execution, Work};
use crate::session::{
    self, AppSession, AuthApi, DEFAULT_SERVER_URL, RestoreDecision, RestoreResult,
};
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
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

// Workflow policy stays with this temporary coordinator, not storage or runtime.
const SECURE_STORE_DEADLINE: Duration = Duration::from_secs(10);
const READ_SEND_DEADLINE: Duration = Duration::from_secs(9);

/// Create the legacy root and start restoration and polling with supplied execution.
pub(crate) fn open(
    window: &mut Window,
    cx: &mut App,
    api: Arc<dyn AuthApi>,
    config: persistence::Config,
    persistence: Option<Persistence>,
    execution: Execution,
) -> Entity<Hamlet> {
    let view =
        cx.new(|cx| Hamlet::with_dependencies(window, cx, api, config, persistence, execution));
    view.update(cx, |view, cx| {
        view.resume_deletions(cx);
        view.start_restore(cx);
    });
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

struct Deletion {
    selection: Selection,
    id: u64,
    in_flight: bool,
}

pub(crate) struct Hamlet {
    session: AppSession,
    conversation: Conversation,
    history_focus: FocusHandle,
    _window_activation: Subscription,
    polling: Polling,
    poll_clock: Instant,
    history_list: ListState,
    history_task: Option<Work>,
    execution: Execution,
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
    persistence: Option<Persistence>,
    credential: Option<Selection>,
    retained_credentials: Vec<Selection>,
    storage_feedback: Option<String>,
    storage_serial: u64,
    deletions: Vec<Deletion>,
    deletion_id: u64,
}

impl Hamlet {
    fn with_dependencies(
        window: &mut Window,
        cx: &mut Context<Self>,
        api: Arc<dyn AuthApi>,
        config: persistence::Config,
        persistence: Option<Persistence>,
        execution: Execution,
    ) -> Self {
        let initial_server = config.server.as_deref().unwrap_or(DEFAULT_SERVER_URL);
        let server = cx.new(|cx| InputState::new(window, cx).default_value(initial_server));
        let username = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config
                    .saved
                    .as_ref()
                    .filter(|s| s.server == initial_server)
                    .map(|s| s.user.username.as_str())
                    .unwrap_or(""),
            )
        });
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
                    view.storage_feedback = None;
                    view.invalidate_storage(cx);
                    view.cancel_history();
                    view.conversation.clear();
                    view.polling = Polling::default();
                    view.history_list.reset(0);
                    view.clear_password = true;
                    cx.notify();
                }
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
        let mut session = AppSession::new(api);
        session.change_server(initial_server.into());
        Self {
            session,
            conversation: Conversation::default(),
            history_focus: cx.focus_handle(),
            _window_activation: window_activation,
            polling: Polling::default(),
            poll_clock: execution.now(),
            execution,
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
            persistence,
            credential: config.saved.filter(|s| s.server == initial_server),
            retained_credentials: Vec::new(),
            storage_feedback: if config.pending_deletions.is_empty() {
                None
            } else {
                Some("Previous saved login cleanup is pending; retry deletion if it fails.".into())
            },
            storage_serial: 0,
            deletions: config
                .pending_deletions
                .into_iter()
                .enumerate()
                .map(|(i, selection)| Deletion {
                    selection,
                    id: i as u64 + 1,
                    in_flight: false,
                })
                .collect(),
            deletion_id: 0,
        }
    }

    fn resume_deletions(&mut self, cx: &mut Context<Self>) {
        self.deletion_id = self.deletions.len() as u64;
        let ids: Vec<_> = self.deletions.iter().map(|d| d.id).collect();
        for id in ids {
            self.run_deletion(id, cx);
        }
    }

    // Deletions have their own identities: a server edit or a newer save must not
    // discard an older failed cleanup callback or its retry target.
    fn queue_deletion(&mut self, selection: Selection, cx: &mut Context<Self>) {
        let Some(store) = &self.persistence else {
            return;
        };
        if self
            .deletions
            .iter()
            .any(|d| persistence::account(&d.selection) == persistence::account(&selection))
        {
            return;
        }
        self.deletion_id = self.deletion_id.wrapping_add(1);
        let id = self.deletion_id;
        self.deletions.push(Deletion {
            selection: selection.clone(),
            id,
            in_flight: false,
        });
        let _ = store;
        self.run_deletion(id, cx);
    }

    fn retryable_deletion(&self) -> Option<u64> {
        let active = self.session.active.as_ref().map(|s| {
            persistence::account(&Selection {
                server: s.server.clone(),
                user: s.user.clone(),
                expires_at: s.expires_at,
            })
        });
        self.deletions
            .iter()
            .find(|d| {
                !d.in_flight
                    && active.as_deref() != Some(persistence::account(&d.selection).as_str())
            })
            .map(|d| d.id)
    }

    fn run_deletion(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(deletion) = self
            .deletions
            .iter_mut()
            .find(|d| d.id == id && !d.in_flight)
        else {
            return;
        };
        // A newer active login for this account must never be removed by a retry.
        if self.session.active.as_ref().is_some_and(|s| {
            persistence::account(&Selection {
                server: s.server.clone(),
                user: s.user.clone(),
                expires_at: s.expires_at,
            }) == persistence::account(&deletion.selection)
        }) {
            return;
        }
        let Some(store) = &self.persistence else {
            return;
        };
        deletion.in_flight = true;
        let queued = store.delete(deletion.selection.clone());
        let reply = self
            .execution
            .bounded(SECURE_STORE_DEADLINE, async move { queued.recv().await });
        self.storage_feedback = Some("Removing saved login from Secret Service…".into());
        cx.spawn(async move |weak, cx| {
            let outcome = reply.recv().await;
            let _ = weak.update(cx, |view, cx| {
                let Some(index) = view.deletions.iter().position(|d| d.id == id) else { return; };
                if matches!(outcome, Ok(Some(Ok(Outcome::Deleted)))) {
                    view.deletions.remove(index);
                    if view.deletions.is_empty() {
                        view.storage_feedback = Some(if view.session.active.is_some() {
                            "Previous saved login removed from Secret Service; current session is unchanged.".into()
                        } else {
                            "Saved login removed from Secret Service.".into()
                        });
                    }
                } else {
                    view.deletions[index].in_flight = false;
                    view.storage_feedback = Some("Could not confirm deletion from Secret Service; a saved login may remain. Unlock the store and retry deletion.".into());
                }
                cx.notify();
            });
        }).detach();
    }

    fn invalidate_storage(&mut self, cx: &mut Context<Self>) {
        self.storage_serial = self.storage_serial.wrapping_add(1);
        if let Some(store) = &self.persistence {
            store.invalidate();
        }
        if let Some(selection) = self.credential.take() {
            self.queue_deletion(selection, cx);
        }
        for selection in std::mem::take(&mut self.retained_credentials) {
            self.queue_deletion(selection, cx);
        }
    }

    fn save_login(&mut self, cx: &mut Context<Self>) {
        let Some(store) = &self.persistence else {
            return;
        };
        let Some(session) = &self.session.active else {
            return;
        };
        let selection = Selection {
            server: session.server.clone(),
            user: session.user.clone(),
            expires_at: session.expires_at,
        };
        let previous = self.credential.replace(selection.clone());
        if let Some(old) = previous.as_ref()
            && persistence::account(old) != persistence::account(&selection)
            && !self
                .retained_credentials
                .iter()
                .any(|s| persistence::account(s) == persistence::account(old))
        {
            self.retained_credentials.push(old.clone());
        }
        // Remember successful authentication independently of secure-store success.
        let remembered = store.remember(selection.server.clone());
        let queued = session.save(store);
        let reply = self.execution.bounded(SECURE_STORE_DEADLINE, async move {
            (remembered.recv().await, queued.recv().await)
        });
        self.storage_serial = self.storage_serial.wrapping_add(1);
        let serial = self.storage_serial;
        self.storage_feedback = Some("Saving login to Secret Service…".into());
        cx.spawn(async move |weak, cx| {
            let outcome = reply.recv().await;
            let _ = weak.update(cx, |view, cx| {
                if view.storage_serial == serial && view.session.active.is_some() {
                    let (remembered, outcome) = match outcome {
                        Ok(Some((remembered, outcome))) => (remembered, outcome),
                        _ => (Err(async_channel::RecvError), Err(async_channel::RecvError)),
                    };
                    if matches!(outcome, Ok(Outcome::Saved | Outcome::SavedWithCleanupWarning)) {
                        view.deletions.retain(|d| persistence::account(&d.selection) != persistence::account(&selection));
                        if let Some(old) = previous {
                            view.retained_credentials.retain(|s| persistence::account(s) != persistence::account(&old));
                            if matches!(outcome, Ok(Outcome::SavedWithCleanupWarning)) {
                                view.queue_deletion(old, cx);
                            }
                        }
                    }
                    view.storage_feedback = Some(match outcome {
                        Ok(Outcome::Saved) => "Login saved in Secret Service for this server and user.".into(),
                        Ok(Outcome::SavedWithCleanupWarning) => "Login saved, but a previous server/user credential could not be removed from Secret Service.".into(),
                        Ok(Outcome::Failed) if remembered == Ok(Outcome::Remembered) => "Secure storage failed; this session is memory-only. A previous saved login may remain until deletion is confirmed.".into(),
                        Ok(Outcome::Failed) => "Secure storage and server preference could not be saved; this session is memory-only.".into(),
                        _ => "Secure storage timed out or was interrupted; saving is unconfirmed. Continue in memory, but a credential may still appear in Secret Service. Log out to request deletion.".into(),
                    });
                    cx.notify();
                }
            });
        }).detach();
    }

    fn start_restore(&mut self, cx: &mut Context<Self>) {
        let Some(store) = &self.persistence else {
            return;
        };
        let Some(selection) = self.credential.clone() else {
            return;
        };
        if self.session.active.is_some()
            || self.session.pending
            || selection.server != self.session.server
        {
            return;
        }
        let generation = self.session.begin_restore();
        if selection.expires_at <= self.execution.unix_seconds() {
            if self.session.finish_restore(
                generation,
                &selection.server,
                &selection.user,
                selection.expires_at,
                RestoreResult::Unavailable,
                self.execution.unix_seconds(),
            ) == RestoreDecision::Delete
            {
                self.invalidate_storage(cx);
            }
            cx.notify();
            return;
        }
        let reply = store.read(selection.clone());
        let server = self.session.api.clone().server(&selection.server);
        // One budget covers the worker reply AND verification, not a fresh budget per step.
        let result = self.execution.bounded(SECURE_STORE_DEADLINE, async move {
            match reply.recv().await {
                Ok(Outcome::Token(Some(token))) => match server {
                    Ok(server) => AppSession::verify_saved(server, token).await,
                    Err(_) => RestoreResult::Unavailable,
                },
                Ok(Outcome::Token(None)) => RestoreResult::MissingCredential,
                _ => RestoreResult::Unavailable,
            }
        });
        cx.spawn(async move |weak, cx| {
            let result = result.recv().await.ok().flatten().unwrap_or(RestoreResult::Unavailable);
            let _ = weak.update_in(cx, |view, window, cx| {
                match view.session.finish_restore(generation, &selection.server, &selection.user, selection.expires_at, result, view.execution.unix_seconds()) {
                    RestoreDecision::Restored => {
                        view.storage_feedback = Some("Login restored from Secret Service.".into());
                        view.enter_authenticated(window, cx);
                    }
                    RestoreDecision::Delete => view.invalidate_storage(cx),
                    RestoreDecision::Retry => view.storage_feedback = Some("Could not verify saved login; use a memory-only login or retry restoration.".into()),
                    RestoreDecision::Stale => return,
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
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
        // Timeout never proves non-delivery and never replays a write.
        let receive = self.execution.bounded(READ_SEND_DEADLINE, async move {
            api.send_message(work.server, work.token, work.channel_id, work.text)
                .await
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
        let outcome = self.conversation.complete_send(
            &mut self.session,
            &request,
            result,
            self.execution.unix_seconds(),
        );
        if self.session.active.is_none() {
            self.polling = Polling::default();
            self.invalidate_storage(cx);
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
        let server = self.session.api.clone().server(&request.server);
        let username = request.username.clone();
        let password = request.password.clone();
        let receive = self.execution.spawn(async move {
            let server = server?;
            if signup {
                server.signup(username, password).await
            } else {
                server.login(username, password).await
            }
        });
        cx.spawn(async move |weak, cx| {
            if let Ok(result) = receive.recv().await {
                let _ = weak.update_in(cx, |view, window, cx| {
                    let accepted = if signup {
                        view.session
                            .complete_signup(request, result, view.execution.unix_seconds())
                    } else {
                        view.session
                            .complete_login(request, result, view.execution.unix_seconds())
                    };
                    if accepted && view.session.active.is_some() {
                        view.storage_serial = view.storage_serial.wrapping_add(1);
                        view.save_login(cx);
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
            let delay = self.execution.sleep(Duration::from_secs(
                expires_at
                    .saturating_sub(self.execution.unix_seconds())
                    .max(0) as u64,
            ));
            cx.spawn(async move |weak, cx| {
                delay.await;
                let _ = weak.update(cx, |view, cx| {
                    if view.session.session_generation() == Some(generation) {
                        view.session.expire(view.execution.unix_seconds());
                        if view.session.active.is_none() {
                            view.invalidate_storage(cx);
                            view.cancel_history();
                            view.conversation.clear();
                            view.polling = Polling::default();
                            view.history_list.reset(0);
                            // The next login restores an empty composer.
                        }
                        cx.notify();
                    }
                });
            })
            .detach();
            self.password
                .update(cx, |input, cx| input.set_value("", window, cx));
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
        if self.session.active.is_none() {
            return;
        }
        let mut dispatched = false;
        if self.polling.due(Resource::Channels, time)
            && let Some(request) = self.conversation.refresh_channels(&self.session)
        {
            self.polling.started(Resource::Channels);
            self.dispatch_channels(request, cx);
            dispatched = true;
        }
        if self.polling.due(Resource::History, time)
            && let Some(id) = self.conversation.selected.as_deref()
            && !matches!(self.conversation.older.get(id), Some(Older::Loading))
            && let Some(request) = self.conversation.refresh_history(&self.session)
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
        if let Some(request) = self.conversation.start(&self.session) {
            self.polling.started(Resource::Channels);
            self.dispatch_channels(request, cx);
        }
    }

    fn refresh_channels(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.conversation.refresh_channels(&self.session) {
            self.polling.started(Resource::Channels);
            self.dispatch_channels(request, cx);
            cx.notify();
        }
    }

    fn dispatch_channels(&mut self, request: ReadRequest, cx: &mut Context<Self>) {
        let api = self.session.api.clone();
        let work = request.clone();
        let receive = self.execution.bounded(READ_SEND_DEADLINE, async move {
            api.channels(work.server, work.token).await
        });
        cx.spawn(async move |weak, cx| {
            if let Ok(result) = receive.recv().await {
                let result = result.unwrap_or(Err(session::AuthError::Unavailable));
                let _ = weak.update_in(cx, |view, window, cx| {
                    let current = view.conversation.is_current_channels(&request)
                        && view.session.session_generation() == Some(request.generation)
                        && view
                            .session
                            .active
                            .as_ref()
                            .is_some_and(|active| active.server == request.server);
                    let succeeded = result.is_ok();
                    let previous = view.conversation.selected.clone();
                    view.store_composer(cx);
                    let next =
                        view.conversation
                            .complete_channels(&mut view.session, &request, result);
                    if view.session.active.is_none() {
                        view.polling = Polling::default();
                        view.invalidate_storage(cx);
                    } else if current {
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
        if let Some(request) = self.conversation.request_older(&self.session) {
            self.load_history(request, cx);
            cx.notify();
        }
    }

    fn refresh_history(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.conversation.refresh_history(&self.session) {
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
        let (task, receive) = self
            .execution
            .start_bounded(READ_SEND_DEADLINE, async move {
                api.history_page(
                    work.server,
                    work.token,
                    work.channel_id.unwrap(),
                    work.before,
                )
                .await
            });
        self.history_task = Some(task);
        cx.spawn(async move |weak, cx| {
            if let Ok(result) = receive.recv().await {
                let result = result.unwrap_or(Err(session::AuthError::Unavailable));
                let _ = weak.update(cx, |view, cx| {
                    view.finish_history(serial, request, result, cx)
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
        let current = selected && self.session.session_generation() == Some(request.generation);
        let is_older = request.before.is_some() && !request.is_catchup();
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
            self.invalidate_storage(cx);
            self.history_list.reset(0);
        }
        self.history_task = None;
        if self.session.active.is_none() {
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
        let work = request.clone();
        let receive = self
            .execution
            .spawn(async move { api.create_channel(work.server, work.token, work.name).await });
        cx.spawn(async move |weak, cx| {
            if let Ok(result) = receive.recv().await {
                let _ = weak.update_in(cx, |view, window, cx| {
                    view.store_composer(cx);
                    let (confirmed, history) = view.conversation.complete_create(
                        &mut view.session,
                        &request,
                        result,
                        view.execution.unix_seconds(),
                    );
                    if view.session.active.is_none() {
                        view.invalidate_storage(cx);
                    }
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
        self.polling = Polling::default();
        self.sync_composer(window, cx);
        let revocation = self.session.logout();
        self.invalidate_storage(cx);
        if let Some(revocation) = revocation {
            let receive = self.execution.spawn(async move {
                let result = revocation.client.logout().await;
                (revocation.generation, result)
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
                .child(div().child(if self.session.restore_pending() {
                    "Checking saved login before opening conversations…"
                } else {
                    "Log in to start a session. Secure storage status appears below."
                }));
        }
        if self.persistence.is_some()
            && !self.session.pending
            && (self.retryable_deletion().is_some()
                || (self.session.active.is_none() && self.credential.is_some()))
        {
            let retry = view.clone();
            surface = surface.child(
                Button::new("retry-storage")
                    .label(if self.retryable_deletion().is_some() {
                        "Retry saved-login deletion"
                    } else {
                        "Retry saved-login restoration"
                    })
                    .on_click(move |_, _, cx| {
                        let _ = retry.update(cx, |view, cx| {
                            if let Some(id) = view.retryable_deletion() {
                                view.run_deletion(id, cx);
                            } else {
                                view.start_restore(cx);
                            }
                        });
                    }),
            );
        }
        if let Some(feedback) = &self.storage_feedback {
            surface = surface.child(
                div()
                    .id("storage-status")
                    .aria_label(feedback.clone())
                    .test_support()
                    .child(feedback.clone()),
            );
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
