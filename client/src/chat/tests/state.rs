use super::*;
#[path = "channel_delete_state.rs"]
mod channel_delete;
fn logged_in() -> Identity {
    Identity::new(1, 100)
}
fn channels() -> Vec<Channel> {
    vec![
        Channel {
            id: "z".into(),
            name: "Zebra".into(),
        },
        Channel {
            id: "a".into(),
            name: "alpha".into(),
        },
    ]
}
fn message(id: &str, channel: &str) -> Message {
    Message {
        id: id.into(),
        channel_id: channel.into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: "same text".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    }
}
fn page(ids: &[&str], cursor: Option<&str>) -> Page {
    Page {
        items: ids.iter().map(|id| message(id, "z")).collect(),
        next_cursor: cursor.map(str::to_owned),
    }
}
fn ready() -> (ChatState, Identity) {
    let mut session = logged_in();
    let mut state = ChatState::default();
    let list = state.start(&session).unwrap();
    let read = state
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    state.complete_history(&mut session, &read, Ok(page(&["8"], Some("opaque older"))));
    (state, session)
}
/// Whether the completion confirmed the creation, observed as the create revision advancing.
fn confirms_create(
    state: &mut ChatState,
    session: &mut Identity,
    request: &CreateRequest,
    result: Result<Channel, ApiError>,
    now: i64,
) -> bool {
    let revision = state.create.revision;
    state.complete_create(session, request, result, now);
    state.create.revision == revision.wrapping_add(1)
}
fn ids(state: &ChatState) -> Vec<&str> {
    let Some(Load::Ready(items)) = state.history("z") else {
        panic!("history not ready")
    };
    items.iter().map(|item| item.id.as_str()).collect()
}

#[test]
fn selected_channel_resolves_only_loaded_matching_selection() {
    let mut state = ChatState {
        selected: Some("a".into()),
        ..ChatState::default()
    };
    assert!(state.selected_channel().is_none());
    state.channels = Some(Load::Loading);
    assert!(state.selected_channel().is_none());
    state.channels = Some(Load::Failed("unavailable".into()));
    assert!(state.selected_channel().is_none());
    state.channels = Some(Load::Ready(channels()));
    assert_eq!(state.selected_channel().unwrap().name, "alpha");
    state.selected = Some("z".into());
    assert_eq!(state.selected_channel().unwrap().name, "Zebra");
    state.selected = Some("unknown".into());
    assert!(state.selected_channel().is_none());
    state.selected = None;
    assert!(state.selected_channel().is_none());
}

#[test]
fn drafts_and_write_identities_are_independent_of_navigation_and_matching_events() {
    let (mut state, mut session) = ready();
    state.set_draft("z", "same text".into());
    let first = state.send(&session).unwrap();
    assert!(state.send(&session).is_none());
    state.set_draft("z", "locked".into());
    assert_eq!(state.draft("z"), "same text");
    let other = state.select(&session, "a").unwrap();
    state.complete_history(&mut session, &other, Ok(page(&[], None)));
    state.set_draft("a", "other draft".into());
    let second = state.send(&session).unwrap();
    state.merge_message(message("9", "z"));
    assert_eq!(state.draft("z"), "same text");
    assert_eq!(
        state.complete_send(&mut session, &first, Err(ApiError::Unavailable), 0),
        SendOutcome::Uncertain
    );
    assert_eq!(
        state.complete_send(&mut session, &second, Err(ApiError::InvalidInput), 0),
        SendOutcome::Rejected
    );
    assert_eq!(state.draft("a"), "other draft");
    assert!(
        state.select(&session, "z").is_none(),
        "uncertainty cannot cause a read"
    );
    state.merge_message(message("10", "z"));
    assert!(state.conversation("z").is_some_and(|c| c.uncertain));
    assert!(state.send_feedback("z").unwrap().contains("may already"));
    let deliberate = state.send(&session).unwrap();
    assert_eq!(
        state.complete_send(&mut session, &deliberate, Ok(message("11", "z")), 0),
        SendOutcome::Confirmed
    );
    state.set_draft("z", "new draft".into());
    assert_eq!(
        state.complete_send(&mut session, &deliberate, Ok(message("11", "z")), 0),
        SendOutcome::Stale
    );
    assert_eq!(state.draft("z"), "new draft");
    assert_eq!(ids(&state), ["11", "10", "9", "8"]);
}

#[test]
fn confirmed_entities_merge_immediately_and_only_originating_draft_is_cleared() {
    let (mut state, mut session) = ready();
    for text in ["  ".to_owned(), "x".repeat(4001)] {
        state.set_draft("z", text);
        assert!(state.send(&session).is_none());
    }
    state.set_draft("z", "same text".into());
    let send = state.send(&session).unwrap();
    let other = state.select(&session, "a").unwrap();
    state.complete_history(&mut session, &other, Ok(page(&[], None)));
    state.set_draft("a", "different".into());
    assert_eq!(
        state.complete_send(&mut session, &send, Ok(message("10", "z")), 0),
        SendOutcome::Confirmed
    );
    state.merge_message(message("10", "z"));
    assert_eq!(ids(&state), ["10", "8"]);
    assert_eq!(state.draft("z"), "");
    assert_eq!(state.draft("a"), "different");
    assert!(state.send_feedback("z").is_none());
    assert!(state.select(&session, "z").is_none());
}

#[test]
fn authority_and_session_replacement_gate_reads_and_writes_before_rejection() {
    for expiry in [false, true] {
        let (mut state, mut session) = ready();
        state.set_draft("z", "private".into());
        let send = state.send(&session).unwrap();
        let older = state.request_older(&session).unwrap();
        let create = state.create(&session, "New").unwrap();
        if expiry {
            session.expire(100);
        } else {
            session.close();
        }
        state.clear();
        session = Identity::new(2, session.expires_at);
        let list = state.start(&session).unwrap();
        assert_eq!(
            state.complete_send(&mut session, &send, Err(ApiError::SessionRejected), 0),
            SendOutcome::Stale
        );
        state.complete_history(&mut session, &older, Err(ApiError::SessionRejected));
        assert!(!confirms_create(
            &mut state,
            &mut session,
            &create,
            Err(ApiError::SessionRejected),
            0
        ));
        assert_eq!(session.session_generation(), Some(2));
        assert!(state.is_current_channels(&list));
        assert!(state.conversations().values().all(|c| c.draft.is_none()));
        assert!(state.conversations().values().all(|c| c.history.is_none()));
    }
    let (mut state, mut session) = ready();
    state.set_draft("z", "private".into());
    let send = state.send(&session).unwrap();
    let old = state.request_older(&session).unwrap();
    assert_eq!(
        state.complete_send(&mut session, &send, Err(ApiError::SessionRejected), 0),
        SendOutcome::Invalidated
    );
    assert!(session.session_generation().is_none());
    assert!(state.conversations().values().all(|c| c.draft.is_none()));
    state.complete_history(&mut session, &old, Ok(page(&["7"], None)));
    assert!(state.channels.is_none());
}

#[test]
fn channel_snapshot_preserves_server_order_and_only_loads_current_selection() {
    let mut session = logged_in();
    let mut state = ChatState::default();
    let list = state.start(&session).unwrap();
    let first = state
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    assert_eq!(first.channel_id, "z");
    assert_eq!(state.channels, Some(Load::Ready(channels())));
    assert_eq!(
        state
            .conversations()
            .values()
            .filter(|c| c.history.is_some())
            .count(),
        1
    );
    assert!(state.select(&session, "unknown").is_none());
    assert!(state.select(&session, "z").is_none());
    let other = state.select(&session, "a").unwrap();
    state.complete_history(&mut session, &first, Err(ApiError::SessionRejected));
    assert_eq!(session.session_generation(), Some(1));
    assert!(state.history("z").is_none());
    state.complete_history(&mut session, &other, Ok(page(&[], None)));
    let current = state.select(&session, "z").unwrap();
    state.complete_history(&mut session, &current, Ok(page(&["9", "8"], None)));
    assert_eq!(ids(&state), ["9", "8"]);
    assert!(state.select(&session, "a").is_none());
}

#[test]
fn channel_creation_validates_names_selects_only_on_confirmation_and_rejects_late_results() {
    let (mut state, mut session) = ready();
    for name in ["", "   ", "bad!", &"x".repeat(65)] {
        assert!(state.create(&session, name).is_none());
    }
    let create = state.create(&session, "  New  ").unwrap();
    assert_eq!(create.name, "New");
    assert!(state.create(&session, "Duplicate").is_none());
    let channel = Channel {
        id: "new".into(),
        name: "New".into(),
    };
    state.merge_channel(channel.clone());
    assert_eq!(state.selected.as_deref(), Some("z"));
    let revision = state.create.revision;
    let read = state.complete_create(&mut session, &create, Ok(channel), 0);
    assert_ne!(state.create.revision, revision);
    assert_eq!(read.unwrap().channel_id, "new");
    assert!(!confirms_create(
        &mut state,
        &mut session,
        &create,
        Err(ApiError::SessionRejected),
        0
    ));
    assert_eq!(session.session_generation(), Some(1));
    let pending = state.create(&session, "Later").unwrap();
    assert!(!confirms_create(
        &mut state,
        &mut session,
        &pending,
        Ok(Channel {
            id: "later".into(),
            name: "Later".into()
        }),
        100
    ));
    assert!(session.session_generation().is_none());
    assert!(state.channels.is_none());
}

#[test]
fn older_pages_keep_cursor_and_entities_through_local_failure_retry_and_duplicate_pages() {
    let (mut state, mut session) = ready();
    let older = state.request_older(&session).unwrap();
    assert_eq!(older.before.as_deref(), Some("opaque older"));
    assert!(state.request_older(&session).is_none());
    state.merge_message(message("9", "z"));
    state.complete_history(&mut session, &older, Err(ApiError::Unavailable));
    assert_eq!(ids(&state), ["9", "8"]);
    assert!(matches!(state.older("z"), Some(OlderPages::Failed(_))));
    assert!(state.request_older(&session).is_none());
    let retry = state.retry_older(&session).unwrap();
    assert_eq!(retry.before, older.before);
    assert_eq!(
        state
            .complete_history(&mut session, &older, Err(ApiError::SessionRejected))
            .added,
        0
    );
    assert_eq!(session.session_generation(), Some(1));
    assert_eq!(
        state
            .complete_history(
                &mut session,
                &retry,
                Ok(page(&["8", "7", "6", "7"], Some("next opaque")))
            )
            .added,
        2
    );
    let final_read = state.request_older(&session).unwrap();
    assert_eq!(final_read.before.as_deref(), Some("next opaque"));
    state.complete_history(
        &mut session,
        &final_read,
        Ok(page(&["6", "5"], Some("next opaque"))),
    );
    assert_eq!(state.older("z"), Some(&OlderPages::Exhausted));
    assert_eq!(ids(&state), ["9", "8", "7", "6", "5"]);
    assert!(state.request_older(&session).is_none());
}

#[test]
fn navigation_and_local_creation_cancel_older_identity_without_losing_cursor() {
    let (mut state, mut session) = ready();
    let old = state.request_older(&session).unwrap();
    let create = state.create(&session, "New").unwrap();
    state.complete_create(
        &mut session,
        &create,
        Ok(Channel {
            id: "new".into(),
            name: "New".into(),
        }),
        0,
    );
    state.select(&session, "z");
    let current = state.request_older(&session).unwrap();
    assert_eq!(current.before, old.before);
    state.complete_history(&mut session, &old, Err(ApiError::SessionRejected));
    assert_eq!(session.session_generation(), Some(1));
    state.complete_history(&mut session, &current, Err(ApiError::SessionRejected));
    assert!(session.session_generation().is_none());
    assert!(state.conversations().values().all(|c| c.history.is_none()));
    assert!(state.conversations().values().all(|c| c.older.is_none()));
}
