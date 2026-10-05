use super::*;
use crate::api::Channel;
use std::time::Duration;

fn channel(id: &str) -> LiveEvent {
    LiveEvent::ChannelCreated(Channel {
        id: id.into(),
        name: format!("Room {id}"),
    })
}

#[test]
fn readiness_then_current_baseline_are_required_before_live_delivery() {
    let mut live = LiveUpdates::new(7);
    let attempt = live.attempt();
    assert_eq!(live.phase(), Phase::Connecting);
    assert_eq!(live.finish_baseline(attempt), None);
    assert!(!live.channels_loaded(attempt, Some("1")));
    assert!(live.ready(attempt));
    assert_eq!(live.phase(), Phase::LoadingBaseline);
    assert!(
        !live.ready(attempt),
        "readiness cannot restart baseline reads"
    );
    assert!(live.channels_loaded(attempt, Some("1")));
    assert_eq!(live.finish_baseline(attempt), None);
    assert!(live.history_loaded(attempt, "1"));
    assert_eq!(live.finish_baseline(attempt), Some(vec![]));
    assert_eq!(live.phase(), Phase::LoadingBaseline);
    assert!(live.synchronized(attempt));
    assert_eq!(live.phase(), Phase::Live);
    assert_eq!(live.finish_baseline(attempt), None);
}

#[test]
fn creations_wait_for_baseline_then_deliver_directly_with_a_separate_recovery_bound() {
    let mut live = LiveUpdates::new(7);
    let attempt = live.attempt();
    assert_eq!(
        live.creation(attempt, channel("before ready")),
        Delivery::Recover
    );
    assert!(live.ready(attempt));
    // Both before and during the HTTP read: retain all entities for shared ID merging.
    assert_eq!(live.creation(attempt, channel("1")), Delivery::Buffered);
    live.channels_loaded(attempt, Some("1"));
    assert_eq!(live.creation(attempt, channel("2")), Delivery::Buffered);
    live.history_loaded(attempt, "1");
    assert_eq!(
        live.finish_baseline(attempt),
        Some(vec![channel("1"), channel("2")])
    );
    assert!(live.synchronized(attempt));
    assert_eq!(
        live.creation(attempt, channel("3")),
        Delivery::Apply(channel("3"))
    );

    let mut recovering = LiveUpdates::new(8);
    let attempt = recovering.attempt();
    recovering.ready(attempt);
    for index in 0..RECOVERY_CAPACITY {
        assert_eq!(
            recovering.creation(attempt, channel(&index.to_string())),
            Delivery::Buffered
        );
    }
    assert_eq!(
        recovering.creation(attempt, channel("overflow")),
        Delivery::Recover
    );
    // Overflow is sticky: a caller must never finish this partial baseline as synchronized.
    recovering.channels_loaded(attempt, None);
    assert_eq!(recovering.finish_baseline(attempt), None);
    assert_eq!(
        recovering.creation(attempt, channel("later")),
        Delivery::Recover
    );
}

#[test]
fn navigation_retargets_only_history_and_empty_channels_need_no_history() {
    let mut live = LiveUpdates::new(7);
    let attempt = live.attempt();
    live.ready(attempt);
    live.channels_loaded(attempt, Some("1"));
    assert!(live.retarget(attempt, Some("2")));
    assert!(!live.history_loaded(attempt, "1"));
    assert_eq!(live.finish_baseline(attempt), None);
    assert_eq!(
        live.attempt(),
        attempt,
        "navigation does not replace the stream"
    );
    assert!(
        !live.channels_loaded(attempt, Some("1")),
        "completed channels cannot restart"
    );
    assert!(live.history_loaded(attempt, "2"));
    assert!(
        !live.retarget(attempt, Some("2")),
        "same selection keeps its completed read"
    );
    assert_eq!(live.finish_baseline(attempt), Some(vec![]));
    assert!(live.synchronized(attempt));
    assert!(
        !live.retarget(attempt, Some("1")),
        "healthy navigation has no recovery policy"
    );

    let mut empty = LiveUpdates::new(8);
    let attempt = empty.attempt();
    empty.ready(attempt);
    empty.channels_loaded(attempt, None);
    assert_eq!(empty.finish_baseline(attempt), Some(vec![]));
    assert!(empty.synchronized(attempt));
    assert_eq!(empty.phase(), Phase::Live);
}

#[test]
fn one_capped_jittered_backoff_resets_only_after_reconciliation_not_readiness() {
    let mut live = LiveUpdates::new(7);
    let mut now = Duration::ZERO;
    // Full jitter sample chooses the upper endpoint: 1, 2, 4, 8, 16, 30, 30 seconds.
    for delay in [1, 2, 4, 8, 16, 30, 30] {
        let attempt = live.attempt();
        live.ready(attempt);
        live.channels_loaded(attempt, None);
        live.finish_baseline(attempt).unwrap();
        assert_eq!(
            live.phase(),
            Phase::LoadingBaseline,
            "merge has not succeeded yet"
        );
        assert!(live.fail(attempt, now, u32::MAX));
        assert_eq!(live.phase(), Phase::Retrying);
        assert!(
            !live.fail(attempt, now, u32::MAX),
            "duplicate terminal cannot extend retry"
        );
        assert_eq!(
            live.retry(now + Duration::from_secs(delay) - Duration::from_nanos(1)),
            None
        );
        now += Duration::from_secs(delay);
        let next = live.retry(now).unwrap();
        assert_ne!(next, attempt);
        assert_eq!(live.retry(now), None, "one stream per attempt");
        assert_eq!(live.phase(), Phase::Connecting);
    }
    let attempt = live.attempt();
    live.ready(attempt);
    live.channels_loaded(attempt, None);
    live.finish_baseline(attempt).unwrap();
    assert!(live.synchronized(attempt));
    // Lowest jitter endpoint is half of the reset one-second base.
    live.fail(attempt, now, 0);
    assert_eq!(live.retry(now + Duration::from_millis(499)), None);
    assert!(live.retry(now + Duration::from_millis(500)).is_some());
}

#[test]
fn connection_notice_persists_until_reconciled_and_never_offers_a_retry_action() {
    let mut live = LiveUpdates::new(7);
    let attempt = live.attempt();
    assert_eq!(live.status(false), "Connecting…");
    assert_eq!(
        live.status(true),
        "Reconnecting… messages may be out of date"
    );
    live.ready(attempt);
    assert_eq!(live.status(false), "Connecting…");
    live.channels_loaded(attempt, None);
    live.finish_baseline(attempt).unwrap();
    assert_eq!(
        live.status(true),
        "Reconnecting… messages may be out of date"
    );
    live.synchronized(attempt);
    assert_eq!(live.status(true), "");
    live.fail(attempt, Duration::ZERO, 0);
    // Even an empty synchronized conversation is reconnecting after delivery loss.
    assert_eq!(
        live.status(false),
        "Reconnecting… messages may be out of date"
    );
    let attempt = live.retry(Duration::from_secs(1)).unwrap();
    live.ready(attempt);
    assert_eq!(
        live.status(true),
        "Reconnecting… messages may be out of date"
    );
    live.close();
    assert_eq!(live.status(true), "");
}

#[test]
fn failed_attempts_other_sessions_and_closed_lifetimes_cannot_deliver_or_complete() {
    let mut live = LiveUpdates::new(7);
    let obsolete = live.attempt();
    live.ready(obsolete);
    live.creation(obsolete, channel("lost"));
    live.fail(obsolete, Duration::ZERO, 0);
    assert_eq!(live.creation(obsolete, channel("late")), Delivery::Discard);
    let current = live.retry(Duration::from_secs(1)).unwrap();
    let other_session = LiveUpdates::new(8).attempt();
    for stale in [obsolete, other_session] {
        assert!(!live.ready(stale));
        assert!(!live.channels_loaded(stale, Some("1")));
        assert!(!live.history_loaded(stale, "1"));
        assert!(!live.retarget(stale, Some("2")));
        assert_eq!(live.creation(stale, channel("late")), Delivery::Discard);
        assert!(!live.fail(stale, Duration::from_secs(1), 0));
        assert_eq!(live.finish_baseline(stale), None);
        assert!(!live.synchronized(stale));
    }
    live.ready(current);
    live.channels_loaded(current, None);
    assert_eq!(
        live.finish_baseline(current),
        Some(vec![]),
        "old buffer was discarded"
    );
    assert!(live.synchronized(current));
    live.close();
    assert_eq!(live.phase(), Phase::Closed);
    assert!(!live.ready(current));
    assert!(!live.fail(current, Duration::from_secs(2), 0));
    assert_eq!(
        live.creation(current, channel("after close")),
        Delivery::Discard
    );
    assert_eq!(live.retry(Duration::MAX), None);
    assert!(!live.synchronized(current));
}
