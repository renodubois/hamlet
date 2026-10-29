use super::*;

#[test]
fn deletion_discards_operations_rejects_late_deliveries_and_can_clear_selection() {
    let (mut state, mut session) = ready();
    state.set_draft("z", "pending send".into());
    let send = state.send(&session).unwrap();
    let rename = state.rename(&session, "z", "new name").unwrap();
    let deletion = state.channel_delete(&session, "z").unwrap();
    assert!(state.channel_delete(&session, "a").is_none());
    let fallback = state
        .complete_channel_delete(&mut session, &deletion, Ok(()))
        .unwrap();
    assert_eq!(state.selected.as_deref(), Some("a"));
    assert!(state.history("z").is_none());
    assert!(state.conversation("z").is_none_or(|c| c.draft.is_none()));
    assert!(!state.is_sending("z"));
    assert!(
        state.rename.pending,
        "deletion does not confirm or cancel a dialog request"
    );
    state.complete_rename(
        &mut session,
        &rename,
        Ok(Channel {
            id: "z".into(),
            name: "late rename".into(),
        }),
    );
    assert_eq!(state.rename.revision, 0);
    assert!(!state.rename.pending);
    assert!(state.rename.feedback.is_some());
    assert_eq!(
        state.complete_send(&mut session, &send, Ok(message("late", "z")), 0),
        SendOutcome::Stale
    );
    state.set_draft("z", "late input".into());
    assert!(state.conversation("z").is_none_or(|c| c.draft.is_none()));
    assert_eq!(
        state.merge_channel(Channel {
            id: "z".into(),
            name: "late creation".into()
        }),
        0
    );
    assert_eq!(state.merge_message(message("late", "z")), 0);
    state.complete_history(
        &mut session,
        &fallback,
        Ok(Page {
            items: vec![],
            next_cursor: None,
        }),
    );
    let final_delete = state.channel_delete(&session, "a").unwrap();
    assert!(
        state
            .complete_channel_delete(&mut session, &final_delete, Ok(()))
            .is_none()
    );
    assert!(
        state.selected.is_none(),
        "no invented fallback when locally empty"
    );
    assert!(state.conversations().values().all(|c| c.history.is_none()));
}

#[test]
fn deletion_failure_and_obsolete_session_never_cleanup() {
    for error in [
        ApiError::Conflict,
        ApiError::NotFound,
        ApiError::Unavailable,
    ] {
        let (mut state, mut session) = ready();
        state.set_draft("z", "retained".into());
        let request = state.channel_delete(&session, "z").unwrap();
        assert!(
            state
                .complete_channel_delete(&mut session, &request, Err(error))
                .is_none()
        );
        assert_eq!(state.selected.as_deref(), Some("z"));
        assert_eq!(state.draft("z"), "retained");
        assert!(state.history("z").is_some());
        assert_eq!(state.channel_delete.revision, 0);
        assert!(state.channel_delete.feedback.is_some());
    }
    let (mut state, mut session) = ready();
    let request = state.channel_delete(&session, "z").unwrap();
    session = Identity::new(2, session.expires_at);
    state.complete_channel_delete(&mut session, &request, Ok(()));
    assert!(state.history("z").is_some());
    assert_eq!(state.channel_delete.revision, 0);
}
