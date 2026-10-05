use super::*;

fn visible_message(
    window: &Window,
    mut ids: impl Iterator<Item = i32>,
) -> (String, gpui_kit::Pixels) {
    let bounds = window.find("history").bounds();
    ids.find_map(|id| {
        let id = format!("message-{id}");
        let row = window.try_find(id.clone())?;
        (row.bounds().origin.y >= bounds.origin.y && row.bounds().bottom() <= bounds.bottom())
            .then_some((id, row.bounds().origin.y))
    })
    .expect("a fully visible message")
}

#[gpui_kit::test]
fn production_wheel_requests_older_and_keeps_reader_at_same_viewport_y(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (tx, requests) = std::sync::mpsc::channel();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(window, cx, Arc::new(PagedAuth(tx)));
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
    let (cursor, reply) = requests
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap();
    assert_eq!(cursor, None);
    let message = |ix: i32| crate::api::Message {
        id: ix.to_string(),
        channel_id: "000000000000001".into(),
        author_id: "42".into(),
        author_name: "Ada".into(),
        text: if ix % 2 == 0 {
            "a long line that wraps at narrow widths\nwith another line".into()
        } else {
            "short".into()
        },
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    reply
        .send_blocking(Ok(crate::api::Page {
            items: (1..=40).rev().map(message).collect(),
            next_cursor: Some("server cursor only".into()),
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("message-1").is_none());
        // Find the first message through actual wheel input and semantic row bounds.
        for _ in 0..100 {
            if window.try_find("message-1").is_some() {
                break;
            }
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(90.))),
                cx,
            );
            window.render_frame(cx);
        }
        assert!(window.find("message-1").bounds().size.height > px(0.));
        window.scroll(
            "history",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(90.))),
            cx,
        );
    });
    cx.run_until_parked();
    let (cursor, reply) = requests
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap();
    assert_eq!(cursor.as_deref(), Some("server cursor only"));
    let anchor = std::rc::Rc::new(std::cell::RefCell::new(None));
    let saved_anchor = anchor.clone();
    cx.update(|window, cx| {
        window.render_frame(cx);
        // Repeated wheel input cannot start a second in-flight request.
        window.scroll(
            "history",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(90.))),
            cx,
        );
        window.render_frame(cx);
        assert!(requests.try_recv().is_err());
        *saved_anchor.borrow_mut() = Some(visible_message(window, 1..=40));
    });
    reply.send_blocking(Err(ApiError::Unavailable)).unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("retry-older").label(),
            Some("Retry older messages")
        );
        window.scroll(
            "history",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(90.))),
            cx,
        );
        window.render_frame(cx);
        assert!(
            requests.try_recv().is_err(),
            "failed pages must not retry on wheel input"
        );
        window.click("retry-older", cx);
    });
    cx.run_until_parked();
    let (cursor, retry) = requests
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap();
    assert_eq!(cursor.as_deref(), Some("server cursor only"));
    cx.update(|window, cx| {
        window.render_frame(cx);
        *anchor.borrow_mut() = Some(visible_message(window, 1..=40));
    });
    retry
        .send_blocking(Ok(crate::api::Page {
            items: vec![message(1), message(0), message(-1)],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let (id, y) = anchor.borrow().clone().unwrap();
        assert_eq!(window.find(id).bounds().origin.y, y);
        assert!(requests.try_recv().is_err());
    });
}

#[gpui_kit::test]
fn confirmed_middle_insertion_keeps_reader_anchor(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (tx, requests) = std::sync::mpsc::channel();
    let sends = Arc::new(Mutex::new(Vec::<Sent>::new()));
    let captured = sends.clone();

    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(
            window,
            cx,
            Arc::new(RaceAuth {
                pages: tx,
                sends: captured,
            }),
        );

        Root::new(view, window, cx)
    });

    let m = |ix: i32| crate::api::Message {
        id: ix.to_string(),
        channel_id: "000000000000001".into(),
        author_id: "42".into(),
        author_name: "Ada".into(),
        text: format!("message {ix}"),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    let original: Vec<_> = (1..=40).rev().filter(|&ix| ix != 20).map(m).collect();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    let (_, initial) = requests
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap();
    initial
        .send_blocking(Ok(crate::api::Page {
            items: original.clone(),
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("twenty", cx);
        window.click("send-message", cx);
        for _ in 0..30 {
            if window.try_find("message-25").is_some() {
                break;
            }
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(40.))),
                cx,
            );
            window.render_frame(cx);
        }
        assert!(window.find("message-25").bounds().size.height > px(0.));
    });
    cx.run_until_parked();
    let (_, text, confirmation) = sends.lock().unwrap().remove(0);
    assert_eq!(text, "twenty");
    let y = cx.update(|window, cx| {
        window.render_frame(cx);
        window.find("message-25").bounds().origin.y
    });
    confirmation.try_send(Ok(m(20))).unwrap();
    assert!(
        requests.try_recv().is_err(),
        "HTTP confirmation requires no read"
    );
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("message-25").bounds().origin.y, y);
        // The inserted row is observable through the real history, not shell internals.
        for _ in 0..30 {
            if window.try_find("message-20").is_some() {
                break;
            }
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(40.))),
                cx,
            );
            window.render_frame(cx);
        }
        assert_eq!(window.find("message-20").label(), Some("message 20"));
    });
}

#[gpui_kit::test]
fn live_creations_preserve_reader_and_jump_follows_later_messages(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (tx, requests) = std::sync::mpsc::channel();
    let streams = crate::test_support::live::Streams::default();

    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_with_streams(window, cx, Arc::new(PagedAuth(tx)), &streams);

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
    let (_, reply) = requests
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap();
    let m = |ix: i32| crate::api::Message {
        id: ix.to_string(),
        channel_id: "000000000000001".into(),
        author_id: "42".into(),
        author_name: "Ada".into(),
        text: if ix % 2 == 0 {
            "multi-line\nwith wrapped content that varies height"
        } else {
            "short"
        }
        .into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    reply
        .send_blocking(Ok(crate::api::Page {
            items: (1..=40).rev().map(m).collect(),
            next_cursor: Some("older".into()),
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        for _ in 0..4 {
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(90.))),
                cx,
            );
            window.render_frame(cx);
        }
        assert!(window.try_find("message-40").is_none());
        assert!(window.try_find("refresh-history").is_none());
    });
    let anchor = cx.update(|window, cx| {
        window.render_frame(cx);
        visible_message(window, 1..=40)
    });
    for id in 41..=70 {
        streams.change(serde_json::json!({"type":"message_created","message":message_json(m(id))}));
    }
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let (id, y) = anchor.clone();
        assert_eq!(window.find(id).bounds().origin.y, y);
        assert!(window.try_find("message-70").is_none());
        window.click("jump-latest", cx);
        window.render_frame(cx);
        assert!(window.find("message-70").bounds().size.height > px(0.));
    });
    assert!(
        requests.try_recv().is_err(),
        "live creations cannot fetch history"
    );
    streams.change(serde_json::json!({"type":"message_created","message":message_json(m(71))}));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("message-71").bounds().size.height > px(0.));
    });
}

#[gpui_kit::test]
fn production_message_is_selectable_and_copyable(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(window, cx, Arc::new(BoundAuth));
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
    cx.update(|window, cx| {
        window.render_frame(cx);
        let bounds = window.find("message-text-000000000000001").bounds();
        window.drag(
            bounds.origin + gpui_kit::point(px(2.), px(2.)),
            bounds.origin
                + gpui_kit::point(bounds.size.width - px(2.), bounds.size.height - px(2.)),
            cx,
        );
        assert_eq!(
            gpui_kit::base::TextSelection::selected_text(window, cx),
            "first line\nsecond line"
        );
        window.press("ctrl-c", cx);
    });
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("first line\nsecond line")
    );
}
