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
