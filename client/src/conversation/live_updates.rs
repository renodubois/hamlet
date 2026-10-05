//! Pure session-owned live synchronization policy. The coordinator executes its decisions.

use crate::api::LiveEvent;
use std::time::Duration;

// Independent of the API delivery and executor bridge bounds.
const RECOVERY_CAPACITY: usize = 256;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Delivery {
    Buffered,
    Apply(LiveEvent),
    Discard,
    Recover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Attempt {
    generation: u64,
    serial: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Connecting,
    LoadingBaseline,
    Live,
    Retrying,
    Closed,
}

pub(super) struct LiveUpdates {
    attempt: Attempt,
    phase: Phase,
    channels_loaded: bool,
    selected: Option<String>,
    history_loaded: bool,
    buffered: Vec<LiveEvent>,
    overflowed: bool,
    failures: u32,
    retry_at: Duration,
    was_synchronized: bool,
}

impl LiveUpdates {
    pub fn new(generation: u64) -> Self {
        Self {
            attempt: Attempt {
                generation,
                serial: 1,
            },
            phase: Phase::Connecting,
            channels_loaded: false,
            selected: None,
            history_loaded: false,
            buffered: Vec::new(),
            overflowed: false,
            failures: 0,
            retry_at: Duration::ZERO,
            was_synchronized: false,
        }
    }

    pub fn attempt(&self) -> Attempt {
        self.attempt
    }

    #[cfg(test)]
    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn can_read_history(&self) -> bool {
        self.phase == Phase::Live || (self.phase == Phase::LoadingBaseline && self.channels_loaded)
    }

    pub fn status(&self, retained_content: bool) -> &'static str {
        if matches!(self.phase, Phase::Live | Phase::Closed) {
            ""
        } else if retained_content || self.was_synchronized {
            "Reconnecting… messages may be out of date"
        } else {
            "Connecting…"
        }
    }

    pub fn accepts(&self, attempt: Attempt) -> bool {
        attempt == self.attempt && !matches!(self.phase, Phase::Retrying | Phase::Closed)
    }

    pub fn close(&mut self) {
        self.phase = Phase::Closed;
        self.buffered.clear();
    }

    pub fn ready(&mut self, attempt: Attempt) -> bool {
        if attempt != self.attempt || self.phase != Phase::Connecting {
            return false;
        }
        self.phase = Phase::LoadingBaseline;
        true
    }

    /// Called only after the coordinator accepts the channel request identity and snapshot.
    pub fn channels_loaded(&mut self, attempt: Attempt, selected: Option<&str>) -> bool {
        if attempt != self.attempt || self.phase != Phase::LoadingBaseline || self.channels_loaded {
            return false;
        }
        self.channels_loaded = true;
        self.selected = selected.map(str::to_owned);
        self.history_loaded = false;
        true
    }

    /// Selection can change freely; successful channels belong to the attempt, not selection.
    /// The coordinator cancels/invalidates the old read and dispatches the new selected read.
    pub fn retarget(&mut self, attempt: Attempt, selected: Option<&str>) -> bool {
        if attempt != self.attempt
            || self.phase != Phase::LoadingBaseline
            || !self.channels_loaded
            || self.selected.as_deref() == selected
        {
            return false;
        }
        self.selected = selected.map(str::to_owned);
        self.history_loaded = false;
        true
    }

    /// Request identity remains with conversation state; the policy also checks its target.
    pub fn history_loaded(&mut self, attempt: Attempt, selected: &str) -> bool {
        if attempt != self.attempt
            || self.phase != Phase::LoadingBaseline
            || !self.channels_loaded
            || self.selected.as_deref() != Some(selected)
        {
            return false;
        }
        self.history_loaded = true;
        true
    }

    pub fn creation(&mut self, attempt: Attempt, event: LiveEvent) -> Delivery {
        if !self.accepts(attempt) {
            return Delivery::Discard;
        }
        if self.overflowed || matches!(event, LiveEvent::Ready) {
            return Delivery::Recover;
        }
        match self.phase {
            Phase::Retrying | Phase::Closed => Delivery::Discard,
            Phase::Connecting => Delivery::Recover,
            Phase::Live => Delivery::Apply(event),
            Phase::LoadingBaseline => {
                if self.buffered.len() == RECOVERY_CAPACITY {
                    self.overflowed = true;
                    self.buffered.clear();
                    Delivery::Recover
                } else {
                    self.buffered.push(event);
                    Delivery::Buffered
                }
            }
        }
    }

    fn baseline_complete(&self, attempt: Attempt) -> bool {
        attempt == self.attempt
            && self.phase == Phase::LoadingBaseline
            && self.channels_loaded
            && !self.overflowed
            && (self.selected.is_none() || self.history_loaded)
    }

    /// Drain for synchronous shared-entity reconciliation; this is not yet synchronization.
    pub fn finish_baseline(&mut self, attempt: Attempt) -> Option<Vec<LiveEvent>> {
        self.baseline_complete(attempt)
            .then(|| std::mem::take(&mut self.buffered))
    }

    /// Call only after shared entity merging succeeded, without yielding between drain and ack.
    pub fn synchronized(&mut self, attempt: Attempt) -> bool {
        if !self.baseline_complete(attempt) || !self.buffered.is_empty() {
            return false;
        }
        self.phase = Phase::Live;
        self.was_synchronized = true;
        self.failures = 0;
        true
    }

    /// All transport, required-read and reconciliation failures enter this same policy.
    /// Randomness is supplied by the executor boundary; full-range samples select equal jitter
    /// in [base / 2, base], with exponential base 1s..30s and no overflow at long outages.
    pub fn fail(&mut self, attempt: Attempt, now: Duration, jitter: u32) -> bool {
        if !self.accepts(attempt) {
            return false;
        }
        let base_ms = (1_000u64 << self.failures.min(5)).min(30_000);
        let delay_ms = base_ms / 2 + (base_ms / 2 * u64::from(jitter) / u64::from(u32::MAX));
        self.failures = self.failures.saturating_add(1);
        self.retry_at = now.saturating_add(Duration::from_millis(delay_ms));
        self.phase = Phase::Retrying;
        self.buffered.clear();
        self.channels_loaded = false;
        self.history_loaded = false;
        true
    }

    pub fn retry(&mut self, now: Duration) -> Option<Attempt> {
        if self.phase != Phase::Retrying || now < self.retry_at {
            return None;
        }
        self.attempt.serial = self.attempt.serial.wrapping_add(1);
        self.phase = Phase::Connecting;
        self.selected = None;
        self.overflowed = false;
        Some(self.attempt)
    }
}

#[cfg(test)]
#[path = "tests/live_updates.rs"]
mod tests;
