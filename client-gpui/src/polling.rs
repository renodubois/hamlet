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
mod tests {
    use super::*;
    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn in_flight_focus_return_and_recovery_keep_one_catch_up_but_failure_backs_off() {
        let mut poll = Polling::default();
        poll.focus(true, secs(0));
        poll.started(Resource::History);
        poll.focus(false, secs(1));
        poll.focus(true, secs(2));
        assert!(poll.due(Resource::History, secs(2)));
        poll.completed(Resource::History, true, secs(4));
        assert!(poll.due(Resource::History, secs(4)));
        poll.started(Resource::History);
        poll.completed(Resource::History, true, secs(5));
        assert!(!poll.due(Resource::History, secs(7)));
        assert!(poll.due(Resource::History, secs(8)));

        poll.started(Resource::Channels);
        poll.recover_other(Resource::Channels, secs(9));
        poll.completed(Resource::History, true, secs(10));
        assert!(poll.due(Resource::History, secs(10)));
        poll.started(Resource::History);
        poll.focus(false, secs(11));
        poll.focus(true, secs(12));
        poll.completed(Resource::History, false, secs(13));
        assert!(!poll.due(Resource::History, secs(13)));
        assert!(poll.due(Resource::History, secs(16)));
    }

    #[test]
    fn failed_reads_while_unfocused_report_paused_not_retrying() {
        let mut poll = Polling::default();
        poll.focus(true, secs(0));
        poll.started(Resource::History);
        poll.completed(Resource::History, false, secs(1));
        poll.focus(false, secs(2));
        assert!(poll.status().contains("conversation refresh failed"));
        assert!(poll.status().contains("Updates paused"));
        assert!(!poll.status().contains("Retrying"));
        poll.focus(true, secs(3));
        assert!(poll.status().contains("Retrying"));
    }

    #[test]
    fn manual_recovery_can_wake_the_other_resource_without_replaying_pending_reads() {
        let mut poll = Polling::default();
        poll.focus(true, secs(0));
        poll.started(Resource::History);
        poll.started(Resource::Channels);
        poll.completed(Resource::History, false, secs(0));
        poll.completed(Resource::History, false, secs(3));
        poll.completed(Resource::Channels, false, secs(0));
        assert!(!poll.due(Resource::History, secs(5)));
        // A deliberate manual read is permitted despite backoff; its success reconciles
        // the other resource without changing the independent history interval.
        poll.started(Resource::Channels);
        assert!(poll.completed(Resource::Channels, true, secs(5)));
        poll.recover_other(Resource::Channels, secs(5));
        assert!(poll.due(Resource::History, secs(5)));
        poll.started(Resource::History);
        assert!(!poll.due(Resource::History, secs(100)));
        assert!(poll.completed(Resource::History, true, secs(100)));
        assert!(!poll.due(Resource::History, secs(102)));
        assert!(poll.due(Resource::History, secs(103)));
        poll.reset_history(secs(104)); // selection changes invalidate the old deadline
        assert!(poll.due(Resource::History, secs(104)));
        poll = Polling::default(); // logout, server change or protected rejection
        assert!(!poll.due(Resource::History, secs(1000)));
        assert!(!poll.due(Resource::Channels, secs(1000)));
    }

    #[test]
    fn focus_intervals_and_independent_backoff_use_supplied_time() {
        let mut poll = Polling::default();
        assert!(!poll.due(Resource::History, secs(100)));
        assert!(poll.focus(true, secs(0)));
        assert!(poll.due(Resource::History, secs(0)));
        poll.started(Resource::History);
        poll.started(Resource::Channels);
        poll.completed(Resource::History, true, secs(0));
        poll.completed(Resource::Channels, true, secs(0));
        assert!(!poll.due(Resource::History, secs(2)));
        assert!(poll.due(Resource::History, secs(3)));
        assert!(!poll.due(Resource::Channels, secs(3)));
        assert!(poll.due(Resource::Channels, secs(15)));
        poll.started(Resource::History);
        assert!(!poll.completed(Resource::History, false, secs(3)));
        assert!(poll.status().contains("conversation refresh failed"));
        assert!(poll.due(Resource::History, secs(6)));
        poll.started(Resource::History);
        poll.completed(Resource::History, false, secs(6));
        assert!(!poll.due(Resource::History, secs(11)));
        assert!(poll.due(Resource::History, secs(12)));
        poll.focus(false, secs(20));
        assert!(!poll.due(Resource::Channels, secs(100)));
        assert!(poll.focus(true, secs(101)));
        assert!(poll.due(Resource::Channels, secs(101)));
        assert!(poll.completed(Resource::History, true, secs(101)));
        assert!(poll.status().contains("Connected"));
    }
}
