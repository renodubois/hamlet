use super::*;

fn session() -> Identity {
    Identity {
        generation: Some(1),
        expires_at: 100,
        rejected: None,
    }
}
fn channel(id: &str, name: &str) -> Channel {
    Channel {
        id: id.into(),
        name: name.into(),
    }
}
fn message(id: &str, at: &str) -> Message {
    Message {
        id: id.into(),
        channel_id: "1".into(),
        author_id: "1".into(),
        author_name: "Ada".into(),
        text: "same text".into(),
        created_at: at.into(),
    }
}
fn page(items: Vec<Message>, cursor: Option<&str>) -> Page {
    Page {
        items,
        next_cursor: cursor.map(str::to_owned),
    }
}
fn loading() -> (Conversation, Identity, ReadRequest) {
    let mut state = Conversation::default();
    let mut session = session();
    let read = state.start(&session).unwrap();
    let history = state
        .complete_channels(
            &mut session,
            &read,
            Ok(vec![channel("1", "General"), channel("2", "Other")]),
        )
        .unwrap();
    (state, session, history)
}
fn ids(state: &Conversation) -> Vec<&str> {
    match state.history.get("1") {
        Some(Load::Ready(items)) => items.iter().map(|m| m.id.as_str()).collect(),
        _ => vec![],
    }
}

#[test]
fn healthy_confirmation_is_not_reinserted_outside_a_later_newest_page() {
    let (mut state, mut session, read) = loading();
    state.complete_history(&mut session, &read, Ok(page(vec![], None)));
    state.set_draft("1", "accepted".into());
    let send = state.send(&session).unwrap();
    assert_eq!(
        state.complete_send(
            &mut session,
            &send,
            Ok(message("10", "2026-01-01T00:00:00Z")),
            0,
        ),
        SendOutcome::Confirmed
    );
    assert_eq!(ids(&state), ["10"]);

    state.reset_for_recovery();
    let newest = state.select(&session, "1").unwrap();
    state.complete_history(
        &mut session,
        &newest,
        Ok(page(
            vec![message("100", "2026-01-02T00:00:00Z")],
            Some("older"),
        )),
    );
    assert_eq!(
        ids(&state),
        ["100"],
        "recovery loads newest, not old confirmed fragments"
    );
    assert_eq!(state.draft("1"), "");
    assert!(!state.uncertain.contains("1"));
    assert!(!state.send_pending.contains_key("1"));
    assert!(!state.send_feedback.contains_key("1"));
}

#[test]
fn outage_confirmation_survives_repeated_resets_only_until_its_baseline_reconciles() {
    let (mut state, mut session, read) = loading();
    state.complete_history(&mut session, &read, Ok(page(vec![], None)));
    state.reset_for_recovery();
    state.set_draft("1", "outage write".into());
    let send = state.send(&session).unwrap();
    assert_eq!(
        state.complete_send(
            &mut session,
            &send,
            Ok(message("10", "2026-01-01T00:00:00Z")),
            0,
        ),
        SendOutcome::Confirmed
    );
    state.set_draft("1", "next draft".into());
    for _ in 0..2 {
        let obsolete = state.select(&session, "1").unwrap();
        state.reset_for_recovery();
        state.complete_history(&mut session, &obsolete, Ok(page(vec![], None)));
    }
    let failed = state.select(&session, "1").unwrap();
    state.complete_history(&mut session, &failed, Err(ApiError::Unavailable));
    state.reset_for_recovery();
    let baseline = state.select(&session, "1").unwrap();
    state.complete_history(&mut session, &baseline, Ok(page(vec![], None)));
    assert_eq!(
        ids(&state),
        ["10"],
        "unresolved confirmation survives attempt loss"
    );
    assert_eq!(state.draft("1"), "next draft");
    assert!(!state.uncertain.contains("1"));

    state.reset_for_recovery();
    let later = state.select(&session, "1").unwrap();
    state.complete_history(
        &mut session,
        &later,
        Ok(page(
            vec![message("100", "2026-01-02T00:00:00Z")],
            Some("older"),
        )),
    );
    assert_eq!(
        ids(&state),
        ["100"],
        "reconciled confirmations are not a second history cache"
    );
    assert_eq!(state.draft("1"), "next draft");
    assert_eq!(
        state.complete_send(&mut session, &send, Err(ApiError::Unavailable), 0),
        SendOutcome::Stale
    );
    assert!(!state.uncertain.contains("1"));
}

#[test]
fn inactive_confirmations_retire_after_direct_merge_or_their_own_retargeted_baseline() {
    for loaded in [false, true] {
        let (mut state, mut session, original) = loading();
        if loaded {
            state.complete_history(&mut session, &original, Ok(page(vec![], None)));
        }
        state.set_draft("1", "accepted".into());
        let send = state.send(&session).unwrap();
        let other = state.select(&session, "2").unwrap();
        assert_eq!(
            state.complete_send(
                &mut session,
                &send,
                Ok(message("10", "2026-01-01T00:00:00Z")),
                0,
            ),
            SendOutcome::Confirmed
        );
        state.complete_history(&mut session, &original, Ok(page(vec![], None)));
        state.complete_history(&mut session, &other, Ok(page(vec![], None)));
        assert_eq!(state.selected.as_deref(), Some("2"));
        assert_eq!(state.history.get("2"), Some(&Load::Ready(vec![])));
        if loaded {
            assert_eq!(
                ids(&state),
                ["10"],
                "inactive loaded history accepts direct merge"
            );
        } else {
            assert!(
                !state.history.contains_key("1"),
                "confirmation must not allocate an inactive cache"
            );
            state.reset_for_recovery();
            let canceled = state.select(&session, "1").unwrap();
            let other = state.select(&session, "2").unwrap();
            state.complete_history(&mut session, &canceled, Ok(page(vec![], None)));
            state.complete_history(&mut session, &other, Ok(page(vec![], None)));
            let current = state.select(&session, "1").unwrap();
            state.complete_history(&mut session, &current, Ok(page(vec![], None)));
            assert_eq!(
                ids(&state),
                ["10"],
                "only the originating channel's current baseline reconciles the write"
            );
        }
        state.select(&session, "2");
        state.reset_for_recovery();
        let current = state.select(&session, "1").unwrap();
        state.complete_history(
            &mut session,
            &current,
            Ok(page(
                vec![message("100", "2026-01-02T00:00:00Z")],
                Some("older"),
            )),
        );
        assert_eq!(
            ids(&state),
            ["100"],
            "inactive confirmations must not become permanent caches"
        );
        assert_eq!(state.draft("1"), "");
        assert!(!state.uncertain.contains("1"));
    }
}

#[test]
fn recovery_navigation_discards_the_previous_selections_display_only_rows() {
    let (mut state, mut session, read) = loading();
    state.complete_history(
        &mut session,
        &read,
        Ok(page(vec![message("1", "2026-01-01T00:00:00Z")], None)),
    );
    state.reset_for_recovery();
    assert!(matches!(
        state.history_for_display("1"),
        Some(Load::Ready(_))
    ));
    state.select(&session, "2");
    assert!(
        state.history_for_display("1").is_none(),
        "inactive retained history is not a cache"
    );
    state.select(&session, "1");
    assert!(matches!(
        state.history_for_display("1"),
        Some(Load::Loading)
    ));
}

#[test]
fn recovery_retains_selected_display_until_replacement_without_retaining_server_cache() {
    let (mut state, mut session, read) = loading();
    let retained = message("10", "2026-01-01T00:00:00Z");
    state.complete_history(
        &mut session,
        &read,
        Ok(page(vec![retained.clone()], Some("old"))),
    );
    let other = state.select(&session, "2").unwrap();
    state.complete_history(&mut session, &other, Ok(page(vec![], None)));
    state.select(&session, "1");
    state.reset_for_recovery();
    assert!(state.history.is_empty());
    assert!(state.older.is_empty());
    assert_eq!(
        state.history_for_display("1"),
        Some(&Load::Ready(vec![retained.clone()]))
    );
    assert_eq!(
        state.history_for_display("2"),
        None,
        "inactive cache is discarded"
    );
    let obsolete = state.select(&session, "1").unwrap();
    assert_eq!(
        state.history_for_display("1"),
        Some(&Load::Ready(vec![retained.clone()]))
    );
    state.reset_for_recovery();
    state.complete_history(&mut session, &obsolete, Ok(page(vec![], None)));
    assert_eq!(
        state.history_for_display("1"),
        Some(&Load::Ready(vec![retained.clone()]))
    );
    let failed = state.select(&session, "1").unwrap();
    state.complete_history(&mut session, &failed, Err(ApiError::Unavailable));
    assert_eq!(
        state.history_for_display("1"),
        Some(&Load::Ready(vec![retained]))
    );
    state.reset_for_recovery();
    let current = state.select(&session, "1").unwrap();
    state.complete_history(&mut session, &current, Ok(page(vec![], None)));
    assert_eq!(
        state.history_for_display("1"),
        Some(&Load::Ready(vec![])),
        "authoritative empty page removes stale rows"
    );
    state.clear();
    assert_eq!(
        state.history_for_display("1"),
        None,
        "session teardown clears retained display too"
    );
}

#[test]
fn recovery_discards_retained_rows_when_authoritative_channels_are_empty() {
    let (mut state, mut session, read) = loading();
    state.complete_history(
        &mut session,
        &read,
        Ok(page(vec![message("10", "2026-01-01T00:00:00Z")], None)),
    );
    state.reset_for_recovery();
    let channels = state.refresh_channels(&session).unwrap();
    assert!(
        state
            .complete_channels(&mut session, &channels, Ok(vec![]))
            .is_none()
    );
    assert_eq!(state.selected, None);
    assert_eq!(
        state.history_for_display("1"),
        None,
        "no selected baseline will follow to clear retention"
    );
}

#[test]
fn canceled_replacing_reads_cannot_contaminate_new_selection_or_reject_its_session() {
    let (mut state, mut session, obsolete) = loading();
    state
        .merge_message(message("10", "2026-01-01T00:00:00Z"))
        .unwrap();
    let current = state.select(&session, "2").unwrap();
    state.complete_history(&mut session, &obsolete, Err(ApiError::AlreadyInvalid));
    assert_eq!(session.generation, Some(1));
    state.complete_history(&mut session, &current, Ok(page(vec![], None)));
    assert_eq!(state.history.get("2"), Some(&Load::Ready(vec![])));
    assert!(!state.history.contains_key("1"));
    let fresh = state.select(&session, "1").unwrap();
    state
        .merge_message(message("11", "2026-01-01T00:00:00Z"))
        .unwrap();
    state.complete_history(&mut session, &fresh, Ok(page(vec![], None)));
    assert_eq!(ids(&state), ["11"]);
}

#[test]
fn confirmation_survives_staging_overflow_and_reset_without_becoming_uncertain() {
    let (mut state, mut session, read) = loading();
    for index in 0..READ_STAGING_CAPACITY {
        state
            .merge_message(message(&(index + 10).to_string(), "2026-01-01T00:00:00Z"))
            .unwrap();
    }
    state.set_draft("1", "accepted".into());
    let send = state.send(&session).unwrap();
    assert_eq!(
        state.complete_send(
            &mut session,
            &send,
            Ok(message("999", "2026-01-01T00:00:00Z")),
            0
        ),
        SendOutcome::Confirmed
    );
    assert!(state.reconciliation_overflowed());
    assert_eq!(state.draft("1"), "");
    assert!(!state.uncertain.contains("1"));
    let feedback = state.send_feedback.get("1").cloned();
    state.complete_history(&mut session, &read, Ok(page(vec![], None)));
    state.reset_for_recovery();
    assert!(!state.reconciliation_overflowed());
    assert_eq!(state.send_feedback.get("1").cloned(), feedback);
    let fresh = state.select(&session, "1").unwrap();
    state.complete_history(&mut session, &fresh, Ok(page(vec![], None)));
    assert_eq!(ids(&state), ["999"]);
    assert_eq!(
        state.complete_send(&mut session, &send, Err(ApiError::Unavailable), 0),
        SendOutcome::Stale
    );
}

#[test]
fn reset_revision_changes_even_with_identical_replacement_ids_and_preserves_uncertainty() {
    let (mut state, mut session, read) = loading();
    let item = message("1", "2026-01-01T00:00:00Z");
    state.complete_history(&mut session, &read, Ok(page(vec![item.clone()], None)));
    state.set_draft("1", "same text".into());
    let send = state.send(&session).unwrap();
    state.complete_send(&mut session, &send, Err(ApiError::Unavailable), 0);
    let warning = state.send_feedback.get("1").cloned();
    for _ in 0..2 {
        let previous = state.recovery_reset_revision();
        state.reset_for_recovery();
        assert_ne!(state.recovery_reset_revision(), previous);
        let read = state.select(&session, "1").unwrap();
        state.merge_message(item.clone()).unwrap();
        state.complete_history(&mut session, &read, Ok(page(vec![item.clone()], None)));
        assert_eq!(ids(&state), ["1"]);
        assert_eq!(state.draft("1"), "same text");
        assert!(state.uncertain.contains("1"));
        assert_eq!(state.send_feedback.get("1").cloned(), warning);
    }
}

#[test]
fn confirmation_during_initial_read_survives_snapshot_and_duplicate_event() {
    let (mut state, mut session, read) = loading();
    state.set_draft("1", "same text".into());
    let send = state.send(&session).unwrap();
    state.complete_send(
        &mut session,
        &send,
        Ok(message("10", "2026-01-01T00:00:00Z")),
        0,
    );
    state.complete_history(
        &mut session,
        &read,
        Ok(page(vec![message("8", "2026-01-01T00:00:00Z")], None)),
    );
    assert_eq!(ids(&state), ["10", "8"]);
    state
        .merge_message(message("10", "2026-01-01T00:00:00Z"))
        .unwrap();
    state
        .merge_message(message("9", "2026-01-01T00:00:00Z"))
        .unwrap();
    assert_eq!(ids(&state), ["10", "9", "8"]);
    assert!(state.select(&session, "1").is_none());
    assert!(!state.send_feedback.contains_key("1"));
}

#[test]
fn recovery_reset_invalidates_server_reads_but_preserves_local_work_and_write_identities() {
    let (mut state, mut session, read) = loading();
    state.complete_history(
        &mut session,
        &read,
        Ok(page(
            vec![message("1", "2026-01-01T00:00:00Z")],
            Some("old"),
        )),
    );
    let other = state.select(&session, "2").unwrap();
    state.complete_history(&mut session, &other, Ok(page(vec![], None)));
    state.set_draft("2", "uncertain draft".into());
    let uncertain = state.send(&session).unwrap();
    state.complete_send(&mut session, &uncertain, Err(ApiError::Unavailable), 0);
    let warning = state.send_feedback.get("2").cloned();
    state.select(&session, "1");
    state.set_draft("1", "confirmed".into());
    let confirmed = state.send(&session).unwrap();
    state.complete_send(
        &mut session,
        &confirmed,
        Ok(message("2", "2026-01-01T00:00:00Z")),
        0,
    );
    state.set_draft("1", "pending draft".into());
    let pending = state.send(&session).unwrap();
    let create = state.create(&session, "Local").unwrap();
    let older = state.request_older(&session).unwrap();
    let listing = state.refresh_channels(&session).unwrap();
    let revision = state.recovery_reset_revision();

    state.reset_for_recovery();
    assert_ne!(state.recovery_reset_revision(), revision);
    assert_eq!(state.selected.as_deref(), Some("1"));
    assert!(state.history.is_empty());
    assert!(state.older.is_empty());
    assert!(state.request_older(&session).is_none());
    assert_eq!(state.draft("1"), "pending draft");
    assert_eq!(state.draft("2"), "uncertain draft");
    assert!(state.uncertain.contains("2"));
    assert_eq!(state.send_feedback.get("2").cloned(), warning);
    assert!(!state.send_feedback.contains_key("1")); // new send's outcome still pending
    assert!(state.send(&session).is_none());
    assert!(state.create(&session, "Duplicate").is_none());
    state.complete_history(&mut session, &older, Err(ApiError::AlreadyInvalid));
    state.complete_channels(&mut session, &listing, Err(ApiError::AlreadyInvalid));
    assert_eq!(session.generation, Some(1));

    let listing = state.refresh_channels(&session).unwrap();
    let newest = state
        .complete_channels(
            &mut session,
            &listing,
            Ok(vec![channel("1", "General"), channel("2", "Other")]),
        )
        .unwrap();
    assert_eq!(
        state.complete_send(
            &mut session,
            &pending,
            Ok(message("3", "2026-01-01T00:00:00Z")),
            0
        ),
        SendOutcome::Confirmed
    );
    state
        .merge_message(message("4", "2026-01-01T00:00:00Z"))
        .unwrap();
    state.complete_history(
        &mut session,
        &newest,
        Ok(page(vec![message("1", "2026-01-01T00:00:00Z")], None)),
    );
    // The healthy confirmation "2" was already authoritative before recovery;
    // only the unresolved confirmation "3" is overlaid on this newest page.
    assert_eq!(ids(&state), ["4", "3", "1"]);
    assert_eq!(state.draft("1"), "");
    assert_eq!(state.draft("2"), "uncertain draft");
    assert!(
        state
            .complete_create(&mut session, &create, Ok(channel("3", "Local")), 0)
            .0
    );
    assert_eq!(state.selected.as_deref(), Some("3"));
}

#[test]
fn recovery_replaces_cursors_and_orders_concurrent_creations() {
    let (mut state, mut session, read) = loading();
    state.complete_history(
        &mut session,
        &read,
        Ok(page(
            vec![message("50", "2026-01-01T00:00:00.5Z")],
            Some("older"),
        )),
    );
    state.reset_for_recovery();
    let refresh = state.select(&session, "1").unwrap();
    state
        .merge_message(message("60", "2026-01-01T00:00:00.6Z"))
        .unwrap();
    state.complete_history(
        &mut session,
        &refresh,
        Ok(page(
            vec![
                message("55", "2026-01-01T00:00:00.55Z"),
                message("50", "2026-01-01T00:00:00.5Z"),
            ],
            None,
        )),
    );
    assert_eq!(ids(&state), ["60", "55", "50"]);
    assert!(
        state.request_older(&session).is_none(),
        "old cursor cannot survive recovery"
    );
}

#[test]
fn older_page_merges_into_live_history_in_order_and_keeps_its_own_cursor() {
    let (mut state, mut session, read) = loading();
    state.complete_history(
        &mut session,
        &read,
        Ok(page(
            vec![message("50", "2026-01-01T00:00:00.5Z")],
            Some("old"),
        )),
    );
    let older = state.request_older(&session).unwrap();
    // Publication order need not match creation order.
    state
        .merge_message(message("10", "2026-01-01T00:00:00.1Z"))
        .unwrap();
    state
        .merge_message(message("60", "2026-01-01T00:00:00.6Z"))
        .unwrap();
    state.complete_history(
        &mut session,
        &older,
        Ok(page(
            vec![
                message("30", "2026-01-01T00:00:00.3Z"),
                message("10", "2026-01-01T00:00:00.1Z"),
            ],
            Some("next"),
        )),
    );
    assert_eq!(ids(&state), ["60", "50", "30", "10"]);
    let next = state.request_older(&session).unwrap();
    assert_eq!(next.before.as_deref(), Some("next"));
    state.complete_history(&mut session, &older, Err(ApiError::AlreadyInvalid));
    assert_eq!(session.generation, Some(1));
    assert_eq!(state.older.get("1"), Some(&Older::Loading));
}

#[test]
fn replacing_read_overflow_is_explicit_and_cannot_accept_a_lossy_snapshot() {
    let (mut state, mut session, read) = loading();
    let listing = state.refresh_channels(&session).unwrap();
    for index in 0..READ_STAGING_CAPACITY {
        let id = (index + 10).to_string();
        assert!(
            state
                .merge_message(message(&id, "2026-01-01T00:00:00Z"))
                .is_ok()
        );
        assert!(
            state
                .merge_channel(channel(&id, &format!("New {index:03}")))
                .is_ok()
        );
    }
    // Duplicate delivery consumes no extra staging capacity.
    assert!(
        state
            .merge_message(message("10", "2026-01-01T00:00:00Z"))
            .is_ok()
    );
    assert!(state.merge_channel(channel("10", "New 000")).is_ok());
    assert_eq!(
        state.merge_message(message("999", "2026-01-01T00:00:00Z")),
        Err(ReconciliationOverflow)
    );
    assert_eq!(
        state.merge_channel(channel("999", "Overflow")),
        Err(ReconciliationOverflow)
    );
    assert!(state.reconciliation_overflowed());
    state.complete_history(&mut session, &read, Ok(page(vec![], None)));
    state.complete_channels(&mut session, &listing, Ok(vec![]));
    assert!(matches!(state.history.get("1"), Some(Load::Failed(_))));
    assert!(state.channel_error.is_some());
    assert_eq!(state.selected.as_deref(), Some("1"));
    assert!(state.reconciliation_overflowed());
}

#[test]
fn replacing_history_retains_creations_and_confirmed_writes_but_events_do_not_settle_sends() {
    let (mut state, mut session, read) = loading();
    state.set_draft("1", "same text".into());
    let send = state.send(&session).unwrap();
    let first = message("9", "2026-01-01T00:00:00Z");
    let second = message("10", "2026-01-01T00:00:00Z");
    state.merge_message(first.clone()).unwrap();
    assert_eq!(state.draft("1"), "same text");
    assert!(state.send_pending.contains_key("1"));
    assert_eq!(
        state.complete_send(&mut session, &send, Ok(second.clone()), 0),
        SendOutcome::Confirmed
    );
    state.complete_history(&mut session, &read, Ok(page(vec![first], Some("older"))));
    assert_eq!(ids(&state), ["10", "9"]);
    state.merge_message(second).unwrap();
    assert_eq!(ids(&state), ["10", "9"]);
    assert_eq!(state.draft("1"), "");
    assert_eq!(
        state.request_older(&session).unwrap().before.as_deref(),
        Some("older")
    );
}

#[test]
fn replacing_channel_list_retains_remote_and_confirmed_creations_after_read_started() {
    let (mut state, mut session, _) = loading();
    let read = state.refresh_channels(&session).unwrap();
    let create = state.create(&session, "Local").unwrap();
    state.merge_channel(channel("4", "Remote")).unwrap();
    state.complete_create(&mut session, &create, Ok(channel("3", "Local")), 0);
    state.complete_channels(
        &mut session,
        &read,
        Ok(vec![channel("1", "General"), channel("4", "Remote")]),
    );
    assert_eq!(state.selected.as_deref(), Some("3"));
    assert_eq!(
        state.channels,
        Some(Load::Ready(vec![
            channel("1", "General"),
            channel("3", "Local"),
            channel("4", "Remote")
        ]))
    );
    // A replayed completion cannot replace the accepted snapshot or expire the session.
    state.complete_channels(&mut session, &read, Err(ApiError::AlreadyInvalid));
    assert_eq!(session.generation, Some(1));
}

#[test]
fn remote_channels_keep_selection_and_local_confirmation_selects_without_duplicates() {
    let (mut state, mut session, _) = loading();
    let create = state.create(&session, "alpha").unwrap();
    state.merge_channel(channel("3", "alpha")).unwrap();
    state.merge_channel(channel("3", "alpha")).unwrap();
    assert_eq!(state.selected.as_deref(), Some("1"));
    assert!(!state.history.contains_key("3"));
    assert!(
        state
            .complete_create(&mut session, &create, Ok(channel("3", "alpha")), 0)
            .0
    );
    assert_eq!(state.selected.as_deref(), Some("3"));
    state.merge_channel(channel("3", "alpha")).unwrap();
    assert_eq!(
        state.channels,
        Some(Load::Ready(vec![
            channel("3", "alpha"),
            channel("1", "General"),
            channel("2", "Other")
        ]))
    );
}

#[test]
fn creations_deduplicate_and_order_by_instant_then_numeric_id_without_loading_other_channels() {
    let (mut state, mut session, read) = loading();
    state.complete_history(&mut session, &read, Ok(page(vec![], None)));
    for item in [
        message("9", "2026-01-01T00:00:00Z"),
        message("10", "2026-01-01T00:00:00.000Z"),
        message("2", "2026-01-01T00:00:00.1Z"),
        message("11", "2026-01-01T01:00:00+01:00"),
        message("10", "2026-01-01T00:00:00.000Z"),
    ] {
        state.merge_message(item).unwrap();
    }
    assert_eq!(ids(&state), ["2", "11", "10", "9"]);
    let mut unloaded = message("12", "2026-01-01T00:00:00Z");
    unloaded.channel_id = "2".into();
    state.merge_message(unloaded).unwrap();
    assert!(!state.history.contains_key("2"));
}
