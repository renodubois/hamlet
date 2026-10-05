//! One conversation lifetime per accepted session. Views issue intentions and read state;
//! this owner alone dispatches requests, applies completions and schedules selected reads.

mod delivery;
mod live_updates;
mod state;
use crate::api::{ApiError, AuthenticatedClient, Channel, LiveEvent, Message, Page, StreamError};
use crate::runtime::{Execution, Work};
use live_updates::{Attempt, LiveUpdates};
pub(crate) use state::{Conversation, Load, Older};
use state::{CreateRequest, Identity, ReadRequest, SendRequest};
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
    Stream(Attempt, Result<LiveEvent, StreamError>),
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
    live: LiveUpdates,
    stream_task: Option<Work>,
    clock: Instant,
    started: bool,
    updates: delivery::Updates,
    deliver: delivery::Sender,
    timer: Option<Work>,
    channels_task: Option<Work>,
    history_task: Option<Work>,
    history_serial: u64,
    create_task: Option<Work>,
    sends: HashMap<String, Work>,
    created: Option<(u64, String)>,
    create_revision: u64,
    observers: Vec<async_channel::Sender<()>>,
}

// Notify after each intention/completion releases its state borrow, including early returns.
// Each subscriber gets a coalesced invalidation, never a competing executor delivery.
struct Notify<'a>(&'a ConversationHandle);
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

impl ConversationHandle {
    pub(crate) fn new(
        generation: u64,
        expires_at: i64,
        client: AuthenticatedClient,
        execution: Execution,
    ) -> Self {
        let (deliver, updates) = delivery::channel();
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
            live: LiveUpdates::new(generation),
            stream_task: None,
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
            observers: Vec::new(),
        })))
    }
    pub fn read(&self) -> Ref<'_, Conversation> {
        Ref::map(self.0.borrow(), |owner| &owner.state)
    }
    pub fn status(&self) -> &'static str {
        self.0.borrow().live.status()
    }
    /// Presentation may clear an unchanged creation input after this confirmed revision.
    pub fn created(&self) -> Option<(u64, String)> {
        self.0.borrow().created.clone()
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
        owner.selection_changed(previous);
        if let Some(next) = next {
            owner.load_history(next);
        }
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
        let _notify = Notify(self);
        self.0.borrow_mut().apply(update.0)
    }
}

enum HistoryIntent {
    Older,
    RetryOlder,
}
impl Coordinator {
    fn time(&self) -> Duration {
        self.execution.now().duration_since(self.clock)
    }
    fn arm_timer(&mut self) {
        // Expiry keeps its cadence; reconnect wakes at the exact fixed-delay deadline.
        if let Some(task) = self.timer.take() {
            task.abort();
        }
        let elapsed = self.time();
        let next_tick = Duration::from_secs(elapsed.as_secs() + 1);
        let deadline = self
            .live
            .retry_at()
            .map_or(next_tick, |retry| retry.min(next_tick));
        let timer = self.execution.sleep(deadline.saturating_sub(elapsed));
        let deliver = self.deliver.clone();
        let (task, _) = self.execution.start(async move {
            timer.await;
            let _ = deliver.send(ConversationUpdate(Update::Tick)).await;
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
                    Ok(event) => match deliver
                        .try_send(ConversationUpdate(Update::Stream(attempt, Ok(event))))
                    {
                        Ok(()) => continue,
                        Err(async_channel::TrySendError::Full(())) => StreamError::Overflow,
                        Err(async_channel::TrySendError::Closed(())) => break,
                    },
                    Err(error) => error,
                };
                let _ = deliver
                    .send_terminal(ConversationUpdate(Update::Stream(attempt, Err(error))))
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
            LiveEvent::MessageCreated(message) => {
                let _ = self.state.merge_message(message);
            }
            LiveEvent::Ready => {}
        }
    }
    fn disconnect(&mut self) {
        if !self.live.fail(self.live.attempt(), self.time()) {
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
    fn load_channels(&mut self, request: ReadRequest) {
        let Some(api) = self.client.clone() else {
            return;
        };
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
        }
    }
    /// The sole selected-history dispatch path, independent of connection status.
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
                if self.live.retry(self.time()).is_some() {
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
                    Err(StreamError::Api(ApiError::AlreadyInvalid)) => {
                        self.close();
                        return Some(SessionEnd::Rejected(self.generation));
                    }
                    Err(_) => self.disconnect(),
                }
            }
            Update::Channels(request, result) => {
                if !self.state.is_current_channels(&request)
                    || request.generation != self.generation
                {
                    return None;
                }
                self.channels_task = None;
                let previous = self.state.selected.clone();
                let next = self
                    .state
                    .complete_channels(&mut self.identity, &request, result);
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
                self.state
                    .complete_history(&mut self.identity, &request, result);
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
                self.state.complete_send(
                    &mut self.identity,
                    &request,
                    result,
                    self.execution.unix_seconds(),
                );
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
        self.live.close();
        self.created = None;
        self.deliver.close();
        self.cancel_history();
        for task in [
            self.timer.take(),
            self.stream_task.take(),
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
#[path = "tests/live_coordinator.rs"]
mod live_coordinator_tests;
#[cfg(test)]
#[path = "tests/support/live.rs"]
mod live_support;

#[cfg(test)]
#[path = "tests/coordinator.rs"]
mod coordinator_tests;
#[cfg(test)]
#[path = "tests/route.rs"]
mod route_tests;
