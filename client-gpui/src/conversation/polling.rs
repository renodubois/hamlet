//! Focus-aware read schedule. All time is supplied by the caller (monotonic since launch).
use std::time::Duration;

pub const HISTORY_INTERVAL: Duration = Duration::from_secs(3);
pub const CHANNEL_INTERVAL: Duration = Duration::from_secs(15);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    History,
    Channels,
}

#[derive(Clone, Copy, Default)]
struct Slot {
    due: Option<Duration>,
    failures: u32,
}

#[derive(Default)]
pub struct Polling {
    focused: bool,
    history: Slot,
    channels: Slot,
}

impl Polling {
    fn slot(&self, resource: Resource) -> Slot {
        match resource {
            Resource::History => self.history,
            Resource::Channels => self.channels,
        }
    }

    fn slot_mut(&mut self, resource: Resource) -> &mut Slot {
        match resource {
            Resource::History => &mut self.history,
            Resource::Channels => &mut self.channels,
        }
    }

    fn interval(resource: Resource) -> Duration {
        match resource {
            Resource::History => HISTORY_INTERVAL,
            Resource::Channels => CHANNEL_INTERVAL,
        }
    }

    /// Returns true only on a transition to foreground, requesting immediate reconciliation.
    pub fn focus(&mut self, focused: bool, now: Duration) -> bool {
        if self.focused == focused {
            return false;
        }
        self.focused = focused;
        if focused {
            self.history.due = Some(now);
            self.channels.due = Some(now);
        }
        focused
    }

    pub fn due(&self, resource: Resource, now: Duration) -> bool {
        self.focused && self.slot(resource).due.is_some_and(|due| now >= due)
    }

    /// A read is underway; another timer tick must not start a second one.
    pub fn started(&mut self, resource: Resource) {
        self.slot_mut(resource).due = None;
    }

    /// Only accepted, current read completions may change scheduling or status.
    /// Returns true when a failed resource recovers; the other resource may need reconciliation.
    pub fn completed(&mut self, resource: Resource, succeeded: bool, now: Duration) -> bool {
        let slot = self.slot_mut(resource);
        let recovered = succeeded && slot.failures > 0;
        if succeeded {
            slot.failures = 0;
            // Focus return (or recovery of the other resource) can request a fresh read
            // while the pre-focus read is still underway. Do not erase that request.
            slot.due = Some(slot.due.unwrap_or(now + Self::interval(resource)));
        } else {
            slot.failures = slot.failures.saturating_add(1);
            let exponent = slot.failures.saturating_sub(1).min(5);
            let delay = Self::interval(resource)
                .saturating_mul(1 << exponent)
                .min(MAX_BACKOFF);
            slot.due = Some(now + delay);
        }
        recovered
    }

    pub fn recover_other(&mut self, resource: Resource, now: Duration) {
        let other = match resource {
            Resource::History => Resource::Channels,
            Resource::Channels => Resource::History,
        };
        self.slot_mut(other).due = Some(now);
    }

    pub fn reset_history(&mut self, now: Duration) {
        self.history = Slot {
            due: Some(now),
            failures: 0,
        };
    }

    pub fn status(&self) -> &'static str {
        if !self.focused {
            return match (self.channels.failures > 0, self.history.failures > 0) {
                (true, true) => {
                    "Connection trouble: channels and conversation unavailable. Updates paused while window is unfocused; loaded messages and drafts are safe in memory."
                }
                (true, false) => {
                    "Connection trouble: channel refresh failed. Updates paused while window is unfocused; loaded channels remain visible."
                }
                (false, true) => {
                    "Connection trouble: conversation refresh failed. Updates paused while window is unfocused; loaded messages and drafts remain visible."
                }
                (false, false) => "Updates paused while window is unfocused.",
            };
        }
        match (self.channels.failures > 0, self.history.failures > 0) {
            (true, true) => {
                "Connection trouble: channels and conversation unavailable. Retrying reads; loaded messages and drafts are safe in memory."
            }
            (true, false) => {
                "Connection trouble: channel refresh failed. Retrying reads; loaded channels remain visible."
            }
            (false, true) => {
                "Connection trouble: conversation refresh failed. Retrying reads; loaded messages and drafts remain visible."
            }
            (false, false) => "Connected. Checking for new messages and channels.",
        }
    }
}

#[cfg(test)]
#[path = "polling_tests.rs"]
mod tests;
