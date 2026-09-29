use super::*;

struct PollAuth {
    channels: Arc<AtomicUsize>,
    history: Arc<AtomicUsize>,
    offline: Arc<AtomicBool>,
}
impl AuthApi for PollAuth {
    fn signup(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
        TestAuth.signup(s, u, p)
    }
    fn login(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
        TestAuth.login(s, u, p)
    }
    fn logout(&self, s: String, t: String) -> ApiFuture<Result<(), AuthError>> {
        TestAuth.logout(s, t)
    }
    fn channels(
        &self,
        s: String,
        t: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Channel>, AuthError>> {
        self.channels.fetch_add(1, Ordering::SeqCst);
        if self.offline.load(Ordering::SeqCst) {
            Box::pin(async { Err(AuthError::Unavailable) })
        } else {
            TestAuth.channels(s, t)
        }
    }
    fn create_channel(
        &self,
        s: String,
        t: String,
        n: String,
    ) -> ApiFuture<Result<crate::conversation::Channel, AuthError>> {
        TestAuth.create_channel(s, t, n)
    }
    fn history(
        &self,
        s: String,
        t: String,
        id: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Message>, AuthError>> {
        self.history.fetch_add(1, Ordering::SeqCst);
        if self.offline.load(Ordering::SeqCst) {
            Box::pin(async { Err(AuthError::Unavailable) })
        } else {
            TestAuth.history(s, t, id)
        }
    }
}

#[gpui_kit::test]
fn poll_catches_up_multiple_pages_without_duplicate_work_and_switch_cancels_late_read(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (tx, requests) = std::sync::mpsc::channel();
    let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored = saved.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PagedAuth(tx))));
        *stored.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
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
    let message = |id: i32| crate::conversation::Message {
        id: id.to_string(),
        channel_id: "000000000000001".into(),
        author_id: "42".into(),
        author_name: "Ada".into(),
        text: id.to_string(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    initial
        .send_blocking(Ok(crate::conversation::Page {
            items: vec![message(1)],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(4), cx)));
    cx.run_until_parked();
    let (cursor, first) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(cursor.is_none());
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("refresh-history", cx); // pending, inert
        view.update(cx, |v, cx| v.poll_at(Duration::from_secs(20), cx));
    });
    cx.run_until_parked();
    assert!(requests.try_recv().is_err());
    first
        .send_blocking(Ok(crate::conversation::Page {
            items: (53..=102).rev().map(message).collect(),
            next_cursor: Some("next".into()),
        }))
        .unwrap();
    cx.run_until_parked();
    let (cursor, second) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(cursor.as_deref(), Some("next"));
    cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(30), cx)));
    cx.run_until_parked();
    assert!(requests.try_recv().is_err());
    second
        .send_blocking(Ok(crate::conversation::Page {
            items: (1..=52).rev().map(message).collect(),
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).history_list.item_count(), 102);
        assert_eq!(window.find("message-102").label(), Some("102"));
        view.update(cx, |v, cx| v.poll_at(Duration::from_secs(35), cx));
    });
    cx.run_until_parked();
    let (_, late) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("channel-000000000000002", cx);
    });
    cx.run_until_parked();
    let (_, selected) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    // The common path now cancels the controlled read just as production does.
    assert!(
        late.send_blocking(Ok(crate::conversation::Page {
            items: vec![message(999)],
            next_cursor: None,
        }))
        .is_err()
    );
    selected
        .send_blocking(Ok(crate::conversation::Page {
            items: vec![],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).conversation.selected.as_deref(), Some("000000000000002"));
        assert!(!view.read(cx).conversation.history.contains_key("000000000000002") ||
            matches!(view.read(cx).conversation.history.get("000000000000002"), Some(crate::conversation::Load::Ready(items)) if items.is_empty()));
        assert!(matches!(view.read(cx).conversation.history.get("000000000000001"), Some(crate::conversation::Load::Ready(items)) if items.len() == 102));
    });
}

#[gpui_kit::test]
fn focus_return_during_read_catches_up_once_and_logout_stops_selected_reads(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (tx, requests) = std::sync::mpsc::channel();
    let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored = saved.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PagedAuth(tx))));
        *stored.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
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
        .send_blocking(Ok(crate::conversation::Page {
            items: vec![],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(4), cx)));
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
        .send_blocking(Ok(crate::conversation::Page {
            items: vec![],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(v.poll_time(), cx)));
    cx.run_until_parked();
    let (_, catchup) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(requests.try_recv().is_err(), "only one catch-up read");
    catchup
        .send_blocking(Ok(crate::conversation::Page {
            items: vec![],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(v.poll_time(), cx)));
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
    cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(500), cx)));
    cx.run_until_parked();
    assert!(
        requests.try_recv().is_err(),
        "no unselected or overlapping read on switch"
    );
    selected
        .send_blocking(Ok(crate::conversation::Page {
            items: vec![],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(1000), cx)));
    cx.run_until_parked();
    let (_, late) = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(
        requests.try_recv().is_err(),
        "only the selected channel is scheduled"
    );
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("logout", cx);
        view.update(cx, |v, cx| v.poll_at(Duration::from_secs(1500), cx));
    });
    // Let the stale result arrive; it must not restore the logged-out session.
    let _ = late.try_send(Ok(crate::conversation::Page {
        items: vec![],
        next_cursor: None,
    }));
    cx.run_until_parked();
    assert!(
        requests.try_recv().is_err(),
        "logout must not read any channel"
    );
    cx.update(|_, cx| {
        assert!(view.read(cx).conversation.history.is_empty());
        assert!(view.read(cx).conversation.selected.is_none());
    });
}

#[gpui_kit::test]
fn focused_polls_pause_resume_and_recover_without_losing_draft(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let channels = Arc::new(AtomicUsize::new(0));
    let history = Arc::new(AtomicUsize::new(0));
    let offline = Arc::new(AtomicBool::new(false));
    let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored = saved.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            Hamlet::new(
                window,
                cx,
                Arc::new(PollAuth {
                    channels: channels.clone(),
                    history: history.clone(),
                    offline: offline.clone(),
                }),
            )
        });
        *stored.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
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
        view.update(cx, |v, cx| v.poll_at(Duration::from_secs(2), cx));
    });
    assert_eq!(history.load(Ordering::SeqCst), 1);
    assert_eq!(channels.load(Ordering::SeqCst), 1);
    cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(4), cx)));
    cx.run_until_parked();
    assert_eq!(history.load(Ordering::SeqCst), 2);
    assert_eq!(channels.load(Ordering::SeqCst), 1);
    cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(16), cx)));
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
        assert_eq!(
            view.read(cx).conversation.draft("000000000000001"),
            "keep draft"
        );
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
    cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(200), cx)));
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
        assert_eq!(
            view.read(cx).conversation.draft("000000000000001"),
            "keep draft"
        );
    });
    assert!(history.load(Ordering::SeqCst) > before);
}
