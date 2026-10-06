use super::*;

fn mount(
    cx: &mut TestAppContext,
    calls: Arc<Mutex<Vec<Sent>>>,
) -> &mut gpui_kit::VisualTestContext {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(window, cx, Arc::new(SendAuth(calls)));
        Root::new(view, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    cx
}

#[gpui_kit::test]
fn composer_uses_real_textarea_keyboard_focus_and_channel_drafts(cx: &mut TestAppContext) {
    let calls = Arc::new(Mutex::new(Vec::<Sent>::new()));
    let cx = mount(cx, calls.clone());
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("first", cx);
        window.press("shift-enter", cx);
        window.input("second", cx);
        assert_eq!(composer_text(window, cx), "first\nsecond");
        window.click("channel-000000000000002", cx);
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("other", cx);
        window.click("channel-000000000000001", cx);
        assert_eq!(composer_text(window, cx), "first\nsecond");
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
        assert!(window.try_find("send-message").is_none());
        assert_eq!(composer_text(window, cx), "first\nsecond");
        window.input("blocked", cx);
        assert_eq!(composer_text(window, cx), "first\nsecond");
        window.click("composer", cx);
        window.dispatch_action(
            Box::new(gpui_kit::base::input::Enter {
                secondary: false,
                shift: false,
            }),
            cx,
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
    assert_eq!(calls.lock().unwrap().len(), 1);
    let (_, text, sender) = calls.lock().unwrap().remove(0);
    assert_eq!(text, "first\nsecond");
    sender.try_send(Err(ApiError::InvalidInput)).unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(composer_text(window, cx), "first\nsecond");
        assert!(
            window
                .find("send-feedback")
                .label()
                .unwrap()
                .contains("rejected")
        );
        window.click("channel-000000000000002", cx);
        assert_eq!(composer_text(window, cx), "other");
        window.click("composer", cx);
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
    assert_eq!(text, "other");
    sender
        .try_send(Ok(crate::api::Message {
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
        assert_eq!(composer_text(window, cx), "");
        assert_eq!(
            window.find("message-000000000000003").label(),
            Some("other")
        );
        window.click("channel-000000000000001", cx);
        assert_eq!(composer_text(window, cx), "first\nsecond");
        assert!(window.try_find("send-message").is_none());
        window.click("logout", cx);
        window.render_frame(cx);
        assert!(window.try_find("composer").is_none());
    });
}

#[gpui_kit::test]
fn enter_at_mid_caret_and_after_shift_enter_sends_unchanged_text(cx: &mut TestAppContext) {
    let calls = Arc::new(Mutex::new(Vec::<Sent>::new()));
    let cx = mount(cx, calls.clone());
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("middle", cx);
        for _ in 0..3 {
            window.press("left", cx);
        }
        // Verify the actual caret with a selection/copy, then restore it before Enter.
        window.press("shift-home", cx);
        window.press("ctrl-c", cx);
        assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "mid");
        window.press("right", cx);
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
    reply.try_send(Err(ApiError::InvalidInput)).unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(composer_text(window, cx), "middle");
        window.press("ctrl-a", cx);
        window.input("tail", cx);
        window.press("shift-enter", cx);
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
    reply.try_send(Err(ApiError::InvalidInput)).unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(composer_text(window, cx), "tail\n");
    });
}

#[gpui_kit::test]
fn uncertain_send_retains_originating_draft_and_never_replays(cx: &mut TestAppContext) {
    let calls = Arc::new(Mutex::new(Vec::<Sent>::new()));
    let cx = mount(cx, calls.clone());
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("maybe published", cx);
        window.click("composer", cx);
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
    let (_, _, sender) = calls.lock().unwrap().remove(0);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("channel-000000000000002", cx);
        window.render_frame(cx);
        assert!(window.try_find("send-message").is_none());
    });
    sender.try_send(Err(ApiError::Unavailable)).unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("send-feedback").is_none());
        assert!(window.try_find("refresh-history").is_none());
        window.click("channel-000000000000001", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(composer_text(window, cx), "maybe published");
        assert!(
            window
                .find("send-feedback")
                .label()
                .unwrap()
                .contains("may already")
        );
        assert!(window.try_find("refresh-history").is_none());
        assert!(
            calls.lock().unwrap().is_empty(),
            "navigation must not replay the write"
        );
    });
}

#[gpui_kit::test]
fn stalled_send_times_out_and_late_completion_cannot_clear_the_draft(cx: &mut TestAppContext) {
    let calls = Arc::new(Mutex::new(Vec::<Sent>::new()));
    let cx = mount(cx, calls.clone());
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("timeout draft", cx);
        window.click("composer", cx);
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
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(composer_text(window, cx), "timeout draft");
        assert!(window.try_find("send-message").is_none());
        assert!(
            window
                .find("send-feedback")
                .label()
                .unwrap()
                .contains("may already")
        );
    });
    let (_, _, sender) = calls.lock().unwrap().remove(0);
    assert!(sender.try_send(Err(ApiError::AlreadyInvalid)).is_err());
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(composer_text(window, cx), "timeout draft");
        assert!(
            window
                .find("session-status")
                .label()
                .unwrap()
                .contains("Ada")
        );
    });
}
