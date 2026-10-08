//! Bounded executor deliveries with a separate, prioritized terminal lane.
//! Stream producers must use `try_send`: overflow ends the attempt through `send_terminal`.
//! Ordinary HTTP completions may wait for capacity; they are not disposable stream events.

use super::ChatUpdate;
#[cfg(test)]
use async_channel::TryRecvError;
use async_channel::{RecvError, SendError, TrySendError};

const DELIVERY_CAPACITY: usize = 256;

#[derive(Clone)]
pub(super) struct Sender {
    ordinary: async_channel::Sender<ChatUpdate>,
    terminal: async_channel::Sender<ChatUpdate>,
}

/// Hosts consume opaque updates; terminal results never wait behind the event backlog.
#[derive(Clone)]
pub(crate) struct Updates {
    ordinary: async_channel::Receiver<ChatUpdate>,
    terminal: async_channel::Receiver<ChatUpdate>,
}

pub(super) fn channel() -> (Sender, Updates) {
    let (ordinary_send, ordinary) = async_channel::bounded(DELIVERY_CAPACITY);
    let (terminal_send, terminal) = async_channel::bounded(1);
    (
        Sender {
            ordinary: ordinary_send,
            terminal: terminal_send,
        },
        Updates { ordinary, terminal },
    )
}

impl Sender {
    pub async fn send(&self, update: ChatUpdate) -> Result<(), SendError<ChatUpdate>> {
        self.ordinary.send(update).await
    }

    pub fn try_send(&self, update: ChatUpdate) -> Result<(), TrySendError<()>> {
        self.ordinary.try_send(update).map_err(|error| match error {
            TrySendError::Full(_) => TrySendError::Full(()),
            TrySendError::Closed(_) => TrySendError::Closed(()),
        })
    }

    // Exactly one terminal result per owned stream attempt. Cancellation must abort obsolete
    // producers; this lane deliberately bounds even terminal delivery rather than accumulating it.
    pub async fn send_terminal(&self, update: ChatUpdate) -> Result<(), SendError<ChatUpdate>> {
        self.terminal.send(update).await
    }

    pub fn close(&self) {
        self.ordinary.close();
        self.terminal.close();
    }
}

impl Updates {
    pub async fn recv(&self) -> Result<ChatUpdate, RecvError> {
        tokio::select! {
            biased;
            terminal = self.terminal.recv() => match terminal {
                Ok(update) => Ok(update),
                Err(_) => self.ordinary.recv().await,
            },
            ordinary = self.ordinary.recv() => match ordinary {
                Ok(update) => Ok(update),
                Err(_) => self.terminal.recv().await,
            },
        }
    }

    #[cfg(test)]
    pub fn try_recv(&self) -> Result<ChatUpdate, TryRecvError> {
        self.terminal
            .try_recv()
            .or_else(|_| self.ordinary.try_recv())
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.ordinary.len() + self.terminal.len()
    }
}

#[cfg(test)]
#[path = "tests/delivery.rs"]
mod tests;
