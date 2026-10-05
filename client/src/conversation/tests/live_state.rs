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
        _ => panic!("history not ready"),
    }
}

#[test]
fn initial_history_read_can_miss_live_creations_without_reconciliation() {
    let (mut state, mut session, read) = loading();
    assert_eq!(
        state.merge_message(message("10", "2026-01-01T00:00:00Z")),
        0
    );
    state.complete_history(&mut session, &read, Ok(page(vec![], None)));
    assert_eq!(ids(&state), Vec::<&str>::new());
    assert!(state.select(&session, "1").is_none());
    state.merge_message(message("11", "2026-01-01T00:00:00Z"));
    assert_eq!(ids(&state), ["11"]);
}

#[test]
fn inactive_confirmations_merge_into_loaded_history_or_wait_for_their_own_read() {
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
            let canceled = state.select(&session, "1").unwrap();
            assert!(state.select(&session, "2").is_none());
            state.complete_history(&mut session, &canceled, Ok(page(vec![], None)));
            let current = state.select(&session, "1").unwrap();
            state.complete_history(&mut session, &current, Ok(page(vec![], None)));
            assert_eq!(
                ids(&state),
                ["10"],
                "only the originating channel's current read includes the confirmation"
            );
        }
        state.select(&session, "2");
        assert!(state.select(&session, "1").is_none());
        assert_eq!(ids(&state), ["10"], "navigation preserves loaded history");
        assert_eq!(state.draft("1"), "");
        assert!(!state.uncertain.contains("1"));
    }
}

#[test]
fn canceled_replacing_reads_cannot_contaminate_new_selection_or_reject_its_session() {
    let (mut state, mut session, obsolete) = loading();
    state.merge_message(message("10", "2026-01-01T00:00:00Z"));
    let current = state.select(&session, "2").unwrap();
    state.complete_history(&mut session, &obsolete, Err(ApiError::AlreadyInvalid));
    assert_eq!(session.generation, Some(1));
    state.complete_history(&mut session, &current, Ok(page(vec![], None)));
    assert_eq!(state.history.get("2"), Some(&Load::Ready(vec![])));
    assert!(!state.history.contains_key("1"));
    let fresh = state.select(&session, "1").unwrap();
    state.merge_message(message("11", "2026-01-01T00:00:00Z"));
    state.complete_history(&mut session, &fresh, Ok(page(vec![], None)));
    assert_eq!(ids(&state), Vec::<&str>::new());
}

#[test]
fn confirmation_during_initial_read_survives_failure_snapshot_and_duplicate_event() {
    let (mut state, mut session, read) = loading();
    state.set_draft("1", "same text".into());
    let send = state.send(&session).unwrap();
    state.complete_send(
        &mut session,
        &send,
        Ok(message("10", "2026-01-01T00:00:00Z")),
        0,
    );
    state.set_draft("1", "next draft".into());
    state.complete_history(&mut session, &read, Err(ApiError::Unavailable));
    assert!(matches!(
        state.history_for_display("1"),
        Some(Load::Failed(_))
    ));
    assert_eq!(state.draft("1"), "next draft");
    let retry = state.select(&session, "1").unwrap();
    state.complete_history(&mut session, &read, Err(ApiError::AlreadyInvalid));
    assert_eq!(session.generation, Some(1));
    state.complete_history(
        &mut session,
        &retry,
        Ok(page(vec![message("8", "2026-01-01T00:00:00Z")], None)),
    );
    assert_eq!(ids(&state), ["10", "8"]);
    state.merge_message(message("10", "2026-01-01T00:00:00Z"));
    state.merge_message(message("9", "2026-01-01T00:00:00Z"));
    assert_eq!(ids(&state), ["10", "9", "8"]);
    assert!(state.select(&session, "1").is_none());
    assert_eq!(state.draft("1"), "next draft");
    assert!(!state.send_feedback.contains_key("1"));
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
    state.merge_message(message("10", "2026-01-01T00:00:00.1Z"));
    state.merge_message(message("60", "2026-01-01T00:00:00.6Z"));
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
fn initial_read_preserves_http_confirmation_but_live_events_do_not_settle_sends() {
    let (mut state, mut session, read) = loading();
    state.set_draft("1", "same text".into());
    let send = state.send(&session).unwrap();
    let first = message("9", "2026-01-01T00:00:00Z");
    let second = message("10", "2026-01-01T00:00:00Z");
    state.merge_message(first.clone());
    assert_eq!(state.draft("1"), "same text");
    assert!(state.send_pending.contains_key("1"));
    assert_eq!(
        state.complete_send(&mut session, &send, Ok(second.clone()), 0),
        SendOutcome::Confirmed
    );
    state.complete_history(&mut session, &read, Ok(page(vec![first], Some("older"))));
    assert_eq!(ids(&state), ["10", "9"]);
    state.merge_message(second);
    assert_eq!(ids(&state), ["10", "9"]);
    assert_eq!(state.draft("1"), "");
    assert_eq!(
        state.request_older(&session).unwrap().before.as_deref(),
        Some("older")
    );
}

#[test]
fn initial_channel_read_can_miss_live_creations_and_rejects_obsolete_completions() {
    let mut state = Conversation::default();
    let mut session = session();
    let read = state.start(&session).unwrap();
    assert!(state.start(&session).is_none());
    assert_eq!(state.merge_channel(channel("3", "Remote")), 0);
    let history = state
        .complete_channels(&mut session, &read, Ok(vec![channel("1", "General")]))
        .unwrap();
    assert_eq!(history.channel_id.as_deref(), Some("1"));
    assert_eq!(
        state.channels,
        Some(Load::Ready(vec![channel("1", "General")]))
    );
    assert_eq!(state.merge_channel(channel("4", "Remote")), 1);
    assert_eq!(state.merge_channel(channel("4", "Remote")), 0);
    assert_eq!(state.selected.as_deref(), Some("1"));
    state.complete_channels(&mut session, &read, Err(ApiError::AlreadyInvalid));
    assert_eq!(session.generation, Some(1));
    assert_eq!(
        state.channels,
        Some(Load::Ready(vec![
            channel("1", "General"),
            channel("4", "Remote")
        ]))
    );
}

#[test]
fn remote_channels_keep_selection_and_local_confirmation_selects_without_duplicates() {
    let (mut state, mut session, _) = loading();
    let create = state.create(&session, "alpha").unwrap();
    state.merge_channel(channel("3", "alpha"));
    state.merge_channel(channel("3", "alpha"));
    assert_eq!(state.selected.as_deref(), Some("1"));
    assert!(!state.history.contains_key("3"));
    assert!(
        state
            .complete_create(&mut session, &create, Ok(channel("3", "alpha")), 0)
            .0
    );
    assert_eq!(state.selected.as_deref(), Some("3"));
    state.merge_channel(channel("3", "alpha"));
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
        state.merge_message(item);
    }
    assert_eq!(ids(&state), ["2", "11", "10", "9"]);
    let mut unloaded = message("12", "2026-01-01T00:00:00Z");
    unloaded.channel_id = "2".into();
    state.merge_message(unloaded);
    assert!(!state.history.contains_key("2"));
}
