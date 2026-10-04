use actix_web::web::Bytes;
use hamlet_protocol::Event;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::broadcast;

const CAPACITY: usize = 256;

#[cfg(test)]
#[path = "tests/hub.rs"]
mod tests;
#[cfg(test)]
#[path = "tests/transport.rs"]
mod transport_tests;

/// One process-wide, bounded fanout. Clones share retention, not payload copies.
#[derive(Clone)]
pub struct EventHub {
    sender: broadcast::Sender<Delivery>,
    interrupted: Arc<AtomicBool>,
}

impl Default for EventHub {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(CAPACITY);
        Self {
            sender,
            interrupted: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Clone)]
enum Delivery {
    Change(Bytes),
    Interrupted,
}

/// Serialized before a write so publication needs no fallible payload work.
pub struct PreparedEvent(Bytes);

impl PreparedEvent {
    pub fn new(event: &Event) -> Result<Self, serde_json::Error> {
        let json = serde_json::to_string(event)?;
        Ok(Self(Bytes::from(format!(
            "event: change\ndata: {json}\n\n"
        ))))
    }
}

impl EventHub {
    /// Synchronous enqueue of a prepared change. No subscribers is normal.
    /// Bytes clones share immutable storage; this never awaits network delivery.
    pub fn notify(&self, event: PreparedEvent) {
        let _ = self.sender.send(Delivery::Change(event.0));
    }

    /// An unexpected write-task failure makes delivery unsafe until process restart.
    /// Latch closed for fresh subscribers too: a canceled SQLite statement might
    /// still commit *after* a reconnect's baseline read. Ordinary DB errors do not
    /// take this path. Wake existing idle subscribers without waiting for clients.
    pub(crate) fn interrupt(&self) {
        self.interrupted.store(true, Ordering::Release);
        let _ = self.sender.send(Delivery::Interrupted);
    }

    pub fn subscribe(&self) -> Subscription {
        Subscription {
            receiver: self.sender.subscribe(),
            interrupted: self.interrupted.clone(),
            ready: false,
            closed: false,
        }
    }
}

/// A fresh subscription; no replay and no broadcast types escape this boundary.
pub struct Subscription {
    receiver: broadcast::Receiver<Delivery>,
    interrupted: Arc<AtomicBool>,
    ready: bool,
    closed: bool,
}

impl Subscription {
    pub(super) fn has_started(&self) -> bool {
        self.ready
    }
    pub(super) fn is_terminated(&self) -> bool {
        self.interrupted.load(Ordering::Acquire) || self.receiver.len() > CAPACITY
    }

    /// Complete ready/change frames. None is terminal, including any delivery lag.
    pub async fn next_frame(&mut self) -> Option<Bytes> {
        if self.closed || self.is_terminated() {
            self.closed = true;
            return None;
        }
        if !self.ready {
            self.ready = true;
            return Some(Bytes::from_static(b"event: ready\ndata: {}\n\n"));
        }
        match self.receiver.recv().await {
            Ok(Delivery::Change(frame)) => Some(frame),
            Ok(Delivery::Interrupted) | Err(_) => {
                self.closed = true;
                None
            }
        }
    }
}
