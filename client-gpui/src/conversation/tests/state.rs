use super::*;
fn logged_in() -> Identity {
    Identity {
        generation: Some(1),
        expires_at: 100,
        rejected: None,
    }
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
#[test]
fn sends_keep_independent_drafts_and_identity_across_navigation_refresh_and_invalidation() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let listing = view.start(&session).unwrap();
    let first = view
        .complete_channels(&mut session, &listing, Ok(channels()))
        .unwrap();
    let message = |id: &str, channel: &str, text: &str| Message {
        id: id.into(),
        channel_id: channel.into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: text.into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    view.complete_history(
        &mut session,
        &first,
        Ok(Page {
            items: vec![message("8", "z", "old")],
            next_cursor: None,
        }),
    );
    view.set_draft("z", "same text\nsecond line".into());
    let sent = view.send(&session).unwrap();
    assert!(view.send(&session).is_none());
    view.set_draft("z", "blocked".into());
    assert_eq!(view.draft("z"), "same text\nsecond line");
    let other = view.select(&session, "a").unwrap();
    view.complete_history(
        &mut session,
        &other,
        Ok(Page {
            items: vec![],
            next_cursor: None,
        }),
    );
    view.set_draft("a", "other".into());
    let second = view.send(&session).unwrap();
    assert_eq!(
        view.complete_send(&mut session, &sent, Err(ApiError::Unavailable), 0),
        SendOutcome::Uncertain
    );
    assert_eq!(view.draft("z"), "same text\nsecond line");
    assert!(view.uncertain.contains("z"));
    assert!(
        view.refreshing.is_empty(),
        "inactive uncertainty cannot fetch history"
    );
    assert_eq!(
        view.complete_send(&mut session, &second, Err(ApiError::InvalidInput), 0),
        SendOutcome::Rejected
    );
    assert_eq!(view.draft("a"), "other");
    assert_eq!(
        view.complete_send(&mut session, &second, Ok(message("10", "a", "other")), 0),
        SendOutcome::Stale
    );
    let refresh = view.select(&session, "z").unwrap();
    assert_eq!(view.refreshing.get("z"), Some(&Refresh::Running));
    assert!(view.uncertain.is_empty());
    let sent = view.send(&session).unwrap(); // only deliberate user action replays an uncertain write
    assert_eq!(
        view.complete_send(
            &mut session,
            &sent,
            Ok(message("10", "z", "same text\nsecond line")),
            0
        ),
        SendOutcome::Confirmed
    );
    assert_eq!(view.draft("z"), "");
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(vec![message("8", "z", "old")]))
    );
    // Refresh sees the confirmed identity first; no duplicate when its completion arrives.
    view.complete_history(
        &mut session,
        &refresh,
        Ok(Page {
            items: vec![
                message("10", "z", "same text\nsecond line"),
                message("9", "z", "same text\nsecond line"),
                message("8", "z", "old"),
            ],
            next_cursor: None,
        }),
    );
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(vec![
            message("10", "z", "same text\nsecond line"),
            message("9", "z", "same text\nsecond line"),
            message("8", "z", "old")
        ]))
    );
    view.set_draft("z", "new draft".into());
    let late = view.send(&session).unwrap();
    session.generation = None;
    view.clear();
    assert_eq!(
        view.complete_send(&mut session, &late, Ok(message("11", "z", "new draft")), 0),
        SendOutcome::Stale
    );
    assert!(view.drafts.is_empty());
    assert!(view.history.is_empty());
}

#[test]
fn confirmation_before_refresh_merges_once_in_server_order_and_bad_inputs_do_not_send() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let listing = view.start(&session).unwrap();
    let initial = view
        .complete_channels(&mut session, &listing, Ok(channels()))
        .unwrap();
    let m = |id: &str| Message {
        id: id.into(),
        channel_id: "z".into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: "identical text".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    view.complete_history(
        &mut session,
        &initial,
        Ok(Page {
            items: vec![m("8")],
            next_cursor: None,
        }),
    );
    for invalid in ["   ".to_owned(), "x".repeat(4001)] {
        view.set_draft("z", invalid);
        assert!(view.send(&session).is_none());
        assert!(!view.send_pending.contains_key("z"));
    }
    view.set_draft("z", "identical text".into());
    let send = view.send(&session).unwrap();
    assert_eq!(
        view.complete_send(&mut session, &send, Ok(m("10")), 0),
        SendOutcome::Confirmed
    );
    assert_eq!(view.history.get("z"), Some(&Load::Ready(vec![m("8")])));
    let refresh = view.reconcile_confirmed(&session).unwrap();
    view.complete_history(
        &mut session,
        &refresh,
        Ok(Page {
            items: vec![m("10"), m("9"), m("8")],
            next_cursor: None,
        }),
    );
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(vec![m("10"), m("9"), m("8")]))
    );
}

#[test]
fn second_confirmation_during_catchup_requires_a_new_safe_read() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let list = view.start(&session).unwrap();
    let initial = view
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    let message = |id: &str| Message {
        id: id.into(),
        channel_id: "z".into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: id.into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    let page = |ids: &[&str]| Page {
        items: ids.iter().map(|id| message(id)).collect(),
        next_cursor: None,
    };
    view.complete_history(&mut session, &initial, Ok(page(&["1"])));
    view.set_draft("z", "first".into());
    let first = view.send(&session).unwrap();
    assert_eq!(
        view.complete_send(&mut session, &first, Ok(message("3")), 0),
        SendOutcome::Confirmed
    );
    let stale_read = view.reconcile_confirmed(&session).unwrap();
    view.set_draft("z", "second".into());
    let second = view.send(&session).unwrap();
    assert_eq!(
        view.complete_send(&mut session, &second, Ok(message("4")), 0),
        SendOutcome::Confirmed
    );
    assert_eq!(
        view.complete_history(&mut session, &stale_read, Ok(page(&["3", "2", "1"])))
            .added,
        2
    );
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(page(&["3", "2", "1"]).items))
    );
    assert_eq!(view.confirmed.get("z").map(Vec::len), Some(2));
    let fresh = view.after_history_read(&session, true).unwrap();
    assert!(fresh.refresh);
    assert_eq!(
        view.complete_history(&mut session, &fresh, Ok(page(&["4", "3", "2", "1"])))
            .added,
        1
    );
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(page(&["4", "3", "2", "1"]).items))
    );
    assert!(view.confirmed.is_empty());
    assert!(view.after_history_read(&session, true).is_none());
}

#[test]
fn confirmed_message_remains_staged_when_catchup_cannot_prove_continuity() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let listing = view.start(&session).unwrap();
    let first = view
        .complete_channels(&mut session, &listing, Ok(channels()))
        .unwrap();
    let m = |id: &str| Message {
        id: id.into(),
        channel_id: "z".into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: "same".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    view.complete_history(
        &mut session,
        &first,
        Ok(Page {
            items: vec![m("8")],
            next_cursor: None,
        }),
    );
    view.set_draft("z", "same".into());
    let send = view.send(&session).unwrap();
    view.complete_send(&mut session, &send, Ok(m("10")), 0);
    let refresh = view.reconcile_confirmed(&session).unwrap();
    view.complete_history(
        &mut session,
        &refresh,
        Ok(Page {
            items: vec![m("10")],
            next_cursor: None,
        }),
    );
    assert_eq!(view.history.get("z"), Some(&Load::Ready(vec![m("8")])));
    assert!(matches!(
        view.refreshing.get("z"),
        Some(Refresh::Incomplete(_))
    ));
    assert!(view.confirmed.contains_key("z"));
    let retry = view.reconcile_confirmed(&session).unwrap(); // deliberate retry only
    view.complete_history(
        &mut session,
        &retry,
        Ok(Page {
            items: vec![m("10"), m("9"), m("8")],
            next_cursor: None,
        }),
    );
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(vec![m("10"), m("9"), m("8")]))
    );
}

#[test]
fn success_for_inactive_channel_clears_only_its_draft_and_waits_for_selection() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let listing = view.start(&session).unwrap();
    let first = view
        .complete_channels(&mut session, &listing, Ok(channels()))
        .unwrap();
    let m = |id: &str| Message {
        id: id.into(),
        channel_id: "z".into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: "same".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    view.complete_history(
        &mut session,
        &first,
        Ok(Page {
            items: vec![m("8")],
            next_cursor: None,
        }),
    );
    view.set_draft("z", "same".into());
    let send = view.send(&session).unwrap();
    let other = view.select(&session, "a").unwrap();
    view.complete_history(
        &mut session,
        &other,
        Ok(Page {
            items: vec![],
            next_cursor: None,
        }),
    );
    view.set_draft("a", "different draft".into());
    assert_eq!(
        view.complete_send(&mut session, &send, Ok(m("10")), 0),
        SendOutcome::Confirmed
    );
    assert_eq!(view.draft("a"), "different draft");
    assert_eq!(view.draft("z"), "");
    assert!(view.refreshing.is_empty());
    let refresh = view.select(&session, "z").unwrap();
    view.complete_history(
        &mut session,
        &refresh,
        Ok(Page {
            items: vec![m("10"), m("9"), m("8")],
            next_cursor: None,
        }),
    );
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(vec![m("10"), m("9"), m("8")]))
    );
}

#[test]
fn authoritative_send_rejection_clears_all_drafts_and_invalidates_late_channel_completion() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let listing = view.start(&session).unwrap();
    let first = view
        .complete_channels(&mut session, &listing, Ok(channels()))
        .unwrap();
    view.complete_history(
        &mut session,
        &first,
        Ok(Page {
            items: vec![],
            next_cursor: None,
        }),
    );
    view.set_draft("z", "first".into());
    let rejected = view.send(&session).unwrap();
    let other = view.select(&session, "a").unwrap();
    view.complete_history(
        &mut session,
        &other,
        Ok(Page {
            items: vec![],
            next_cursor: None,
        }),
    );
    view.set_draft("a", "second".into());
    let late = view.send(&session).unwrap();
    assert_eq!(
        view.complete_send(&mut session, &rejected, Err(ApiError::AlreadyInvalid), 0),
        SendOutcome::Invalidated
    );
    assert!(session.generation.is_none());
    assert!(view.drafts.is_empty());
    assert!(view.send_pending.is_empty());
    assert_eq!(
        view.complete_send(&mut session, &late, Err(ApiError::Unavailable), 0),
        SendOutcome::Stale
    );
    assert!(view.send_feedback.is_empty());
}

#[test]
fn refresh_that_started_before_confirmation_cannot_hide_an_intervening_message() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let list = view.start(&session).unwrap();
    let first = view
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    let m = |id: &str| Message {
        id: id.into(),
        channel_id: "z".into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: "same".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    view.complete_history(
        &mut session,
        &first,
        Ok(Page {
            items: vec![m("8")],
            next_cursor: None,
        }),
    );
    let old_refresh = view.refresh_history(&session).unwrap();
    view.set_draft("z", "same".into());
    let send = view.send(&session).unwrap();
    assert_eq!(
        view.complete_send(&mut session, &send, Ok(m("10")), 0),
        SendOutcome::Confirmed
    );
    assert_eq!(view.history.get("z"), Some(&Load::Ready(vec![m("8")])));
    view.complete_history(
        &mut session,
        &old_refresh,
        Ok(Page {
            items: vec![m("8")],
            next_cursor: None,
        }),
    );
    assert_eq!(view.history.get("z"), Some(&Load::Ready(vec![m("8")])));
    let after_send = view.reconcile_confirmed(&session).unwrap();
    view.complete_history(
        &mut session,
        &after_send,
        Ok(Page {
            items: vec![m("10"), m("9"), m("8")],
            next_cursor: None,
        }),
    );
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(vec![m("10"), m("9"), m("8")]))
    );
}

#[test]
fn poll_before_send_confirmation_requires_a_fresh_read_and_deduplicates_by_id() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let list = view.start(&session).unwrap();
    let initial = view
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    let m = |id: &str| Message {
        id: id.into(),
        channel_id: "z".into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: "same text".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    let page = |ids: &[&str]| Page {
        items: ids.iter().map(|id| m(id)).collect(),
        next_cursor: None,
    };
    view.complete_history(&mut session, &initial, Ok(page(&["8"])));
    view.set_draft("z", "same text".into());
    let send = view.send(&session).unwrap();
    let poll = view.refresh_history(&session).unwrap();
    assert!(view.refresh_history(&session).is_none());
    // The poll observes the publication before the POST response arrives.
    view.complete_history(&mut session, &poll, Ok(page(&["10", "9", "8"])));
    assert_eq!(view.draft("z"), "same text");
    assert_eq!(
        view.complete_send(&mut session, &send, Ok(m("10")), 0),
        SendOutcome::Confirmed
    );
    assert_eq!(view.draft("z"), "");
    let confirmation_read = view.reconcile_confirmed(&session).unwrap();
    view.complete_history(
        &mut session,
        &confirmation_read,
        Ok(page(&["10", "9", "8"])),
    );
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(page(&["10", "9", "8"]).items))
    );
    assert!(view.confirmed.is_empty());
    assert!(view.after_history_read(&session, true).is_none());
}

#[test]
fn uncertain_send_while_polling_switches_channels_never_replays_or_clears_draft() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let list = view.start(&session).unwrap();
    let first = view
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    let m = |id: &str| Message {
        id: id.into(),
        channel_id: "z".into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: "maybe".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    view.complete_history(
        &mut session,
        &first,
        Ok(Page {
            items: vec![m("8")],
            next_cursor: None,
        }),
    );
    view.set_draft("z", "maybe".into());
    let send = view.send(&session).unwrap();
    let poll = view.refresh_history(&session).unwrap();
    let other = view.select(&session, "a").unwrap();
    assert_eq!(
        view.complete_send(&mut session, &send, Err(ApiError::Unavailable), 0),
        SendOutcome::Uncertain
    );
    view.complete_history(&mut session, &poll, Err(ApiError::AlreadyInvalid));
    assert!(
        session.generation.is_some(),
        "canceled poll cannot invalidate the session"
    );
    assert!(view.after_history_read(&session, false).is_none());
    assert_eq!(view.draft("z"), "maybe");
    view.complete_history(
        &mut session,
        &other,
        Ok(Page {
            items: vec![],
            next_cursor: None,
        }),
    );
    assert_eq!(view.draft("z"), "maybe");
    let reconcile = view.select(&session, "z").unwrap();
    assert!(reconcile.is_catchup());
    assert!(view.uncertain.is_empty());
    assert!(view.send_pending.is_empty());
    assert!(view.send_feedback.get("z").unwrap().contains("may already"));
    view.complete_history(
        &mut session,
        &reconcile,
        Ok(Page {
            items: vec![m("10"), m("9"), m("8")],
            next_cursor: None,
        }),
    );
    assert_eq!(
        view.draft("z"),
        "maybe",
        "matching text is not confirmation"
    );
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(vec![m("10"), m("9"), m("8")]))
    );
}

#[test]
fn logout_expiry_and_old_server_rejection_cannot_settle_pending_poll_or_write() {
    for expire in [false, true] {
        let mut session = logged_in();
        let mut view = Conversation::default();
        let list = view.start(&session).unwrap();
        let first = view
            .complete_channels(&mut session, &list, Ok(channels()))
            .unwrap();
        view.complete_history(
            &mut session,
            &first,
            Ok(Page {
                items: vec![],
                next_cursor: None,
            }),
        );
        view.set_draft("z", "private".into());
        let send = view.send(&session).unwrap();
        let poll = view.refresh_history(&session).unwrap();
        if expire {
            session.expire(100);
        } else {
            session.generation = None;
        }
        view.clear();
        assert_eq!(
            view.complete_send(&mut session, &send, Err(ApiError::AlreadyInvalid), 0),
            SendOutcome::Stale
        );
        view.complete_history(
            &mut session,
            &poll,
            Ok(Page {
                items: vec![],
                next_cursor: None,
            }),
        );
        assert!(view.drafts.is_empty());
        assert!(view.history.is_empty());
        assert!(session.generation.is_none());
    }
    let mut session = logged_in();
    let mut view = Conversation::default();
    let old_list = view.start(&session).unwrap();
    let old_read = view
        .complete_channels(&mut session, &old_list, Ok(channels()))
        .unwrap();
    view.complete_history(
        &mut session,
        &old_read,
        Ok(Page {
            items: vec![],
            next_cursor: None,
        }),
    );
    view.set_draft("z", "old".into());
    let old_send = view.send(&session).unwrap();
    let old_poll = view.refresh_history(&session).unwrap();
    view.clear();
    session.generation = Some(2);
    session.expires_at = 100;

    let new_list = view.start(&session).unwrap();
    assert_eq!(
        view.complete_send(&mut session, &old_send, Err(ApiError::AlreadyInvalid), 0),
        SendOutcome::Stale
    );
    view.complete_history(&mut session, &old_poll, Err(ApiError::AlreadyInvalid));
    view.complete_channels(&mut session, &old_list, Err(ApiError::AlreadyInvalid));
    assert!(session.generation.is_some());
    assert!(view.is_current_channels(&new_list));
    assert!(view.drafts.is_empty());
}

#[test]
fn ordered_channels_first_selection_and_only_selected_history() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let list = view.start(&session).unwrap();
    let first = view
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    assert_eq!(view.selected.as_deref(), Some("z"));
    assert_eq!(first.channel_id.as_deref(), Some("z"));
    assert_eq!(view.history.len(), 1);
    assert!(view.select(&session, "unknown").is_none());
    assert!(view.select(&session, "z").is_none());
    assert_eq!(
        view.select(&session, "a").unwrap().channel_id.as_deref(),
        Some("a")
    );
}
#[test]
fn empty_failed_and_tied_order_without_resorting() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let list = view.start(&session).unwrap();
    assert!(
        view.complete_channels(&mut session, &list, Ok(vec![]))
            .is_none()
    );
    assert!(view.selected.is_none());
    view.clear();
    let list = view.start(&session).unwrap();
    view.complete_channels(&mut session, &list, Err(ApiError::Unavailable));
    assert!(matches!(view.channels, Some(Load::Failed(_))));
    view.clear();
    let list = view.start(&session).unwrap();
    let request = view
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    let messages = ["9", "8"]
        .map(|id| Message {
            id: id.into(),
            channel_id: "z".into(),
            author_id: "u".into(),
            author_name: "Ada".into(),
            text: id.into(),
            created_at: "same".into(),
        })
        .to_vec();
    view.complete_history(
        &mut session,
        &request,
        Ok(Page {
            items: messages.clone(),
            next_cursor: None,
        }),
    );
    assert_eq!(view.history.get("z"), Some(&Load::Ready(messages)));
}
#[test]
fn out_of_order_reads_and_expired_sessions_do_not_replace_selection() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let list = view.start(&session).unwrap();
    let old = view
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    let current = view.select(&session, "a").unwrap();
    view.complete_history(&mut session, &old, Err(ApiError::Unavailable));
    assert_eq!(view.selected.as_deref(), Some("a"));
    assert!(
        !view.history.contains_key("z"),
        "canceled reads do not cache stale failures"
    );
    view.complete_history(
        &mut session,
        &current,
        Ok(Page {
            items: vec![],
            next_cursor: None,
        }),
    );
    assert_eq!(view.history.get("a"), Some(&Load::Ready(vec![])));
    session.generation = None;
    view.clear();
    view.complete_history(&mut session, &old, Err(ApiError::AlreadyInvalid));
    assert!(view.history.is_empty());
    assert!(view.selected.is_none());
    assert!(session.generation.is_none());
}

#[test]
fn old_server_responses_cannot_repopulate_a_new_session() {
    let mut session = logged_in();
    let mut conversation = Conversation::default();
    let old_channels = conversation.start(&session).unwrap();
    let old_history = conversation
        .complete_channels(&mut session, &old_channels, Ok(channels()))
        .unwrap();
    session.generation = Some(2);
    session.expires_at = 100;
    conversation.clear();

    conversation.complete_history(&mut session, &old_history, Err(ApiError::AlreadyInvalid));
    assert!(
        conversation
            .complete_channels(&mut session, &old_channels, Ok(channels()))
            .is_none()
    );
    assert_eq!(session.generation, Some(2));
    assert!(conversation.channels.is_none());
    assert!(conversation.history.is_empty());
}

#[test]
fn reselecting_a_failed_channel_retries_without_losing_successful_history() {
    let mut session = logged_in();
    let mut conversation = Conversation::default();
    let request = conversation.start(&session).unwrap();
    let first = conversation
        .complete_channels(&mut session, &request, Ok(channels()))
        .unwrap();
    conversation.complete_history(&mut session, &first, Err(ApiError::Unavailable));
    let other = conversation.select(&session, "a").unwrap();
    conversation.complete_history(
        &mut session,
        &other,
        Ok(Page {
            items: vec![],
            next_cursor: None,
        }),
    );
    let retry = conversation
        .select(&session, "z")
        .expect("failed channel must retry");
    assert!(matches!(conversation.history.get("z"), Some(Load::Loading)));
    conversation.complete_history(
        &mut session,
        &retry,
        Ok(Page {
            items: vec![],
            next_cursor: None,
        }),
    );
    assert_eq!(conversation.history.get("z"), Some(&Load::Ready(vec![])));
    assert!(
        conversation.select(&session, "a").is_none(),
        "successful history is cached"
    );
}

#[test]
fn late_create_cannot_navigate_after_logout_or_server_switch() {
    let mut session = logged_in();
    let mut conversation = Conversation::default();
    let list = conversation.start(&session).unwrap();
    conversation.complete_channels(&mut session, &list, Ok(vec![]));
    let old = conversation.create(&session, "New").unwrap();
    assert!(conversation.create(&session, "New").is_none());
    session.generation = None;
    conversation.clear();
    assert!(
        !conversation
            .complete_create(
                &mut session,
                &old,
                Ok(Channel {
                    id: "3".into(),
                    name: "New".into()
                }),
                0
            )
            .0
    );
    assert!(conversation.channels.is_none());
    session.generation = Some(2);
    session.expires_at = 100;

    let list = conversation.start(&session).unwrap();
    conversation.complete_channels(&mut session, &list, Ok(channels()));
    let selection = conversation.selected.clone();
    assert!(
        !conversation
            .complete_create(&mut session, &old, Err(ApiError::AlreadyInvalid), 0)
            .0
    );
    assert_eq!(conversation.selected, selection);
    assert!(session.generation.is_some());
}

#[test]
fn an_expired_session_cannot_accept_late_channel_creation() {
    let mut session = logged_in();
    let mut conversation = Conversation::default();
    let list = conversation.start(&session).unwrap();
    conversation.complete_channels(&mut session, &list, Ok(vec![]));
    let create = conversation.create(&session, "New").unwrap();
    let (confirmed, history) = conversation.complete_create(
        &mut session,
        &create,
        Ok(Channel {
            id: "3".into(),
            name: "New".into(),
        }),
        100,
    );
    assert!(!confirmed);
    assert!(history.is_none());
    assert!(session.generation.is_none());
    assert!(conversation.channels.is_none());
    assert!(conversation.selected.is_none());
}

#[test]
fn older_pages_merge_identity_in_server_order_retry_and_stop_at_exhaustion() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let channels = view.start(&session).unwrap();
    let first = view
        .complete_channels(&mut session, &channels, Ok(super::tests::channels()))
        .unwrap();
    let message = |id: &str| Message {
        id: id.into(),
        channel_id: "z".into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: id.into(),
        created_at: "same".into(),
    };
    view.complete_history(
        &mut session,
        &first,
        Ok(Page {
            items: vec![message("5"), message("4"), message("3")],
            next_cursor: Some("opaque one".into()),
        }),
    );
    let older = view.request_older(&session).unwrap();
    assert_eq!(older.before.as_deref(), Some("opaque one"));
    assert!(view.request_older(&session).is_none());
    view.complete_history(&mut session, &older, Err(ApiError::Unavailable));
    assert_eq!(
        view.history.get("z").as_ref().unwrap(),
        &&Load::Ready(vec![message("5"), message("4"), message("3")])
    );
    assert!(matches!(view.older.get("z"), Some(Older::Failed(_))));
    assert!(view.request_older(&session).is_none());
    let retry = view.retry_older(&session).unwrap();
    assert_eq!(retry.before, older.before);
    assert_eq!(
        view.complete_history(
            &mut session,
            &older,
            Ok(Page {
                items: vec![message("2")],
                next_cursor: None
            })
        )
        .added,
        0
    );
    assert_eq!(
        view.complete_history(
            &mut session,
            &retry,
            Ok(Page {
                items: vec![message("3"), message("2"), message("1"), message("2")],
                next_cursor: Some("opaque two".into()),
            })
        )
        .added,
        2
    );
    let final_read = view.request_older(&session).unwrap();
    assert_eq!(final_read.before.as_deref(), Some("opaque two"));
    assert_eq!(
        view.complete_history(
            &mut session,
            &final_read,
            Ok(Page {
                items: vec![message("1"), message("0")],
                next_cursor: None,
            })
        )
        .added,
        1
    );
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(
            ["5", "4", "3", "2", "1", "0"].map(message).to_vec()
        ))
    );
    assert_eq!(view.older.get("z"), Some(&Older::Exhausted));
    assert!(view.request_older(&session).is_none());
}

#[test]
fn switching_or_invalidating_session_discards_older_completions() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let channels = view.start(&session).unwrap();
    let first = view
        .complete_channels(&mut session, &channels, Ok(super::tests::channels()))
        .unwrap();
    view.complete_history(
        &mut session,
        &first,
        Ok(Page {
            items: vec![],
            next_cursor: Some("cursor".into()),
        }),
    );
    let old = view.request_older(&session).unwrap();
    let other = view.select(&session, "a").unwrap();
    view.complete_history(&mut session, &old, Err(ApiError::AlreadyInvalid));
    assert!(session.generation.is_some());
    assert_eq!(view.selected.as_deref(), Some("a"));
    view.complete_history(
        &mut session,
        &other,
        Ok(Page {
            items: vec![],
            next_cursor: Some("other cursor".into()),
        }),
    );
    let pending = view.request_older(&session).unwrap();
    session.generation = None;
    view.clear();
    view.complete_history(
        &mut session,
        &pending,
        Ok(Page {
            items: vec![],
            next_cursor: None,
        }),
    );
    assert!(view.history.is_empty());
    assert!(view.older.is_empty());
}

#[test]
fn creating_a_channel_cancels_the_old_selected_page_and_allows_reselection() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let channels = view.start(&session).unwrap();
    let first = view
        .complete_channels(&mut session, &channels, Ok(super::tests::channels()))
        .unwrap();
    view.complete_history(
        &mut session,
        &first,
        Ok(Page {
            items: vec![],
            next_cursor: Some("opaque server cursor".into()),
        }),
    );
    let pending = view.request_older(&session).unwrap();
    let create = view.create(&session, "New").unwrap();
    let (confirmed, next) = view.complete_create(
        &mut session,
        &create,
        Ok(Channel {
            id: "new".into(),
            name: "New".into(),
        }),
        0,
    );
    assert!(confirmed);
    assert_eq!(next.unwrap().channel_id.as_deref(), Some("new"));
    assert_eq!(view.older.get("z"), Some(&Older::Available));
    view.select(&session, "z");
    let retry = view
        .request_older(&session)
        .expect("canceled traversal is retryable");
    assert_eq!(retry.before.as_deref(), Some("opaque server cursor"));
    assert_eq!(
        view.complete_history(&mut session, &pending, Err(ApiError::AlreadyInvalid))
            .added,
        0
    );
    assert!(session.generation.is_some());
}

#[test]
fn selected_older_page_rejection_invalidates_session_and_clears_history() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let channels = view.start(&session).unwrap();
    let first = view
        .complete_channels(&mut session, &channels, Ok(super::tests::channels()))
        .unwrap();
    view.complete_history(
        &mut session,
        &first,
        Ok(Page {
            items: vec![],
            next_cursor: Some("opaque".into()),
        }),
    );
    let older = view.request_older(&session).unwrap();
    view.complete_history(&mut session, &older, Err(ApiError::AlreadyInvalid));
    assert!(session.generation.is_none());
    assert!(view.history.is_empty());
    assert!(view.older.is_empty());
    assert!(view.selected.is_none());
}

#[test]
fn catchup_waits_for_overlap_across_opaque_pages_and_recovers_after_failure() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let list = view.start(&session).unwrap();
    let initial = view
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    let message = |id: &str| Message {
        id: id.into(),
        channel_id: "z".into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: id.into(),
        created_at: "same".into(),
    };
    let page = |ids: &[&str], cursor: Option<&str>| Page {
        items: ids.iter().map(|id| message(id)).collect(),
        next_cursor: cursor.map(str::to_owned),
    };
    view.complete_history(
        &mut session,
        &initial,
        Ok(page(&["3", "2", "1"], Some("older"))),
    );
    let refresh = view.refresh_history(&session).unwrap();
    assert!(view.refresh_history(&session).is_none());
    assert!(view.request_older(&session).is_none());
    let pending = view
        .complete_history(
            &mut session,
            &refresh,
            Ok(page(&["9", "8"], Some("opaque /?+"))),
        )
        .next
        .unwrap();
    assert_eq!(pending.before.as_deref(), Some("opaque /?+"));
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(page(&["3", "2", "1"], None).items))
    );
    assert_eq!(view.refreshing.get("z"), Some(&Refresh::Running));
    view.complete_history(&mut session, &pending, Err(ApiError::Unavailable));
    assert!(matches!(
        view.refreshing.get("z"),
        Some(Refresh::Incomplete(_))
    ));
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(page(&["3", "2", "1"], None).items))
    );
    let retry = view.refresh_history(&session).unwrap();
    assert_eq!(
        view.complete_history(&mut session, &pending, Ok(page(&["7"], None)))
            .added,
        0
    );
    let middle = view
        .complete_history(
            &mut session,
            &retry,
            Ok(page(&["9", "8"], Some("opaque /?+"))),
        )
        .next
        .unwrap();
    let last = view
        .complete_history(
            &mut session,
            &middle,
            Ok(page(&["8", "7", "6"], Some("next"))),
        )
        .next
        .unwrap();
    assert_eq!(last.before.as_deref(), Some("next"));
    let outcome = view.complete_history(
        &mut session,
        &last,
        Ok(page(&["6", "3", "2"], Some("unused"))),
    );
    assert!(outcome.next.is_none());
    assert_eq!(outcome.added, 4);
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(
            page(&["9", "8", "7", "6", "3", "2", "1"], None).items
        ))
    );
    assert!(!view.refreshing.contains_key("z"));
    assert_eq!(view.older.get("z"), Some(&Older::Available));
}

#[test]
fn catchup_from_an_empty_loaded_conversation_waits_until_exhaustion() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let list = view.start(&session).unwrap();
    let initial = view
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    view.complete_history(
        &mut session,
        &initial,
        Ok(Page {
            items: vec![],
            next_cursor: None,
        }),
    );
    let message = |id: &str| Message {
        id: id.into(),
        channel_id: "z".into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: id.into(),
        created_at: "same".into(),
    };
    let refresh = view.refresh_history(&session).unwrap();
    let next = view.complete_history(
        &mut session,
        &refresh,
        Ok(Page {
            items: vec![message("4"), message("3")],
            next_cursor: Some("server cursor".into()),
        }),
    );
    assert_eq!(next.added, 0);
    assert_eq!(view.history.get("z"), Some(&Load::Ready(vec![])));
    let continuation = next.next.unwrap();
    view.complete_history(&mut session, &continuation, Err(ApiError::Unavailable));
    assert_eq!(view.history.get("z"), Some(&Load::Ready(vec![])));
    assert!(matches!(
        view.refreshing.get("z"),
        Some(Refresh::Incomplete(_))
    ));
    let retry = view.refresh_history(&session).unwrap();
    let continuation = view
        .complete_history(
            &mut session,
            &retry,
            Ok(Page {
                items: vec![message("4"), message("3")],
                next_cursor: Some("server cursor".into()),
            }),
        )
        .next
        .unwrap();
    let outcome = view.complete_history(
        &mut session,
        &continuation,
        Ok(Page {
            items: vec![message("3"), message("2"), message("1")],
            next_cursor: None,
        }),
    );
    assert_eq!(outcome.added, 4);
    assert!(outcome.next.is_none());
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(vec![
            message("4"),
            message("3"),
            message("2"),
            message("1"),
        ]))
    );
    assert!(!view.refreshing.contains_key("z"));
}

#[test]
fn refresh_supersedes_an_older_read_without_losing_its_cursor() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let list = view.start(&session).unwrap();
    let first = view
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    let message = Message {
        id: "1".into(),
        channel_id: "z".into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: "one".into(),
        created_at: "same".into(),
    };
    view.complete_history(
        &mut session,
        &first,
        Ok(Page {
            items: vec![message.clone()],
            next_cursor: Some("opaque older".into()),
        }),
    );
    let older = view.request_older(&session).unwrap();
    let refresh = view.refresh_history(&session).unwrap();
    assert_eq!(view.older.get("z"), Some(&Older::Available));
    assert_eq!(
        view.complete_history(&mut session, &older, Err(ApiError::AlreadyInvalid))
            .added,
        0
    );
    assert!(session.generation.is_some());
    view.complete_history(
        &mut session,
        &refresh,
        Ok(Page {
            items: vec![message.clone()],
            next_cursor: Some("opaque newer".into()),
        }),
    );
    assert_eq!(view.history.get("z"), Some(&Load::Ready(vec![message])));
    assert_eq!(
        view.request_older(&session).unwrap().before.as_deref(),
        Some("opaque older")
    );
}

#[test]
fn disconnected_exhaustion_keeps_contiguous_old_history_and_switch_cancels() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let list = view.start(&session).unwrap();
    let first = view
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    let m = |id: &str| Message {
        id: id.into(),
        channel_id: "z".into(),
        author_id: "u".into(),
        author_name: "Ada".into(),
        text: id.into(),
        created_at: "same".into(),
    };
    view.complete_history(
        &mut session,
        &first,
        Ok(Page {
            items: vec![m("2"), m("1")],
            next_cursor: None,
        }),
    );
    let refresh = view.refresh_history(&session).unwrap();
    view.complete_history(
        &mut session,
        &refresh,
        Ok(Page {
            items: vec![m("9"), m("1")],
            next_cursor: None,
        }),
    );
    assert_eq!(
        view.history.get("z"),
        Some(&Load::Ready(vec![m("2"), m("1")]))
    );
    assert!(matches!(
        view.refreshing.get("z"),
        Some(Refresh::Incomplete(_))
    ));
    let pending = view.refresh_history(&session).unwrap();
    view.select(&session, "a");
    view.complete_history(&mut session, &pending, Err(ApiError::AlreadyInvalid));
    assert!(session.generation.is_some());
    assert!(!view.refreshing.contains_key("z"));
}

#[test]
fn discovery_preserves_selection_handles_empty_and_rejects_stale_results() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let first = view.start(&session).unwrap();
    view.complete_channels(&mut session, &first, Ok(channels()));
    view.select(&session, "a");
    let refresh = view.refresh_channels(&session).unwrap();
    assert!(view.refresh_channels(&session).is_none());
    view.complete_channels(&mut session, &refresh, Err(ApiError::Unavailable));
    assert_eq!(view.selected.as_deref(), Some("a"));
    assert!(matches!(view.channels, Some(Load::Ready(_))));
    assert!(view.channel_error.is_some());
    let refresh = view.refresh_channels(&session).unwrap();
    view.complete_channels(
        &mut session,
        &refresh,
        Ok(vec![
            Channel {
                id: "new".into(),
                name: "New".into(),
            },
            channels()[1].clone(),
        ]),
    );
    assert_eq!(view.selected.as_deref(), Some("a"));
    let empty = view.refresh_channels(&session).unwrap();
    view.complete_channels(&mut session, &empty, Ok(vec![]));
    assert_eq!(view.selected, None);
    assert_eq!(view.channels, Some(Load::Ready(vec![])));
    view.complete_channels(&mut session, &refresh, Err(ApiError::AlreadyInvalid));
    assert!(session.generation.is_some());
}

#[test]
fn protected_rejection_clears_selection_and_cached_conversation() {
    let mut session = logged_in();
    let mut view = Conversation::default();
    let list = view.start(&session).unwrap();
    let read = view
        .complete_channels(&mut session, &list, Ok(channels()))
        .unwrap();
    view.complete_history(&mut session, &read, Err(ApiError::AlreadyInvalid));
    assert!(session.generation.is_none());
    assert!(view.selected.is_none());
    assert!(view.channels.is_none());
    assert!(view.history.is_empty());
}
