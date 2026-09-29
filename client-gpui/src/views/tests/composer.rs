use super::*;

#[gpui_kit::test]
fn composer_uses_real_textarea_keyboard_button_focus_and_channel_drafts(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let calls = Arc::new(std::sync::Mutex::new(Vec::<Sent>::new()));
    let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored = saved.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(SendAuth(calls.clone()))));
        *stored.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        assert!(
            view.read(cx)
                .composer
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        window.input("first", cx);
        window.press("shift-enter", cx);
        window.input("second", cx);
        assert_eq!(
            view.read(cx).composer.read(cx).text().to_string(),
            "first\nsecond"
        );
        window.click("channel-000000000000002", cx);
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("other", cx);
        window.click("channel-000000000000001", cx);
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).composer.read(cx).text().to_string(),
            "first\nsecond"
        );
        window.click("composer", cx);
        assert!(
            view.read(cx)
                .composer
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        window.dispatch_action(
            Box::new(gpui_kit::base::input::Enter {
                secondary: false,
                shift: false,
            }),
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("send-message").label(), Some("Sending…"));
        assert_eq!(
            view.read(cx).composer.read(cx).text().to_string(),
            "first\nsecond"
        );
        window.input("blocked", cx);
        assert_eq!(
            view.read(cx).composer.read(cx).text().to_string(),
            "first\nsecond"
        );
        window.click("send-message", cx);
        window.dispatch_action(
            Box::new(gpui_kit::base::input::Enter {
                secondary: false,
                shift: false,
            }),
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(calls.lock().unwrap().len(), 1);
    let (_, text, sender) = calls.lock().unwrap().remove(0);
    assert_eq!(text, "first\nsecond");
    sender.try_send(Err(AuthError::InvalidInput)).unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).composer.read(cx).text().to_string(),
            "first\nsecond"
        );
        assert!(
            window
                .find("send-feedback")
                .label()
                .unwrap()
                .contains("rejected")
        );
        window.click("channel-000000000000002", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "other");
        window.click("send-message", cx);
    });
    cx.run_until_parked();
    assert_eq!(calls.lock().unwrap().len(), 1);
    let (_, text, sender) = calls.lock().unwrap().remove(0);
    assert_eq!(text, "other");
    sender
        .try_send(Ok(crate::conversation::Message {
            id: "000000000000003".into(),
            channel_id: "000000000000002".into(),
            author_id: "42".into(),
            author_name: "Ada".into(),
            text: "other".into(),
            created_at: "2027-01-01T00:00:00Z".into(),
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "");
        assert_eq!(
            window.find("message-000000000000003").label(),
            Some("other")
        );
        window.click("channel-000000000000001", cx);
        window.render_frame(cx);
        assert_eq!(window.find("send-message").label(), Some("Send message"));
        assert_eq!(
            view.read(cx).composer.read(cx).text().to_string(),
            "first\nsecond"
        );
        window.click("logout", cx);
        assert!(view.read(cx).conversation.drafts.is_empty());
    });
}

#[gpui_kit::test]
fn enter_at_mid_caret_and_after_shift_enter_sends_unchanged_text(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let calls = Arc::new(std::sync::Mutex::new(Vec::<Sent>::new()));
    let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored = saved.clone();
    let sender = calls.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(SendAuth(sender))));
        *stored.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("middle", cx);
        view.update(cx, |v, cx| {
            v.composer.update(cx, |input, cx| {
                input.set_cursor_position(
                    gpui_kit::base::input::Position {
                        line: 0,
                        character: 3,
                    },
                    window,
                    cx,
                );
            });
        });
        assert_eq!(
            view.read(cx).composer.read(cx).cursor_position().character,
            3
        );
        assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "middle");
        window.dispatch_event(
            gpui_kit::PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
                keystroke: gpui_kit::Keystroke::parse("enter").unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
    });
    cx.run_until_parked();
    let (_, text, reply) = calls.lock().unwrap().remove(0);
    assert_eq!(text, "middle");
    reply.try_send(Err(AuthError::InvalidInput)).unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.composer
                .update(cx, |input, cx| input.set_value("tail", window, cx));
            v.conversation.set_draft("000000000000001", "tail".into());
        });
        window.render_frame(cx);
        window.click("composer", cx);
        window.press("shift-enter", cx);
        assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "tail\n");
        window.dispatch_event(
            gpui_kit::PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
                keystroke: gpui_kit::Keystroke::parse("enter").unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
    });
    cx.run_until_parked();
    let (_, text, reply) = calls.lock().unwrap().remove(0);
    assert_eq!(text, "tail\n");
    reply.try_send(Err(AuthError::InvalidInput)).unwrap();
    cx.run_until_parked();
    cx.update(|_window, cx| {
        assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "tail\n");
    });
}

#[gpui_kit::test]
fn uncertain_send_retains_draft_refreshes_only_selected_channel_and_never_replays(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let calls = Arc::new(std::sync::Mutex::new(Vec::<Sent>::new()));
    let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored = saved.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(SendAuth(calls.clone()))));
        *stored.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("maybe published", cx);
        window.click("send-message", cx);
    });
    cx.run_until_parked();
    assert_eq!(calls.lock().unwrap().len(), 1);
    let (_, _, sender) = calls.lock().unwrap().remove(0);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("channel-000000000000002", cx);
        window.render_frame(cx);
        assert_eq!(window.find("send-message").label(), Some("Send message"));
    });
    sender.try_send(Err(AuthError::Unavailable)).unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            view.read(cx)
                .conversation
                .uncertain
                .contains("000000000000001")
        );
        assert!(view.read(cx).conversation.refreshing.is_empty());
        window.click("channel-000000000000001", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).composer.read(cx).text().to_string(),
            "maybe published"
        );
        assert!(
            window
                .find("send-feedback")
                .label()
                .unwrap()
                .contains("may already")
        );
        assert!(
            !view
                .read(cx)
                .conversation
                .uncertain
                .contains("000000000000001")
        );
        assert!(
            calls.lock().unwrap().is_empty(),
            "refresh must not replay the write"
        );
    });
}

#[gpui_kit::test]
fn stalled_send_times_out_and_late_completion_cannot_clear_the_draft(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let calls = Arc::new(std::sync::Mutex::new(Vec::<Sent>::new()));
    let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored = saved.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(SendAuth(calls.clone()))));
        *stored.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("timeout draft", cx);
        window.click("send-message", cx);
    });
    cx.run_until_parked();
    assert_eq!(calls.lock().unwrap().len(), 1);
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(10));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("send-message").label(), Some("Send message"));
        assert_eq!(
            view.read(cx).composer.read(cx).text().to_string(),
            "timeout draft"
        );
        assert!(
            window
                .find("send-feedback")
                .label()
                .unwrap()
                .contains("may already")
        );
        assert!(view.read(cx).conversation.send_pending.is_empty());
    });
    // The fixture's future was dropped rather than retried, so delivery is impossible.
    let (_, _, sender) = calls.lock().unwrap().remove(0);
    assert!(sender.try_send(Err(AuthError::AlreadyInvalid)).is_err());
    assert!(view.read_with(cx, |v, cx| v.session.read(cx).active().is_some()));
}
