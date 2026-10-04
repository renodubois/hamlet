use actix_web::web::Bytes;
use hamlet_protocol::Event;
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
    sender: broadcast::Sender<Bytes>,
}

impl Default for EventHub {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(CAPACITY);
        Self { sender }
    }
}

impl EventHub {
    /// Synchronous, best-effort enqueue. No subscribers is an ordinary outcome.
    pub fn notify(&self, event: Event) {
        // Event consists only of infallibly serializable wire values. Serialize once,
        // then Bytes clones share immutable storage across every receiver.
        let json = serde_json::to_string(&event).expect("event wire serialization");
        let _ = self
            .sender
            .send(Bytes::from(format!("event: change\ndata: {json}\n\n")));
    }

    pub fn subscribe(&self) -> Subscription {
        Subscription {
            receiver: self.sender.subscribe(),
            ready: false,
            closed: false,
        }
    }
}

/// A fresh subscription; no replay and no broadcast types escape this boundary.
pub struct Subscription {
    receiver: broadcast::Receiver<Bytes>,
    ready: bool,
    closed: bool,
}

impl Subscription {
    pub(super) fn has_started(&self) -> bool {
        self.ready
    }
    pub(super) fn is_lagged(&self) -> bool {
        self.receiver.len() > CAPACITY
    }

    /// Complete ready/change frames. None is terminal, including any delivery lag.
    pub async fn next_frame(&mut self) -> Option<Bytes> {
        if self.closed {
            return None;
        }
        if !self.ready {
            self.ready = true;
            if self.is_lagged() {
                self.closed = true;
                return None;
            }
            return Some(Bytes::from_static(b"event: ready\ndata: {}\n\n"));
        }
        match self.receiver.recv().await {
            Ok(frame) => Some(frame),
            Err(_) => {
                self.closed = true;
                None
            }
        }
    }
}
