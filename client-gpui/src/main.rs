mod conversation;
mod http;
mod persistence;
mod polling;
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
use persistence::{Outcome, Persistence, Selection};
use polling::{Polling, Resource};
use session::{AppSession, AuthApi, DEFAULT_SERVER_URL, RestoreDecision, RestoreResult};
use std::{
    sync::{Arc, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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

fn bounded<T: Send + 'static>(
    future: impl std::future::Future<Output = T> + Send + 'static,
) -> async_channel::Receiver<Option<T>> {
    let (send, receive) = async_channel::bounded(1);
    runtime().spawn(async move {
        let result = tokio::time::timeout(persistence::DEADLINE, future)
            .await
            .ok();
        let _ = send.send(result).await;
    });
    receive
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("network runtime"))
}

struct Deletion {
    selection: Selection,
    id: u64,
    in_flight: bool,
}

struct Hamlet {
    session: AppSession,
    conversation: Conversation,
    history_focus: FocusHandle,
    _window_activation: Subscription,
    polling: Polling,
    poll_clock: Instant,
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
    persistence: Option<Persistence>,
    credential: Option<Selection>,
    retained_credentials: Vec<Selection>,
    storage_feedback: Option<String>,
    storage_serial: u64,
    deletions: Vec<Deletion>,
    deletion_id: u64,
}

impl Hamlet {
    fn new(window: &mut Window, cx: &mut Context<Self>, api: Arc<dyn AuthApi>) -> Self {
        #[cfg(not(test))]
        let config = persistence::load();
        #[cfg(test)]
        let config = persistence::Config::default();
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
            poll_clock: Instant::now(),
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
            #[cfg(not(test))]
            persistence: Some(Persistence::new()),
            #[cfg(test)]
            persistence: None,
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
        let reply = bounded(async move { queued.recv().await });
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
        let queued = store.save(selection.clone(), session.token().into());
        let reply = bounded(async move { (remembered.recv().await, queued.recv().await) });
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
        if selection.expires_at <= now() {
            if self.session.finish_restore(
                generation,
                &selection.server,
                &selection.user,
                selection.expires_at,
                RestoreResult::Unavailable,
                now(),
            ) == RestoreDecision::Delete
            {
                self.invalidate_storage(cx);
            }
            cx.notify();
            return;
        }
        let reply = store.read(selection.clone());
        let api = self.session.api.clone();
        let server = selection.server.clone();
        let result = bounded(async move {
            match reply.recv().await {
                Ok(Outcome::Token(Some(token))) if !token.is_empty() => match tokio::time::timeout(
                    persistence::DEADLINE,
                    api.current_user(server, token.clone()),
                )
                .await
                {
                    Ok(Ok(user)) => RestoreResult::Verified { user, token },
                    Ok(Err(session::AuthError::AlreadyInvalid)) => RestoreResult::Rejected,
                    _ => RestoreResult::Unavailable,
                },
                Ok(Outcome::Token(None)) => RestoreResult::MissingCredential,
                _ => RestoreResult::Unavailable,
            }
        });
        cx.spawn(async move |weak, cx| {
            let result = result.recv().await.ok().flatten().unwrap_or(RestoreResult::Unavailable);
            let _ = weak.update_in(cx, |view, window, cx| {
                match view.session.finish_restore(generation, &selection.server, &selection.user, selection.expires_at, result, now()) {
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
                }
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

    #[cfg(not(test))]
    fn start_poll_timer(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
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
        self.poll_clock.elapsed()
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
        let (send, receive) = async_channel::bounded(1);
        let work = request.clone();
        runtime().spawn(async move {
            let result = tokio::time::timeout(
                Duration::from_secs(9),
                api.channels(work.server, work.token),
            )
            .await
            .unwrap_or(Err(session::AuthError::Unavailable));
            let _ = send.send(result).await;
        });
        cx.spawn(async move |weak, cx| {
            if let Ok(result) = receive.recv().await {
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
        // The headless scheduler is single-threaded: deliver controlled fixture outcomes on
        // its own executor, without a foreign thread waking it during a GPUI frame.
        #[cfg(test)]
        {
            let timeout = cx.background_executor().timer(Duration::from_secs(9));
            cx.spawn(async move |weak, cx| {
            let result = tokio::select! {
                result = api.history_page(work.server, work.token, work.channel_id.unwrap(), work.before) => result,
                _ = timeout => Err(session::AuthError::Unavailable),
            };
            let _ = weak.update(cx, |view, cx| {
                view.finish_history(serial, request, result, cx)
            });
        })
        .detach();
        }
        #[cfg(not(test))]
        {
            let (send, receive) = async_channel::bounded(1);
            self.history_task = Some(runtime().spawn(async move {
                let result = tokio::time::timeout(
                    Duration::from_secs(9),
                    api.history_page(
                        work.server,
                        work.token,
                        work.channel_id.unwrap(),
                        work.before,
                    ),
                )
                .await
                .unwrap_or(Err(session::AuthError::Unavailable));
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
                    view.update(cx, |view, cx| {
                        view.resume_deletions(cx);
                        view.start_restore(cx);
                    });

                    #[cfg(not(test))]
                    view.update(cx, |view, cx| view.start_poll_timer(cx));
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
    use crate::persistence::{
        Persistence,
        tests::{Controlled, Shared},
    };
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
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

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

    // Exercises the actual signup button -> Hamlet::submit -> enter_authenticated ->
    // save_login chain. The worker is real, but its Store is controlled, not Secret Service.
    struct PersistentSignupAuth {
        verified: Arc<AtomicUsize>,
        revoked: Arc<AtomicBool>,
    }
    impl AuthApi for PersistentSignupAuth {
        fn signup(
            &self,
            server: String,
            username: String,
            password: String,
        ) -> ApiFuture<Result<Login, AuthError>> {
            assert_eq!(server, crate::session::DEFAULT_SERVER_URL);
            assert_eq!(username, "Alice_1");
            assert_eq!(password, "long password");
            Box::pin(async move {
                Ok(Login {
                    user: User {
                        id: "42".into(),
                        username,
                    },
                    token: "controlled-signup-token".into(),
                    expires_at: 4_070_908_800,
                })
            })
        }
        fn login(&self, _: String, _: String, _: String) -> ApiFuture<Result<Login, AuthError>> {
            Box::pin(async { panic!("signup must not call login") })
        }
        fn current_user(
            &self,
            server: String,
            token: String,
        ) -> ApiFuture<Result<User, AuthError>> {
            assert_eq!(server, crate::session::DEFAULT_SERVER_URL);
            self.verified.fetch_add(1, Ordering::SeqCst);
            let valid = token == "controlled-signup-token" && !self.revoked.load(Ordering::SeqCst);
            Box::pin(async move {
                if valid {
                    Ok(User {
                        id: "42".into(),
                        username: "Alice_1".into(),
                    })
                } else {
                    Err(AuthError::AlreadyInvalid)
                }
            })
        }
        fn logout(&self, _: String, token: String) -> ApiFuture<Result<(), AuthError>> {
            assert_eq!(token, "controlled-signup-token");
            self.revoked.store(true, Ordering::SeqCst);
            Box::pin(async { Ok(()) })
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
    }

    #[gpui_kit::test]
    fn signup_controls_save_and_restore_through_view_and_controlled_worker(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let shared: Shared = Arc::new((
            Mutex::new((vec![], false, false, false, false, false)),
            Condvar::new(),
        ));
        let verified = Arc::new(AtomicUsize::new(0));
        let revoked = Arc::new(AtomicBool::new(false));
        let api: Arc<dyn AuthApi> = Arc::new(PersistentSignupAuth {
            verified: verified.clone(),
            revoked: revoked.clone(),
        });
        let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
        let target = saved.clone();
        let first_store = shared.clone();
        let first_path = path.clone();
        let first_api = api.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut view = Hamlet::new(window, cx, first_api);
                view.persistence = Some(Persistence::start(
                    Controlled(first_store),
                    Some(first_path),
                ));
                view
            });
            *target.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let first: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("auth-mode", cx);
            window.click("username", cx);
            window.input("Alice_1", cx);
            window.click("password", cx);
            window.input("long password", cx);
            window.click("signup", cx);
        });
        // The signup future and storage worker run on foreign threads. Pump GPUI until
        // both callbacks have been delivered, without a timing-dependent sleep.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            cx.run_until_parked();
            let saved = cx.update(|_, cx| {
                let view = first.read(cx);
                view.storage_feedback.as_deref()
                    == Some("Login saved in Secret Service for this server and user.")
            });
            if saved {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "signup/storage did not finish"
            );
            std::thread::yield_now();
        }
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find("session-status")
                    .label()
                    .unwrap()
                    .contains("Alice_1")
            );
            assert!(window.find("message-000000000000001").label().is_some());
            let view = first.read(cx);
            assert!(view.session.password.is_empty());
            assert_eq!(view.password.read(cx).text().len(), 0);
        });
        let config = crate::persistence::load_at(Some(&path));
        let selection = config.saved.clone().expect("signup stored selection");
        assert_eq!(
            config.server.as_deref(),
            Some(crate::session::DEFAULT_SERVER_URL)
        );
        assert_eq!(selection.user.username, "Alice_1");
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(!json.contains("long password") && !json.contains("controlled-signup-token"));
        assert_eq!(
            shared.0.lock().unwrap().0.as_slice(),
            &[(
                crate::persistence::account(&selection),
                "controlled-signup-token".into()
            )]
        );

        // Simulate a fresh view/worker using only the saved metadata and the shared
        // controlled credential backend. start_restore must read and verify identity.
        let restored = std::rc::Rc::new(std::cell::RefCell::new(None));
        let target = restored.clone();
        let second_store = shared.clone();
        let second_path = path.clone();
        let second_api = api.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut view = Hamlet::new(window, cx, second_api);
                view.persistence = Some(Persistence::start(
                    Controlled(second_store),
                    Some(second_path),
                ));
                view.credential = config.saved;
                view
            });
            *target.borrow_mut() = Some(view.clone());
            view.update(cx, |view, cx| view.start_restore(cx));
            Root::new(view, window, cx)
        });
        let restarted: gpui_kit::Entity<Hamlet> = restored.borrow().as_ref().unwrap().clone();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            cx.run_until_parked();
            let ready = cx.update(|_, cx| restarted.read(cx).session.active.is_some());
            if ready {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "restoration did not finish"
            );
            std::thread::yield_now();
        }
        assert_eq!(
            verified.load(Ordering::SeqCst),
            1,
            "restore must check /me identity"
        );
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
                restarted.read(cx).session.active.as_ref().unwrap().user,
                selection.user
            );
            window.click("logout", cx);
            window.render_frame(cx);
            assert!(restarted.read(cx).session.active.is_none());
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            cx.run_until_parked();
            if crate::persistence::load_at(Some(&path)).saved.is_none()
                && shared.0.lock().unwrap().0.is_empty()
                && revoked.load(Ordering::SeqCst)
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "logout cleanup did not finish"
            );
            std::thread::yield_now();
        }
        assert!(restarted.read_with(cx, |view, _| view.deletions.is_empty()));
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

    struct RaceAuth {
        pages: std::sync::mpsc::Sender<(Option<String>, PageReply)>,
        sends: Arc<Mutex<Vec<Sent>>>,
    }
    impl AuthApi for RaceAuth {
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
            let tx = self.pages.clone();
            Box::pin(async move {
                let (reply, rx) = async_channel::bounded(1);
                tx.send((before, reply)).unwrap();
                rx.recv().await.unwrap()
            })
        }
        fn send_message(
            &self,
            _: String,
            _: String,
            id: String,
            text: String,
        ) -> ApiFuture<Result<crate::conversation::Message, AuthError>> {
            let (tx, rx) = async_channel::bounded(1);
            self.sends.lock().unwrap().push((id, text, tx));
            Box::pin(async move { rx.recv().await.unwrap() })
        }
    }

    #[gpui_kit::test]
    fn polling_and_send_confirmation_share_one_headless_history_without_duplicate(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let (pages_tx, pages) = std::sync::mpsc::channel();
        let sends = Arc::new(Mutex::new(Vec::<Sent>::new()));
        let captured = sends.clone();
        let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = saved.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                Hamlet::new(
                    window,
                    cx,
                    Arc::new(RaceAuth {
                        pages: pages_tx,
                        sends: captured,
                    }),
                )
            });
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
        cx.deactivate_window();
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        let (_, initial) = pages.recv_timeout(Duration::from_secs(2)).unwrap();
        let message = |id: &str| crate::conversation::Message {
            id: id.into(),
            channel_id: "000000000000001".into(),
            author_id: "42".into(),
            author_name: "Ada".into(),
            text: "same".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let page = |ids: &[&str]| crate::conversation::Page {
            items: ids.iter().map(|id| message(id)).collect(),
            next_cursor: None,
        };
        initial.send_blocking(Ok(page(&["8"]))).unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            window.input("same", cx);
            window.click("send-message", cx);
        });
        cx.run_until_parked();
        assert_eq!(sends.lock().unwrap().len(), 1);
        cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(4), cx)));
        cx.run_until_parked();
        let (_, poll) = pages.recv_timeout(Duration::from_secs(2)).unwrap();
        poll.send_blocking(Ok(page(&["10", "9", "8"]))).unwrap();
        cx.run_until_parked();
        let (_, text, confirmation) = sends.lock().unwrap().remove(0);
        assert_eq!(text, "same");
        confirmation.try_send(Ok(message("10"))).unwrap();
        cx.run_until_parked();
        let (_, read) = pages.recv_timeout(Duration::from_secs(2)).unwrap();
        read.send_blocking(Ok(page(&["10", "9", "8"]))).unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("message-10").label(), Some("same"));
            assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "");
            assert!(
                matches!(view.read(cx).conversation.history.get("000000000000001"),
                Some(crate::conversation::Load::Ready(items)) if items.len() == 3)
            );
            assert!(sends.lock().unwrap().is_empty());
            assert!(pages.try_recv().is_err());
        });
    }

    struct PollAuth {
        channels: Arc<AtomicUsize>,
        history: Arc<AtomicUsize>,
        offline: Arc<AtomicBool>,
    }
    impl AuthApi for PollAuth {
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
            self.channels.fetch_add(1, Ordering::SeqCst);
            if self.offline.load(Ordering::SeqCst) {
                Box::pin(async { Err(AuthError::Unavailable) })
            } else {
                TestAuth.channels(s, t)
            }
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
            self.history.fetch_add(1, Ordering::SeqCst);
            if self.offline.load(Ordering::SeqCst) {
                Box::pin(async { Err(AuthError::Unavailable) })
            } else {
                TestAuth.history(s, t, id)
            }
        }
    }

    #[gpui_kit::test]
    fn poll_catches_up_multiple_pages_without_duplicate_work_and_switch_cancels_late_read(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let (tx, requests) = std::sync::mpsc::channel();
        let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = saved.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PagedAuth(tx))));
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
        cx.deactivate_window();
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        let (_, initial) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        let message = |id: i32| crate::conversation::Message {
            id: id.to_string(),
            channel_id: "000000000000001".into(),
            author_id: "42".into(),
            author_name: "Ada".into(),
            text: id.to_string(),
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        initial
            .send_blocking(Ok(crate::conversation::Page {
                items: vec![message(1)],
                next_cursor: None,
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(4), cx)));
        cx.run_until_parked();
        let (cursor, first) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(cursor.is_none());
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("refresh-history", cx); // pending, inert
            view.update(cx, |v, cx| v.poll_at(Duration::from_secs(20), cx));
        });
        cx.run_until_parked();
        assert!(requests.try_recv().is_err());
        first
            .send_blocking(Ok(crate::conversation::Page {
                items: (53..=102).rev().map(message).collect(),
                next_cursor: Some("next".into()),
            }))
            .unwrap();
        cx.run_until_parked();
        let (cursor, second) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(cursor.as_deref(), Some("next"));
        cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(30), cx)));
        cx.run_until_parked();
        assert!(requests.try_recv().is_err());
        second
            .send_blocking(Ok(crate::conversation::Page {
                items: (1..=52).rev().map(message).collect(),
                next_cursor: None,
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(view.read(cx).history_list.item_count(), 102);
            assert_eq!(window.find("message-102").label(), Some("102"));
            view.update(cx, |v, cx| v.poll_at(Duration::from_secs(35), cx));
        });
        cx.run_until_parked();
        let (_, late) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("channel-000000000000002", cx);
        });
        cx.run_until_parked();
        let (_, selected) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        late.send_blocking(Ok(crate::conversation::Page {
            items: vec![message(999)],
            next_cursor: None,
        }))
        .unwrap();
        selected
            .send_blocking(Ok(crate::conversation::Page {
                items: vec![],
                next_cursor: None,
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).conversation.selected.as_deref(), Some("000000000000002"));
            assert!(!view.read(cx).conversation.history.contains_key("000000000000002") ||
                matches!(view.read(cx).conversation.history.get("000000000000002"), Some(crate::conversation::Load::Ready(items)) if items.is_empty()));
            assert!(matches!(view.read(cx).conversation.history.get("000000000000001"), Some(crate::conversation::Load::Ready(items)) if items.len() == 102));
        });
    }

    // Bridge only the test fixture's HTTP futures onto Tokio; Hamlet still dispatches
    // channels and history via its production poll_at / completion paths.
    struct ServerPollingAuth {
        api: crate::http::HttpAuth,
        completed: std::sync::mpsc::Sender<&'static str>,
    }
    impl AuthApi for ServerPollingAuth {
        fn signup(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
            self.api.signup(s, u, p)
        }
        fn login(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
            self.api.login(s, u, p)
        }
        fn current_user(&self, s: String, t: String) -> ApiFuture<Result<User, AuthError>> {
            self.api.current_user(s, t)
        }
        fn logout(&self, s: String, t: String) -> ApiFuture<Result<(), AuthError>> {
            self.api.logout(s, t)
        }
        fn channels(
            &self,
            s: String,
            t: String,
        ) -> ApiFuture<Result<Vec<crate::conversation::Channel>, AuthError>> {
            let future = self.api.channels(s, t);
            let done = self.completed.clone();
            Box::pin(async move {
                let result = future.await;
                done.send("channels").unwrap();
                result
            })
        }
        fn create_channel(
            &self,
            s: String,
            t: String,
            n: String,
        ) -> ApiFuture<Result<crate::conversation::Channel, AuthError>> {
            self.api.create_channel(s, t, n)
        }
        fn send_message(
            &self,
            s: String,
            t: String,
            id: String,
            text: String,
        ) -> ApiFuture<Result<crate::conversation::Message, AuthError>> {
            self.api.send_message(s, t, id, text)
        }
        fn history(
            &self,
            s: String,
            t: String,
            id: String,
        ) -> ApiFuture<Result<Vec<crate::conversation::Message>, AuthError>> {
            self.api.history(s, t, id)
        }
        fn history_page(
            &self,
            s: String,
            t: String,
            id: String,
            before: Option<String>,
        ) -> ApiFuture<Result<crate::conversation::Page, AuthError>> {
            let future = self.api.history_page(s, t, id, before);
            let done = self.completed.clone();
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            super::runtime().spawn(async move {
                let result = future.await;
                tx.send(result).unwrap();
                done.send("history").unwrap();
            });
            // The headless scheduler forbids a foreign thread waking its futures. The
            // test-only bridge waits for the bounded HTTP read on its own thread instead.
            Box::pin(async move { rx.recv_timeout(Duration::from_secs(9)).unwrap() })
        }
    }

    #[gpui_kit::test]
    fn bob_activity_arrives_through_hamlet_poll_at_and_real_rewrite_routes(
        cx: &mut TestAppContext,
    ) {
        use actix_web::{App, HttpServer, web};
        use std::net::TcpListener;
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let server_thread = std::thread::spawn(move || {
            actix_web::rt::System::new().block_on(async move {
                let dir = tempfile::tempdir().unwrap();
                let db = hamlet::connect_to_database(&format!(
                    "sqlite://{}?mode=rwc",
                    dir.path().join("poll-at.db").display()
                ))
                .await
                .unwrap();
                let listener = TcpListener::bind("127.0.0.1:0").unwrap();
                let url = format!("http://{}", listener.local_addr().unwrap());
                let server = HttpServer::new(move || {
                    App::new()
                        .app_data(web::Data::new(db.clone()))
                        .configure(hamlet::routes)
                })
                .listen(listener)
                .unwrap()
                .run();
                ready_tx.send((url, server.handle())).unwrap();
                server.await.unwrap();
            });
        });
        let (url, server_handle) = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let api = crate::http::HttpAuth::new();
        let alice = super::runtime()
            .block_on(api.signup(url.clone(), "Alice".into(), "long password".into()))
            .unwrap();
        let bob = super::runtime()
            .block_on(api.signup(url.clone(), "Bob".into(), "long password".into()))
            .unwrap();
        let (done, completions) = std::sync::mpsc::channel();
        cx.update(gpui_kit::init);
        let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = saved.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                Hamlet::new(
                    window,
                    cx,
                    Arc::new(ServerPollingAuth {
                        api: crate::http::HttpAuth::new(),
                        completed: done,
                    }),
                )
            });
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
        cx.update(|_, cx| {
            view.update(cx, |v, cx| {
                v.session.change_server(url.clone());
                v.session.username = "Alice".into();
                v.session.password = "long password".into();
                let login = v.session.submit().unwrap();
                v.session
                    .complete_login(login, Ok(alice), chrono::Utc::now().timestamp());
                v.polling.focus(true, Duration::ZERO);
                v.load_channels(cx);
            })
        });
        assert_eq!(
            completions.recv_timeout(Duration::from_secs(5)).unwrap(),
            "channels"
        );
        cx.run_until_parked();
        assert_eq!(
            completions.recv_timeout(Duration::from_secs(5)).unwrap(),
            "history"
        );
        cx.run_until_parked();
        let selected = cx.update(|_, cx| view.read(cx).conversation.selected.clone().unwrap());
        let posted = super::runtime()
            .block_on(api.send_message(
                url.clone(),
                bob.token.clone(),
                selected.clone(),
                "hello from Bob".into(),
            ))
            .unwrap();
        let channel = super::runtime()
            .block_on(api.create_channel(url.clone(), bob.token, "Bob room".into()))
            .unwrap();
        cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(100), cx)));
        assert_eq!(
            completions.recv_timeout(Duration::from_secs(5)).unwrap(),
            "channels"
        );
        cx.run_until_parked();
        assert_eq!(
            completions.recv_timeout(Duration::from_secs(5)).unwrap(),
            "history"
        );
        cx.update(|_, cx| {
            let v = view.read(cx);
            assert!(matches!(&v.conversation.channels, Some(crate::conversation::Load::Ready(items)) if items.contains(&channel)));
            assert!(matches!(v.conversation.history.get(&selected), Some(crate::conversation::Load::Ready(items)) if items.iter().any(|m| m.id == posted.id && m.author_name == "Bob")));
        });
        super::runtime().block_on(server_handle.stop(true));
        server_thread.join().unwrap();
    }

    #[gpui_kit::test]
    fn focus_return_during_read_catches_up_once_and_logout_stops_selected_reads(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let (tx, requests) = std::sync::mpsc::channel();
        let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = saved.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PagedAuth(tx))));
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
        cx.deactivate_window();
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        let (_, initial) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        initial
            .send_blocking(Ok(crate::conversation::Page {
                items: vec![],
                next_cursor: None,
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(4), cx)));
        cx.run_until_parked();
        let (_, pending) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        cx.deactivate_window();
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        assert!(
            requests.try_recv().is_err(),
            "focus must not overlap an in-flight read"
        );
        pending
            .send_blocking(Ok(crate::conversation::Page {
                items: vec![],
                next_cursor: None,
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(v.poll_time(), cx)));
        cx.run_until_parked();
        let (_, catchup) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(requests.try_recv().is_err(), "only one catch-up read");
        catchup
            .send_blocking(Ok(crate::conversation::Page {
                items: vec![],
                next_cursor: None,
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(v.poll_time(), cx)));
        cx.run_until_parked();
        assert!(
            requests.try_recv().is_err(),
            "catch-up resets the normal interval"
        );
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("channel-000000000000002", cx);
        });
        cx.run_until_parked();
        let (_, selected) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(500), cx)));
        cx.run_until_parked();
        assert!(
            requests.try_recv().is_err(),
            "no unselected or overlapping read on switch"
        );
        selected
            .send_blocking(Ok(crate::conversation::Page {
                items: vec![],
                next_cursor: None,
            }))
            .unwrap();
        cx.run_until_parked();
        cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(1000), cx)));
        cx.run_until_parked();
        let (_, late) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            requests.try_recv().is_err(),
            "only the selected channel is scheduled"
        );
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("logout", cx);
            view.update(cx, |v, cx| v.poll_at(Duration::from_secs(1500), cx));
        });
        // Let the stale result arrive; it must not restore the logged-out session.
        let _ = late.try_send(Ok(crate::conversation::Page {
            items: vec![],
            next_cursor: None,
        }));
        cx.run_until_parked();
        assert!(
            requests.try_recv().is_err(),
            "logout must not read any channel"
        );
        cx.update(|_, cx| {
            assert!(view.read(cx).conversation.history.is_empty());
            assert!(view.read(cx).conversation.selected.is_none());
        });
    }

    #[gpui_kit::test]
    fn focused_polls_pause_resume_and_recover_without_losing_draft(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let channels = Arc::new(AtomicUsize::new(0));
        let history = Arc::new(AtomicUsize::new(0));
        let offline = Arc::new(AtomicBool::new(false));
        let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = saved.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                Hamlet::new(
                    window,
                    cx,
                    Arc::new(PollAuth {
                        channels: channels.clone(),
                        history: history.clone(),
                        offline: offline.clone(),
                    }),
                )
            });
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
        cx.deactivate_window();
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
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
            assert_eq!(
                window.find("connection-status").label(),
                Some("Connected. Checking for new messages and channels.")
            );
            window.click("composer", cx);
            window.input("keep draft", cx);
            view.update(cx, |v, cx| v.poll_at(Duration::from_secs(2), cx));
        });
        assert_eq!(history.load(Ordering::SeqCst), 1);
        assert_eq!(channels.load(Ordering::SeqCst), 1);
        cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(4), cx)));
        cx.run_until_parked();
        assert_eq!(history.load(Ordering::SeqCst), 2);
        assert_eq!(channels.load(Ordering::SeqCst), 1);
        cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(16), cx)));
        cx.run_until_parked();
        assert_eq!(channels.load(Ordering::SeqCst), 2);
        offline.store(true, Ordering::SeqCst);
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("refresh-history", cx);
            window.click("refresh-channels", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find("connection-status")
                    .label()
                    .unwrap()
                    .contains("Connection trouble")
            );
            assert!(window.find("message-000000000000001").label().is_some());
            assert_eq!(
                view.read(cx).conversation.draft("000000000000001"),
                "keep draft"
            );
        });
        cx.deactivate_window();
        cx.update(|window, cx| {
            window.render_frame(cx);
            let status_node = window.find("connection-status");
            let status = status_node.label().unwrap();
            assert!(status.contains("Connection trouble"));
            assert!(status.contains("Updates paused"));
            assert!(!status.contains("Retrying"));
        });
        let before = history.load(Ordering::SeqCst);
        cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(200), cx)));
        cx.run_until_parked();
        assert_eq!(history.load(Ordering::SeqCst), before);
        offline.store(false, Ordering::SeqCst);
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("connection-status").label(),
                Some("Connected. Checking for new messages and channels.")
            );
            assert_eq!(
                view.read(cx).conversation.draft("000000000000001"),
                "keep draft"
            );
        });
        assert!(history.load(Ordering::SeqCst) > before);
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
    fn headless_logout_dispatches_deletion_warns_and_retries(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
        let target = saved.clone();
        let shared: Shared = Arc::new((
            Mutex::new((vec![], false, true, false, false, false)),
            Condvar::new(),
        ));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
            view.update(cx, |view, _| {
                view.persistence = Some(Persistence::start(
                    Controlled(shared.clone()),
                    Some(path.clone()),
                ));
            });
            *target.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.session.begin_restore();
                cx.notify();
            });
            window.render_frame(cx);
            assert!(
                window
                    .find("auth-feedback")
                    .label()
                    .unwrap()
                    .contains("Checking saved session")
            );
            assert!(window.find("login").label().unwrap().contains("Signing in"));
            view.update(cx, |view, cx| {
                view.session.cancel_pending();
                cx.notify();
            });
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
                    .find("storage-status")
                    .label()
                    .unwrap()
                    .contains("memory-only")
            );
            // Force an actual failed Secret Service deletion, then retry via the button.
            let selection = view.read(cx).credential.clone().unwrap();
            shared.0.lock().unwrap().2 = false;
            shared.0.lock().unwrap().3 = true;
            assert_eq!(
                view.read(cx)
                    .persistence
                    .as_ref()
                    .unwrap()
                    .save(selection, "token".into())
                    .recv_blocking()
                    .unwrap(),
                super::Outcome::Saved
            );
            shared.0.lock().unwrap().5 = false;
            window.click("logout", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                shared.0.lock().unwrap().5,
                "logout must dispatch deletion without retry"
            );
            assert_eq!(
                serde_json::from_slice::<crate::persistence::Config>(
                    &std::fs::read(&path).unwrap()
                )
                .unwrap()
                .pending_deletions
                .len(),
                1
            );
            assert!(
                window
                    .find("storage-status")
                    .label()
                    .unwrap()
                    .contains("saved login may remain")
            );
            assert!(
                window
                    .find("retry-storage")
                    .label()
                    .unwrap()
                    .contains("deletion")
            );
            shared.0.lock().unwrap().3 = false;
            window.click("retry-storage", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find("storage-status")
                    .label()
                    .unwrap()
                    .contains("removed")
            );
            assert!(shared.0.lock().unwrap().0.is_empty());
        });
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
