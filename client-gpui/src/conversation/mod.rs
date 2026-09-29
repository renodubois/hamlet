//! One conversation lifetime per accepted session. Views issue intentions and read state;
//! this owner alone dispatches requests, applies completions and schedules selected reads.

pub(crate) mod polling;
mod state;
use crate::api::{ApiError, AuthenticatedClient};
use crate::runtime::{Execution, Work};
use polling::{Polling, Resource};
#[cfg(not(test))]
use state::ReadRequest;
pub(crate) use state::{Channel, Conversation, Load, Message, Older, Page, Refresh};
use state::{CreateRequest, Identity, SendOutcome, SendRequest};
#[cfg(test)]
pub(crate) use state::{Identity as ConversationIdentity, ReadRequest};
use std::{
    cell::{Ref, RefCell},
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

const READ_SEND_DEADLINE: Duration = Duration::from_secs(9);

/// A clone shares the same closed gate and state; it cannot reopen a lost session.
#[derive(Clone)]
pub(crate) struct ConversationHandle(Rc<RefCell<Coordinator>>);

/// Opaque executor delivery. Hosts never interpret request identities or HTTP results.
pub(crate) struct ConversationUpdate(Update);
enum Update {
    Tick,
    Channels(ReadRequest, Result<Vec<Channel>, ApiError>),
    History(u64, ReadRequest, Result<Page, ApiError>),
    Create(CreateRequest, Result<Channel, ApiError>),
    Send(SendRequest, Result<Message, ApiError>),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SessionEnd {
    Rejected(u64),
    Expired(u64),
}

struct Coordinator {
    state: Conversation,
    identity: Identity,
    generation: u64,
    client: Option<AuthenticatedClient>,
    execution: Execution,
    polling: Polling,
    clock: Instant,
    started: bool,
    updates: async_channel::Receiver<ConversationUpdate>,
    deliver: async_channel::Sender<ConversationUpdate>,
    timer: Option<Work>,
    channels_task: Option<Work>,
    history_task: Option<Work>,
    history_serial: u64,
    create_task: Option<Work>,
    sends: HashMap<String, Work>,
    created: Option<(u64, String)>,
    create_revision: u64,
}

impl ConversationHandle {
    pub(crate) fn new(
        generation: u64,
        expires_at: i64,
        client: AuthenticatedClient,
        execution: Execution,
    ) -> Self {
        let (deliver, updates) = async_channel::unbounded();
        Self(Rc::new(RefCell::new(Coordinator {
            state: Conversation::default(),
            identity: Identity {
                generation: Some(generation),
                expires_at,
                rejected: None,
            },
            generation,
            client: Some(client),
            clock: execution.now(),
            execution,
            polling: Polling::default(),
            started: false,
            updates,
            deliver,
            timer: None,
            channels_task: None,
            history_task: None,
            history_serial: 0,
            create_task: None,
            sends: HashMap::new(),
            created: None,
            create_revision: 0,
        })))
    }
    pub fn read(&self) -> Ref<'_, Conversation> {
        Ref::map(self.0.borrow(), |owner| &owner.state)
    }
    pub fn status(&self) -> &'static str {
        self.0.borrow().polling.status()
    }
    /// Presentation may clear an unchanged creation input after this confirmed revision.
    pub fn created(&self) -> Option<(u64, String)> {
        self.0.borrow().created.clone()
    }
    pub fn updates(&self) -> async_channel::Receiver<ConversationUpdate> {
        self.0.borrow().updates.clone()
    }
    pub fn start(&self, focused: bool) {
        let mut owner = self.0.borrow_mut();
        if owner.started || owner.client.is_none() {
            return;
        }
        owner.started = true;
        let time = owner.time();
        owner.polling.focus(focused, time);
        let Coordinator {
            state, identity, ..
        } = &mut *owner;
        if let Some(request) = state.start(identity) {
            owner.load_channels(request);
        }
        owner.arm_timer();
    }
    pub fn close(&self) {
        self.0.borrow_mut().close();
    }
    pub fn set_focused(&self, focused: bool) {
        let mut owner = self.0.borrow_mut();
        if owner.client.is_none() {
            return;
        }
        let time = owner.time();
        if owner.polling.focus(focused, time) {
            owner.poll();
        }
    }
    pub fn edit_draft(&self, text: String) {
        let mut owner = self.0.borrow_mut();
        if owner.client.is_none() {
            return;
        }
        if let Some(id) = owner.state.selected.clone() {
            owner.state.set_draft(&id, text);
        }
    }
    pub fn select_channel(&self, id: &str) {
        let mut owner = self.0.borrow_mut();
        if owner.client.is_none() {
            return;
        }
        let previous = owner.state.selected.clone();
        let Coordinator {
            state, identity, ..
        } = &mut *owner;
        let next = state.select(identity, id);
        owner.selection_changed(previous);
        if let Some(next) = next {
            owner.load_history(next);
        }
    }
    pub fn refresh_channels(&self) {
        let mut owner = self.0.borrow_mut();
        if owner.client.is_none() {
            return;
        }
        let Coordinator {
            state, identity, ..
        } = &mut *owner;
        if let Some(request) = state.refresh_channels(identity) {
            owner.load_channels(request);
        }
    }
    pub fn refresh_history(&self) {
        self.0.borrow_mut().history(HistoryIntent::Refresh);
    }
    pub fn request_older(&self) {
        self.0.borrow_mut().history(HistoryIntent::Older);
    }
    pub fn retry_older(&self) {
        self.0.borrow_mut().history(HistoryIntent::RetryOlder);
    }
    pub fn create_channel(&self, name: &str) {
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
        let work = request.clone();
        let deliver = owner.deliver.clone();
        let (task, _) = owner.execution.start(async move {
            let result = api.create_channel(work.name).await;
            let _ = deliver
                .send(ConversationUpdate(Update::Create(request, result)))
                .await;
        });
        owner.create_task = Some(task);
    }
    pub fn send(&self) {
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
        let work = request.clone();
        let channel = request.channel_id.clone();
        let deliver = owner.deliver.clone();
        let timeout = owner.execution.sleep(READ_SEND_DEADLINE);
        let (task, _) = owner.execution.start(async move {
            let result = tokio::select! {
                biased;
                _ = timeout => Err(ApiError::Unavailable),
                result = api.send_message(work.channel_id, work.text) => result,
            };
            let _ = deliver
                .send(ConversationUpdate(Update::Send(request, result)))
                .await;
        });
        owner.sends.insert(channel, task);
    }
    /// Applies only this lifetime's opaque delivery, reporting authoritative loss by identity.
    pub fn apply(&self, update: ConversationUpdate) -> Option<SessionEnd> {
        self.0.borrow_mut().apply(update.0)
    }
}

enum HistoryIntent {
    Refresh,
    Older,
    RetryOlder,
}
impl Coordinator {
    fn time(&self) -> Duration {
        self.execution.now().duration_since(self.clock)
    }
    fn arm_timer(&mut self) {
        // Keep the one-second cadence anchored to this activity's clock, not to
        // delivery latency across executors. A delayed tick must not move the
        // next 3s/15s resource wakeup past its deadline.
        let elapsed = self.time();
        let next_tick = Duration::from_secs(elapsed.as_secs() + 1);
        let timer = self.execution.sleep(next_tick - elapsed);
        let deliver = self.deliver.clone();
        let (task, _) = self.execution.start(async move {
            timer.await;
            let _ = deliver.send(ConversationUpdate(Update::Tick)).await;
        });
        self.timer = Some(task);
    }
    fn poll(&mut self) {
        if self.client.is_none() {
            return;
        }
        let time = self.time();
        if self.polling.due(Resource::Channels, time)
            && let Some(request) = self.state.refresh_channels(&self.identity)
        {
            self.load_channels(request);
        }
        if self.polling.due(Resource::History, time)
            && let Some(id) = self.state.selected.as_deref()
            && !matches!(self.state.older.get(id), Some(Older::Loading))
        {
            self.history(HistoryIntent::Refresh);
        }
    }
    fn history(&mut self, intent: HistoryIntent) {
        if self.client.is_none() {
            return;
        }
        let request = match intent {
            HistoryIntent::Refresh => self.state.refresh_history(&self.identity),
            HistoryIntent::Older => self.state.request_older(&self.identity),
            HistoryIntent::RetryOlder => self.state.retry_older(&self.identity),
        };
        if let Some(request) = request {
            if matches!(intent, HistoryIntent::Refresh) {
                self.polling.started(Resource::History);
            }
            self.load_history(request);
        }
    }
    fn load_channels(&mut self, request: ReadRequest) {
        let Some(api) = self.client.clone() else {
            return;
        };
        self.polling.started(Resource::Channels);
        let deliver = self.deliver.clone();
        let timeout = self.execution.sleep(READ_SEND_DEADLINE);
        let (task, _) = self.execution.start(async move {
            let result = tokio::select! {
                biased;
                _ = timeout => Err(ApiError::Unavailable),
                result = api.channels() => result,
            };
            let _ = deliver
                .send(ConversationUpdate(Update::Channels(request, result)))
                .await;
        });
        self.channels_task = Some(task);
    }
    fn cancel_history(&mut self) {
        self.history_serial = self.history_serial.wrapping_add(1);
        if let Some(task) = self.history_task.take() {
            task.abort();
        }
    }
    fn selection_changed(&mut self, previous: Option<String>) {
        if previous != self.state.selected {
            self.cancel_history();
            self.polling.reset_history(self.time());
        }
    }
    /// The sole selected-history dispatch path, including traversal and post-send catch-up.
    fn load_history(&mut self, request: ReadRequest) {
        self.cancel_history();
        let serial = self.history_serial;
        let Some(api) = self.client.clone() else {
            return;
        };
        let work = request.clone();
        let deliver = self.deliver.clone();
        let timeout = self.execution.sleep(READ_SEND_DEADLINE);
        let (task, _) = self.execution.start(async move {
            let result = tokio::select! {
                biased;
                _ = timeout => Err(ApiError::Unavailable),
                result = api.history_page(work.channel_id.unwrap(), work.before) => result,
            };
            let _ = deliver
                .send(ConversationUpdate(Update::History(serial, request, result)))
                .await;
        });
        self.history_task = Some(task);
    }
    fn completed(&mut self, resource: Resource, succeeded: bool) {
        let time = self.time();
        if self.polling.completed(resource, succeeded, time) {
            self.polling.recover_other(resource, time);
        }
    }
    fn apply(&mut self, update: Update) -> Option<SessionEnd> {
        self.client.as_ref()?;
        self.identity.expire(self.execution.unix_seconds());
        if self.identity.generation.is_none() {
            self.close();
            return Some(SessionEnd::Expired(self.generation));
        }
        match update {
            Update::Tick => {
                self.timer = None;
                self.poll();
                self.arm_timer();
            }
            Update::Channels(request, result) => {
                if !self.state.is_current_channels(&request)
                    || request.generation != self.generation
                {
                    return None;
                }
                self.channels_task = None;
                let succeeded = result.is_ok();
                let previous = self.state.selected.clone();
                let next = self
                    .state
                    .complete_channels(&mut self.identity, &request, result);
                self.completed(Resource::Channels, succeeded);
                self.selection_changed(previous);
                if let Some(next) = next {
                    self.load_history(next);
                }
            }
            Update::History(serial, request, result) => {
                if serial != self.history_serial || request.generation != self.generation {
                    return None;
                }
                self.history_task = None;
                let succeeded = result.is_ok();
                let is_older = request.before.is_some() && !request.is_catchup();
                let outcome = self
                    .state
                    .complete_history(&mut self.identity, &request, result);
                if !is_older && outcome.next.is_none() {
                    let complete = succeeded
                        && !matches!(
                            request
                                .channel_id
                                .as_ref()
                                .and_then(|id| self.state.refreshing.get(id)),
                            Some(Refresh::Incomplete(_))
                        );
                    self.completed(Resource::History, complete);
                }
                if let Some(next) = outcome.next {
                    self.load_history(next);
                }
                // Failure retains contiguous history and needs a deliberate/scheduled retry;
                // it must not spin while a confirmation waits for continuity.
                if let Some(next) = self.state.after_history_read(&self.identity, succeeded) {
                    self.load_history(next);
                }
            }
            Update::Create(request, result) => {
                if request.generation != self.generation {
                    return None;
                }
                self.create_task = None;
                let previous = self.state.selected.clone();
                let (confirmed, next) = self.state.complete_create(
                    &mut self.identity,
                    &request,
                    result,
                    self.execution.unix_seconds(),
                );
                if confirmed {
                    self.create_revision = self.create_revision.wrapping_add(1);
                    self.created = Some((self.create_revision, request.name));
                }
                self.selection_changed(previous);
                if let Some(next) = next {
                    self.load_history(next);
                }
            }
            Update::Send(request, result) => {
                if request.generation != self.generation {
                    return None;
                }
                self.sends.remove(&request.channel_id);
                let selected = self.state.selected.as_deref() == Some(&request.channel_id);
                let outcome = self.state.complete_send(
                    &mut self.identity,
                    &request,
                    result,
                    self.execution.unix_seconds(),
                );
                if selected {
                    let next = match outcome {
                        SendOutcome::Confirmed => self.state.reconcile_confirmed(&self.identity),
                        SendOutcome::Uncertain => self.state.reconcile_uncertain(&self.identity),
                        _ => None,
                    };
                    if let Some(next) = next {
                        self.load_history(next);
                    }
                }
            }
        }
        if let Some(generation) = self.identity.rejected.take() {
            self.close();
            return Some(SessionEnd::Rejected(generation));
        }
        None
    }
    fn close(&mut self) {
        self.client = None;
        self.identity.generation = None;
        self.state.clear();
        self.polling = Polling::default();
        self.created = None;
        self.deliver.close();
        self.cancel_history();
        for task in [
            self.timer.take(),
            self.channels_task.take(),
            self.create_task.take(),
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
mod coordinator_tests;
