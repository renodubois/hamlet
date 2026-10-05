//! Connection readiness and fixed-delay reconnect state for the conversation's SSE stream.
//! Reconnection does not trigger reads or replay writes; obsolete attempts are ignored.
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Attempt {
    generation: u64,
    serial: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Connecting,
    Connected,
    WaitingToRetry,
    Closed,
}

pub(super) struct LiveUpdates {
    attempt: Attempt,
    phase: Phase,
    retry_at: Duration,
    disconnected: bool,
}

impl LiveUpdates {
    pub fn new(generation: u64) -> Self {
        Self {
            attempt: Attempt {
                generation,
                serial: 1,
            },
            phase: Phase::Connecting,
            retry_at: Duration::ZERO,
            disconnected: false,
        }
    }

    pub fn attempt(&self) -> Attempt {
        self.attempt
    }

    pub fn status(&self) -> &'static str {
        if matches!(self.phase, Phase::Connected | Phase::Closed) {
            ""
        } else if self.disconnected {
            "Live updates disconnected — reconnecting."
        } else {
            "Connecting…"
        }
    }

    pub fn accepts(&self, attempt: Attempt) -> bool {
        attempt == self.attempt && matches!(self.phase, Phase::Connecting | Phase::Connected)
    }

    pub fn close(&mut self) {
        self.phase = Phase::Closed;
    }

    pub fn ready(&mut self, attempt: Attempt) {
        if attempt == self.attempt && self.phase == Phase::Connecting {
            self.phase = Phase::Connected;
        }
    }

    pub fn fail(&mut self, attempt: Attempt, now: Duration) -> bool {
        if !self.accepts(attempt) {
            return false;
        }
        self.retry_at = now.saturating_add(Duration::from_secs(3));
        self.phase = Phase::WaitingToRetry;
        self.disconnected = true;
        true
    }

    pub fn retry(&mut self, now: Duration) -> Option<Attempt> {
        if self.phase != Phase::WaitingToRetry || now < self.retry_at {
            return None;
        }
        self.attempt.serial = self.attempt.serial.wrapping_add(1);
        self.phase = Phase::Connecting;
        Some(self.attempt)
    }

    pub fn retry_at(&self) -> Option<Duration> {
        (self.phase == Phase::WaitingToRetry).then_some(self.retry_at)
    }
}
