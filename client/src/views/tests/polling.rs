use super::*;

struct PollAuth {
    channels: Arc<AtomicUsize>,
    history: Arc<AtomicUsize>,
    offline: Arc<AtomicBool>,
}
impl RequestAdapter for PollAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        let count = match request.url().path() {
            "/api/v1/channels" => Some(&self.channels),
            path if path.ends_with("/messages") => Some(&self.history),
            _ => None,
        };
        if let Some(count) = count {
            count.fetch_add(1, Ordering::SeqCst);
            if self.offline.load(Ordering::SeqCst) {
                return Box::pin(async { Err(ApiError::Unavailable) });
            }
        }
        BoundAuth.execute(request)
    }
}

#[gpui_kit::test]
fn poll_catches_up_multiple_pages_without_duplicate_work_and_switch_cancels_late_read(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (tx, requests) = std::sync::mpsc::channel();

    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(window, cx, Arc::new(PagedAuth(tx)));

        Root::new(view, window, cx)
    });

    cx.deactivate_window();
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    let (_, initial) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let message = |id: i32| crate::api::Message {
        id: id.to_string(),
        channel_id: "000000000000001".into(),
        author_id: "42".into(),
        author_name: "Ada".into(),
        text: id.to_string(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    initial
        .send_blocking(Ok(crate::api::Page {
            items: vec![message(1)],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    advance(cx, 1);
    cx.run_until_parked();
    let (cursor, first) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(cursor.is_none());
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("refresh-history", cx); // pending, inert
    });
    cx.run_until_parked();
    assert!(requests.try_recv().is_err());
    first
        .send_blocking(Ok(crate::api::Page {
            items: (53..=102).rev().map(message).collect(),
            next_cursor: Some("next".into()),
        }))
        .unwrap();
    cx.run_until_parked();
    let (cursor, second) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(cursor.as_deref(), Some("next"));
    advance(cx, 1);
    cx.run_until_parked();
    assert!(requests.try_recv().is_err());
    second
        .send_blocking(Ok(crate::api::Page {
            items: (1..=52).rev().map(message).collect(),
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("refresh-history").label(),
            Some("Refresh conversation")
        );
        assert_eq!(window.find("message-102").label(), Some("102"));
    });
    cx.run_until_parked();
    advance(cx, 3);
    let (_, late) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("channel-000000000000002", cx);
    });
    cx.run_until_parked();
    let (_, selected) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    // The common path now cancels the controlled read just as production does.
    assert!(
        late.send_blocking(Ok(crate::api::Page {
            items: vec![message(999)],
            next_cursor: None,
        }))
        .is_err()
    );
    selected
        .send_blocking(Ok(crate::api::Page {
            items: vec![],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("composer").is_some());
        assert!(window.try_find("history").is_none());
        assert!(window.try_find("message-999").is_none());
        window.click("channel-000000000000001", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("message-102").label(), Some("102"));
        // Reselection retains every page, including boundaries and the oldest row.
        for id in [53, 52, 1] {
            for _ in 0..200 {
                if window.try_find(format!("message-{id}")).is_some() {
                    break;
                }
                window.scroll(
                    "history",
                    gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(90.))),
                    cx,
                );
                window.render_frame(cx);
            }
            assert_eq!(
                window.find(format!("message-{id}")).label(),
                Some(id.to_string().as_str())
            );
        }
        assert!(window.try_find("message-999").is_none());
    });
}

#[gpui_kit::test]
fn focus_return_during_read_catches_up_once_and_logout_stops_selected_reads(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (tx, requests) = std::sync::mpsc::channel();

    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(window, cx, Arc::new(PagedAuth(tx)));

        Root::new(view, window, cx)
    });

    cx.deactivate_window();
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    let (_, initial) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    initial
        .send_blocking(Ok(crate::api::Page {
            items: vec![],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    advance(cx, 1);
    cx.run_until_parked();
    let (_, pending) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    cx.deactivate_window();
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert!(
        requests.try_recv().is_err(),
        "focus must not overlap an in-flight read"
    );
    pending
        .send_blocking(Ok(crate::api::Page {
            items: vec![],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    advance(cx, 1);
    cx.run_until_parked();
    let (_, catchup) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(requests.try_recv().is_err(), "only one catch-up read");
    catchup
        .send_blocking(Ok(crate::api::Page {
            items: vec![],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    advance(cx, 1);
    cx.run_until_parked();
    assert!(
        requests.try_recv().is_err(),
        "catch-up resets the normal interval"
    );
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("channel-000000000000002", cx);
    });
    cx.run_until_parked();
    let (_, selected) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    advance(cx, 1);
    cx.run_until_parked();
    assert!(
        requests.try_recv().is_err(),
        "no unselected or overlapping read on switch"
    );
    selected
        .send_blocking(Ok(crate::api::Page {
            items: vec![],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    advance(cx, 3);
    cx.run_until_parked();
    let (_, late) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(
        requests.try_recv().is_err(),
        "only the selected channel is scheduled"
    );
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("logout", cx);
    });
    // Let the stale result arrive; it must not restore the logged-out session.
    let _ = late.try_send(Ok(crate::api::Page {
        items: vec![],
        next_cursor: None,
    }));
    cx.run_until_parked();
    assert!(
        requests.try_recv().is_err(),
        "logout must not read any channel"
    );
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Log in"));
        assert!(window.try_find("composer").is_none());
    });
}

#[gpui_kit::test]
fn focused_polls_pause_resume_and_recover_without_losing_draft(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let channels = Arc::new(AtomicUsize::new(0));
    let history = Arc::new(AtomicUsize::new(0));
    let offline = Arc::new(AtomicBool::new(false));

    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(
            window,
            cx,
            Arc::new(PollAuth {
                channels: channels.clone(),
                history: history.clone(),
                offline: offline.clone(),
            }),
        );

        Root::new(view, window, cx)
    });

    cx.deactivate_window();
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
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
        assert_eq!(
            window.find("connection-status").label(),
            Some("Connected. Checking for new messages and channels.")
        );
        window.click("composer", cx);
        window.input("keep draft", cx);
    });
    assert_eq!(history.load(Ordering::SeqCst), 1);
    assert_eq!(channels.load(Ordering::SeqCst), 1);
    advance(cx, 1);
    cx.run_until_parked();
    assert_eq!(history.load(Ordering::SeqCst), 2);
    assert_eq!(channels.load(Ordering::SeqCst), 1);
    advance(cx, 14);
    cx.run_until_parked();
    assert_eq!(channels.load(Ordering::SeqCst), 2);
    offline.store(true, Ordering::SeqCst);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("refresh-history", cx);
        window.click("refresh-channels", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("connection-status")
                .label()
                .unwrap()
                .contains("Connection trouble")
        );
        assert!(window.find("message-000000000000001").label().is_some());
        assert_eq!(composer_text(window, cx), "keep draft");
    });
    cx.deactivate_window();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let status_node = window.find("connection-status");
        let status = status_node.label().unwrap();
        assert!(status.contains("Connection trouble"));
        assert!(status.contains("Updates paused"));
        assert!(!status.contains("Retrying"));
    });
    let before = history.load(Ordering::SeqCst);
    advance(cx, 200);
    cx.run_until_parked();
    assert_eq!(history.load(Ordering::SeqCst), before);
    offline.store(false, Ordering::SeqCst);
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("connection-status").label(),
            Some("Connected. Checking for new messages and channels.")
        );
        assert_eq!(composer_text(window, cx), "keep draft");
    });
    assert!(history.load(Ordering::SeqCst) > before);
}
