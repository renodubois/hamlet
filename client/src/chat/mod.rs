//! One chat lifetime per accepted session. Views issue intentions and read state;
//! this owner alone dispatches requests, applies completions and schedules selected reads.

mod delivery;
mod live_updates;
mod message_deletion;
mod state;

use crate::api::{
    ApiError, ApiFuture, AuthenticatedClient, Channel, LiveEvent, Message, Page, REQUEST_DEADLINE,
    StreamError,
};
use crate::runtime::{Execution, Work};
use live_updates::{Attempt, LiveUpdates};
use message_deletion::MessageDeleteRequest;
use state::{
    ChannelDeleteRequest, ChannelsRead, CreateRequest, HistoryRead, Identity, RenameRequest,
    SendRequest,
};
pub(crate) use state::{ChatState, Load, OlderPages};
use std::{
    cell::{Ref, RefCell},
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

/// A clone shares the same closed gate and state; it cannot reopen a lost session.
#[derive(Clone)]
pub(crate) struct ChatHandle(Rc<RefCell<Coordinator>>);

/// Opaque executor delivery. Hosts never interpret request identities or HTTP results.
// Tests can replay an opaque delivery without interpreting or fabricating identities.
#[cfg_attr(test, derive(Clone))]
pub(crate) struct ChatUpdate(Update);
#[cfg_attr(test, derive(Clone))]
enum Update {
    Tick,
    Stream(Attempt, Result<LiveEvent, StreamError>),
    Channels(ChannelsRead, Result<Vec<Channel>, ApiError>),
    History(HistoryRead, Result<Page, ApiError>),
    Create(CreateRequest, Result<Channel, ApiError>),
    Rename(RenameRequest, Result<Channel, ApiError>),
    ChannelDelete(ChannelDeleteRequest, Result<(), ApiError>),
    MessageDelete(MessageDeleteRequest, Result<(), ApiError>),
    Send(SendRequest, Result<Message, ApiError>),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SessionEnd {
    Rejected(u64),
    Expired(u64),
}

struct Coordinator {
    state: ChatState,
    identity: Identity,
    user_id: String,
    client: Option<AuthenticatedClient>,
    execution: Execution,
    live: LiveUpdates,
    stream_task: Option<Work>,
    /// Origin for the reconnect/expiry timer's monotonic offsets.
    started_at: Instant,
    started: bool,
    updates: delivery::Updates,
    deliver: delivery::Sender,
    timer: Option<Work>,
    channels_task: Option<Work>,
    history_task: Option<Work>,
    create_task: Option<Work>,
    rename_task: Option<Work>,
    channel_delete_task: Option<Work>,
    message_delete_task: Option<Work>,
    sends: HashMap<String, Work>,
    observers: Vec<async_channel::Sender<()>>,
}

// Notify after each intention/completion releases its state borrow, including early returns.
// Each subscriber gets a coalesced invalidation, never a competing executor delivery.
struct Notify<'a>(&'a ChatHandle);
impl Drop for Notify<'_> {
    fn drop(&mut self) {
        self.0.0.borrow_mut().observers.retain(|observer| {
            !matches!(
                observer.try_send(()),
                Err(async_channel::TrySendError::Closed(_))
            )
        });
    }
}

impl ChatHandle {
    pub(crate) fn new(
        generation: u64,
        user_id: String,
        expires_at: i64,
        client: AuthenticatedClient,
        execution: Execution,
    ) -> Self {
        let (deliver, updates) = delivery::channel();
        Self(Rc::new(RefCell::new(Coordinator {
            state: ChatState::default(),
            identity: Identity::new(generation, expires_at),
            user_id,
            client: Some(client),
            started_at: execution.now(),
            execution,
            live: LiveUpdates::new(generation),
            stream_task: None,
            started: false,
            updates,
            deliver,
            timer: None,
            channels_task: None,
            history_task: None,
            create_task: None,
            rename_task: None,
            channel_delete_task: None,
            message_delete_task: None,
            sends: HashMap::new(),
            observers: Vec::new(),
        })))
    }
    pub fn read(&self) -> Ref<'_, ChatState> {
        Ref::map(self.0.borrow(), |owner| &owner.state)
    }
    /// Inject displayed rows at the controlled-history seam without changing cursors,
    /// older-read state or executing requests. No deletion workflow is introduced.
    #[cfg(test)]
    pub(crate) fn replace_history_for_test(&self, channel: &str, messages: Vec<Message>) {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        let Some(Load::Ready(rows)) = owner
            .state
            .conversations
            .get_mut(channel)
            .and_then(|conversation| conversation.history.as_mut())
        else {
            panic!("controlled history must already be ready");
        };
        *rows = messages;
    }
    #[cfg(test)]
    pub fn status(&self) -> &'static str {
        self.0.borrow().live.status()
    }
    /// Presentation may clear an unchanged creation input after this confirmed revision.
    pub fn created(&self) -> Option<(u64, String)> {
        let state = self.read();
        let name = state.last_created.clone()?;
        Some((state.create.revision, name))
    }
    pub fn updates(&self) -> delivery::Updates {
        self.0.borrow().updates.clone()
    }
    /// Independent, bounded feature invalidations. Hydrate from `read()` on subscription;
    /// notifications carry no state or request results. Dropped receivers are pruned.
    pub fn notifications(&self) -> async_channel::Receiver<()> {
        let (send, receive) = async_channel::bounded(1);
        self.0.borrow_mut().observers.push(send);
        receive
    }
    pub fn start(&self) {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        if owner.started || owner.client.is_none() {
            return;
        }
        owner.started = true;
        owner.open_stream();
        let Coordinator {
            state, identity, ..
        } = &mut *owner;
        if let Some(request) = state.start(identity) {
            owner.load_channels(request);
        }
        owner.arm_timer();
    }
    pub fn close(&self) {
        let _notify = Notify(self);
        self.0.borrow_mut().close();
    }
    pub fn edit_draft(&self, text: String) {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        if owner.client.is_none() {
            return;
        }
        if let Some(id) = owner.state.selected.clone() {
            owner.state.set_draft(&id, text);
        }
    }
    /// An input may deliver its final edit after navigation; retain its originating channel.
    pub fn edit_channel_draft(&self, channel: &str, text: String) {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        if owner.client.is_some() {
            owner.state.set_draft(channel, text);
        }
    }
    pub fn select_channel(&self, id: &str) {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        if owner.client.is_none() {
            return;
        }
        let previous = owner.state.selected.clone();
        let Coordinator {
            state, identity, ..
        } = &mut *owner;
        let next = state.select(identity, id);
        owner.selection_changed(previous, next);
    }
    pub fn request_older(&self) {
        let _notify = Notify(self);
        self.0.borrow_mut().history(HistoryIntent::Older);
    }
    pub fn retry_older(&self) {
        let _notify = Notify(self);
        self.0.borrow_mut().history(HistoryIntent::RetryOlder);
    }
    pub fn create_channel(&self, name: &str) {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        let Some(api) = owner.client.clone() else {
            return;
        };
        let Coordinator {
            state, identity, ..
        } = &mut *owner;
        let Some(request) = state.create(identity, name) else {
            return;
        };
        let call = api.create_channel(request.name.clone());
        owner.create_task = Some(owner.dispatch(call, |result| Update::Create(request, result)));
    }
    pub fn reset_channel_delete_feedback(&self) {
        self.0.borrow_mut().state.channel_delete.reset_feedback();
    }
    pub fn delete_channel(&self, id: &str) {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        let now = owner.execution.unix_seconds();
        owner.identity.expire(now);
        let Some(api) = owner.client.clone() else {
            return;
        };
        let Coordinator {
            state, identity, ..
        } = &mut *owner;
        let Some(request) = state.channel_delete(identity, id) else {
            return;
        };
        let call = api.delete_channel(request.channel_id.clone());
        owner.channel_delete_task =
            Some(owner.dispatch(call, |result| Update::ChannelDelete(request, result)));
    }
    pub fn reset_rename_feedback(&self) {
        self.0.borrow_mut().state.rename.reset_feedback();
    }
    pub fn rename_channel(&self, id: &str, name: &str) {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        let now = owner.execution.unix_seconds();
        owner.identity.expire(now);
        let Some(api) = owner.client.clone() else {
            return;
        };
        let Coordinator {
            state, identity, ..
        } = &mut *owner;
        let Some(request) = state.rename(identity, id, name) else {
            return;
        };
        let call = api.rename_channel(request.channel_id.clone(), request.name.clone());
        owner.rename_task = Some(owner.dispatch(call, |result| Update::Rename(request, result)));
    }
    pub fn send(&self) {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        let Some(api) = owner.client.clone() else {
            return;
        };
        let Coordinator {
            state, identity, ..
        } = &mut *owner;
        let Some(request) = state.send(identity) else {
            return;
        };
        let channel = request.channel_id.clone();
        let call = api.send_message(channel.clone(), request.text.clone());
        let task = owner.dispatch(call, |result| Update::Send(request, result));
        owner.sends.insert(channel, task);
    }
    pub fn owns_message(&self, message: &Message) -> bool {
        let owner = self.0.borrow();
        owner.client.is_some() && owner.user_id == message.author_id
    }
    /// Shared opening seam for message actions; chat resolves and captures the target.
    pub fn open_message_deletion(&self, channel_id: &str, message_id: &str) -> bool {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        if owner.client.is_none() {
            return false;
        }
        let now = owner.execution.unix_seconds();
        let Coordinator {
            state,
            identity,
            user_id,
            ..
        } = &mut *owner;
        state.open_message_deletion(identity, user_id, now, channel_id, message_id)
    }
    pub fn cancel_message_deletion(&self) {
        let _notify = Notify(self);
        self.0.borrow_mut().state.cancel_message_deletion();
    }
    pub fn confirm_message_deletion(&self) {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        let Some(api) = owner.client.clone() else {
            return;
        };
        let now = owner.execution.unix_seconds();
        let Coordinator {
            state, identity, ..
        } = &mut *owner;
        let Some(request) = state.confirm_message_deletion(identity, now) else {
            return;
        };
        let call = api.delete_message(request.channel_id.clone(), request.message_id.clone());
        owner.message_delete_task =
            Some(owner.dispatch(call, |result| Update::MessageDelete(request, result)));
    }
    /// Applies only this lifetime's opaque delivery, reporting authoritative loss by identity.
    pub fn apply(&self, update: ChatUpdate) -> Option<SessionEnd> {
        let _notify = Notify(self);
        self.0.borrow_mut().apply(update.0)
    }
}

enum HistoryIntent {
    Older,
    RetryOlder,
}
impl Coordinator {
    /// The single protected-request path: bounded by the API deadline and delivered as an
    /// opaque update. A deadline is not authoritative; it completes as `Unavailable`.
    fn dispatch<T: Send + 'static>(
        &self,
        call: ApiFuture<Result<T, ApiError>>,
        complete: impl FnOnce(Result<T, ApiError>) -> Update + Send + 'static,
    ) -> Work {
        let deliver = self.deliver.clone();
        let bounded = self.execution.deadline(REQUEST_DEADLINE, call);
        let (task, _) = self.execution.start(async move {
            let result = bounded.await.unwrap_or(Err(ApiError::Unavailable));
            let _ = deliver.send(ChatUpdate(complete(result))).await;
        });
        task
    }
    fn elapsed(&self) -> Duration {
        self.execution.now().duration_since(self.started_at)
    }
    fn arm_timer(&mut self) {
        // Expiry keeps its cadence; reconnect wakes at the exact fixed-delay deadline.
        if let Some(task) = self.timer.take() {
            task.abort();
        }
        let elapsed = self.elapsed();
        let next_tick = Duration::from_secs(elapsed.as_secs() + 1);
        let deadline = self
            .live
            .retry_at()
            .map_or(next_tick, |retry| retry.min(next_tick));
        let timer = self.execution.sleep(deadline.saturating_sub(elapsed));
        let deliver = self.deliver.clone();
        let (task, _) = self.execution.start(async move {
            timer.await;
            let _ = deliver.send(ChatUpdate(Update::Tick)).await;
        });
        self.timer = Some(task);
    }
    fn open_stream(&mut self) {
        let Some(api) = &self.client else { return };
        let attempt = self.live.attempt();
        let mut stream = api.events(&self.execution);
        let deliver = self.deliver.clone();
        let (task, _) = self.execution.start(async move {
            loop {
                let error = match stream.next().await {
                    Ok(event) => {
                        match deliver.try_send(ChatUpdate(Update::Stream(attempt, Ok(event)))) {
                            Ok(()) => continue,
                            Err(async_channel::TrySendError::Full(())) => StreamError::Overflow,
                            Err(async_channel::TrySendError::Closed(())) => break,
                        }
                    }
                    Err(error) => error,
                };
                let _ = deliver
                    .send_terminal(ChatUpdate(Update::Stream(attempt, Err(error))))
                    .await;
                break;
            }
        });
        self.stream_task = Some(task);
    }
    fn merge_event(&mut self, event: LiveEvent) {
        match event {
            LiveEvent::ChannelCreated(channel) => {
                let _ = self.state.merge_channel(channel);
            }
            LiveEvent::ChannelRenamed(channel) => self.state.merge_rename(channel),
            LiveEvent::ChannelDeleted(id) => {
                let previous = self.state.selected.clone();
                let next = self.state.merge_channel_deletion(&self.identity, &id);
                if let Some(task) = self.sends.remove(&id) {
                    task.abort();
                }
                self.selection_changed(previous, next);
            }
            LiveEvent::MessageCreated(message) => {
                let _ = self.state.merge_message(message);
            }
            LiveEvent::MessageDeleted {
                channel_id,
                message_id,
            } => {
                self.state.merge_message_deletion(&channel_id, &message_id);
            }
            LiveEvent::Ready => {}
        }
    }
    fn disconnect(&mut self) {
        if !self.live.fail(self.live.attempt(), self.elapsed()) {
            return;
        }
        if let Some(task) = self.stream_task.take() {
            task.abort();
        }
        self.arm_timer();
    }
    fn history(&mut self, intent: HistoryIntent) {
        if self.client.is_none() {
            return;
        }
        let request = match intent {
            HistoryIntent::Older => self.state.request_older(&self.identity),
            HistoryIntent::RetryOlder => self.state.retry_older(&self.identity),
        };
        if let Some(request) = request {
            self.load_history(request);
        }
    }
    fn load_channels(&mut self, request: ChannelsRead) {
        let Some(api) = self.client.clone() else {
            return;
        };
        self.channels_task =
            Some(self.dispatch(api.channels(), |result| Update::Channels(request, result)));
    }
    fn cancel_history(&mut self) {
        if let Some(task) = self.history_task.take() {
            task.abort();
        }
    }
    /// Abandons a read for a deselected channel, then starts the next selected read.
    fn selection_changed(&mut self, previous: Option<String>, next: Option<HistoryRead>) {
        if previous != self.state.selected {
            self.cancel_history();
        }
        if let Some(next) = next {
            self.load_history(next);
        }
    }
    /// The sole selected-history dispatch path, independent of connection status.
    fn load_history(&mut self, request: HistoryRead) {
        self.cancel_history();
        let Some(api) = self.client.clone() else {
            return;
        };
        let call = api.history_page(request.channel_id.clone(), request.before.clone());
        self.history_task = Some(self.dispatch(call, |result| Update::History(request, result)));
    }
    fn apply(&mut self, update: Update) -> Option<SessionEnd> {
        self.client.as_ref()?;
        self.identity.expire(self.execution.unix_seconds());
        let session = self.identity.session;
        if self.identity.session_generation().is_none() {
            self.close();
            return Some(SessionEnd::Expired(session));
        }
        match update {
            Update::Tick => {
                self.timer = None;
                if self.live.retry(self.elapsed()).is_some() {
                    self.open_stream();
                }
                self.arm_timer();
            }
            Update::Stream(attempt, result) => {
                if !self.live.accepts(attempt) {
                    return None;
                }
                match result {
                    Ok(LiveEvent::Ready) => self.live.ready(attempt),
                    Ok(event) => self.merge_event(event),
                    Err(StreamError::Api(ApiError::SessionRejected)) => {
                        self.close();
                        return Some(SessionEnd::Rejected(session));
                    }
                    Err(_) => self.disconnect(),
                }
            }
            Update::Channels(request, result) => {
                if request.generation != session || !self.state.is_current_channels(&request) {
                    return None;
                }
                self.channels_task = None;
                let previous = self.state.selected.clone();
                let next = self
                    .state
                    .complete_channels(&mut self.identity, &request, result);
                self.selection_changed(previous, next);
            }
            Update::History(request, result) => {
                if request.generation != session {
                    return None;
                }
                self.state
                    .complete_history(&mut self.identity, &request, result);
            }
            Update::Create(request, result) => {
                if request.generation != session {
                    return None;
                }
                self.create_task = None;
                let previous = self.state.selected.clone();
                let next = self.state.complete_create(
                    &mut self.identity,
                    &request,
                    result,
                    self.execution.unix_seconds(),
                );
                self.selection_changed(previous, next);
            }
            Update::Rename(request, result) => {
                if request.generation != session {
                    return None;
                }
                self.rename_task = None;
                self.state
                    .complete_rename(&mut self.identity, &request, result);
            }
            Update::ChannelDelete(request, result) => {
                if request.generation != session {
                    return None;
                }
                self.channel_delete_task = None;
                let previous = self.state.selected.clone();
                let revision = self.state.channel_delete.revision;
                let next = self
                    .state
                    .complete_channel_delete(&mut self.identity, &request, result);
                if revision != self.state.channel_delete.revision
                    && let Some(task) = self.sends.remove(&request.channel_id)
                {
                    task.abort();
                }
                self.selection_changed(previous, next);
            }
            Update::MessageDelete(request, result) => {
                if request.generation != session {
                    return None;
                }
                if self
                    .state
                    .complete_message_delete(&mut self.identity, &request, result)
                {
                    self.message_delete_task = None;
                }
            }
            Update::Send(request, result) => {
                if request.generation != session {
                    return None;
                }
                self.sends.remove(&request.channel_id);
                self.state.complete_send(
                    &mut self.identity,
                    &request,
                    result,
                    self.execution.unix_seconds(),
                );
            }
        }
        if self.identity.take_rejected() {
            self.close();
            return Some(SessionEnd::Rejected(session));
        }
        None
    }
    fn close(&mut self) {
        self.client = None;
        self.identity.close();
        self.state.clear();
        self.live.close();
        self.deliver.close();
        for task in [
            self.timer.take(),
            self.stream_task.take(),
            self.channels_task.take(),
            self.history_task.take(),
            self.create_task.take(),
            self.rename_task.take(),
            self.channel_delete_task.take(),
            self.message_delete_task.take(),
        ]
        .into_iter()
        .flatten()
        {
            task.abort();
        }
        for (_, task) in self.sends.drain() {
            task.abort();
        }
    }
}
impl Drop for Coordinator {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
#[path = "tests/support/live.rs"]
mod live_support;
#[cfg(test)]
#[path = "tests/support/message_delete.rs"]
mod message_delete_support;

#[cfg(test)]
#[path = "tests/channel_delete_coordinator.rs"]
mod channel_delete_coordinator_tests;
#[cfg(test)]
#[path = "tests/coordinator.rs"]
mod coordinator_tests;
#[cfg(test)]
#[path = "tests/live_coordinator.rs"]
mod live_coordinator_tests;
#[cfg(test)]
#[path = "tests/message_delete_confirmation.rs"]
mod message_delete_confirmation_tests;
#[cfg(test)]
#[path = "tests/message_delete_events.rs"]
mod message_delete_events_tests;
#[cfg(test)]
#[path = "tests/message_delete_races.rs"]
mod message_delete_races_tests;
#[cfg(test)]
#[path = "tests/route.rs"]
mod route_tests;
