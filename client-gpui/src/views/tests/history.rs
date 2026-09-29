use super::*;

// This is a real Kit/GPUI history surface, not a simulated scrolling model.
struct HistoryProbe {
    list: ListState,
    messages: Vec<(String, SharedString)>,
    focus: FocusHandle,
}

impl HistoryProbe {
    fn new(cx: &mut Context<Self>) -> Self {
        let messages = (0..40)
            .map(|ix| {
                (
                    format!("original-{ix}"),
                    if ix % 2 == 0 {
                        "first line\nsecond line with enough words to wrap at a narrow width"
                    } else {
                        "short line"
                    }
                    .into(),
                )
            })
            .collect();
        Self {
            list: ListState::new(40, ListAlignment::Top, px(0.)).measure_all(),
            messages,
            focus: cx.focus_handle(),
        }
    }

    fn prepend(&mut self, cx: &mut Context<Self>) {
        self.messages.splice(
            0..0,
            [
                ("older-0".into(), "older\nfirst line".into()),
                ("older-1".into(), "older short".into()),
            ],
        );
        self.list.splice(0..0, 2);
        cx.notify();
    }
}

impl Render for HistoryProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let messages = self.messages.clone();
        let focus = self.focus.clone();
        div().w(px(260.)).h(px(180.)).child(
            div()
                .id("history")
                .test_support()
                .track_focus(&self.focus)
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.focus(&focus, cx)
                })
                .size_full()
                .child(
                    list(self.list.clone(), move |ix, _, _| {
                        let (id, text) = &messages[ix];
                        div()
                            .id(format!("message-{id}"))
                            .test_support()
                            .w_full()
                            .p_2()
                            .child(SelectableText::new(
                                format!("selectable-{id}"),
                                text.clone(),
                            ))
                            .into_any_element()
                    })
                    .size_full(),
                ),
        )
    }
}

#[gpui_kit::test]
fn varied_height_history_preserves_reader_on_prepend(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let probe = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored = probe.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(HistoryProbe::new);
        *stored.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<HistoryProbe> = probe.borrow().as_ref().unwrap().clone();
    cx.update(|window, cx| {
        window.render_frame(cx);
        for _ in 0..8 {
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
                    gpui_kit::px(0.),
                    gpui_kit::px(-90.),
                )),
                cx,
            );
        }
        let before = view.read(cx).list.logical_scroll_top();
        assert!(
            before.item_ix > 0,
            "the actual wheel must scroll variable-height rows"
        );
        let id = format!("message-original-{}", before.item_ix);
        let y = window.find(id.clone()).bounds().origin.y;
        view.update(cx, |view, cx| view.prepend(cx));
        window.render_frame(cx);
        let after = view.read(cx).list.logical_scroll_top();
        assert_eq!(after.item_ix, before.item_ix + 2);
        assert_eq!(after.offset_in_item, before.offset_in_item);
        assert_eq!(window.find(id).bounds().origin.y, y);
    });
}

#[gpui_kit::test]
fn selectable_history_copies_line_breaks(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(HistoryProbe::new);
        Root::new(view, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        let bounds = window.find("message-original-0").bounds();
        window.drag(
            bounds.origin + gpui_kit::point(gpui_kit::px(2.), gpui_kit::px(12.)),
            bounds.origin
                + gpui_kit::point(
                    bounds.size.width - gpui_kit::px(2.),
                    bounds.size.height - gpui_kit::px(2.),
                ),
            cx,
        );
        assert!(gpui_kit::base::TextSelection::selected_text(window, cx).contains('\n'));
        window.press("ctrl-c", cx);
    });
    assert!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap()
            .contains('\n')
    );
}

#[gpui_kit::test]
fn production_wheel_requests_older_and_keeps_reader_at_same_viewport_y(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (tx, requests) = std::sync::mpsc::channel();
    let stored = std::rc::Rc::new(std::cell::RefCell::new(None));
    let saved = stored.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PagedAuth(tx))));
        *saved.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = stored.borrow().as_ref().unwrap().clone();
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
    let message = |ix: i32| crate::conversation::Message {
        id: ix.to_string(),
        channel_id: "000000000000001".into(),
        author_id: "42".into(),
        author_name: "Ada".into(),
        text: if ix % 2 == 0 {
            "a long line that wraps at narrow widths\nwith another line".into()
        } else {
            "short".into()
        },
        created_at: "same".into(),
    };
    reply
        .send_blocking(Ok(crate::conversation::Page {
            items: (1..=40).rev().map(message).collect(),
            next_cursor: Some("server cursor only".into()),
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(view.read(cx).history_list.logical_scroll_top().item_ix > 1);
        // Find the threshold by scrolling the actual list, not by fixing screen coordinates.
        for _ in 0..100 {
            if view.read(cx).history_list.logical_scroll_top().item_ix <= 1 {
                break;
            }
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(90.))),
                cx,
            );
            window.render_frame(cx);
        }
        assert!(
            view.read(cx).history_list.logical_scroll_top().item_ix <= 1,
            "offset={:?} end={:?}",
            view.read(cx).history_list.logical_scroll_top(),
            view.read(cx).history_list.is_scrolled_to_end()
        );
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
        let ix = view.read(cx).history_list.logical_scroll_top().item_ix;
        let id = format!("message-{}", ix + 1);
        *saved_anchor.borrow_mut() = Some((id.clone(), window.find(id).bounds().origin.y, ix));
    });
    reply.send_blocking(Err(AuthError::Unavailable)).unwrap();
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
        let ix = view.read(cx).history_list.logical_scroll_top().item_ix;
        let id = format!("message-{}", ix + 1);
        *anchor.borrow_mut() = Some((id.clone(), window.find(id).bounds().origin.y, ix));
    });
    retry
        .send_blocking(Ok(crate::conversation::Page {
            items: vec![message(1), message(0), message(-1)],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let (id, y, ix) = anchor.borrow().clone().unwrap();
        assert_eq!(
            view.read(cx).history_list.logical_scroll_top().item_ix,
            ix + 2
        );
        assert_eq!(window.find(id).bounds().origin.y, y);
        assert!(requests.try_recv().is_err());
    });
}

#[gpui_kit::test]
fn confirmed_middle_insertion_keeps_reader_anchor(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (tx, requests) = std::sync::mpsc::channel();
    let stored = std::rc::Rc::new(std::cell::RefCell::new(None));
    let saved = stored.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PagedAuth(tx))));
        *saved.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = stored.borrow().as_ref().unwrap().clone();
    let m = |ix: i32| crate::conversation::Message {
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
        .send_blocking(Ok(crate::conversation::Page {
            items: original.clone(),
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.history_list.scroll_to(gpui_kit::ListOffset {
                item_ix: 23,
                offset_in_item: px(0.),
            });
            v.conversation.set_draft("000000000000001", "twenty".into());
            let send = v.conversation.send(&v.session).unwrap();
            assert_eq!(
                v.conversation
                    .complete_send(&mut v.session, &send, Ok(m(20)), 0),
                crate::conversation::SendOutcome::Confirmed
            );
            let read = v.conversation.reconcile_confirmed(&v.session).unwrap();
            v.load_history(read, cx);
        });
        window.render_frame(cx);
        assert_eq!(view.read(cx).history_list.logical_scroll_top().item_ix, 23);
        assert!(window.find("message-25").bounds().size.height > px(0.));
    });
    cx.run_until_parked();
    let (_, refresh) = requests
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap();
    let y = cx.update(|window, cx| {
        window.render_frame(cx);
        window.find("message-25").bounds().origin.y
    });
    refresh
        .send_blocking(Ok(crate::conversation::Page {
            items: original,
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).history_list.logical_scroll_top().item_ix, 24);
        assert_eq!(window.find("message-25").bounds().origin.y, y);
        assert_eq!(view.read(cx).history_list.item_count(), 40);
        let crate::conversation::Load::Ready(messages) = view
            .read(cx)
            .conversation
            .history
            .get("000000000000001")
            .unwrap()
        else {
            panic!("history not ready")
        };
        assert_eq!(messages.iter().filter(|m| m.id == "20").count(), 1);
    });
}

#[gpui_kit::test]
fn refresh_controls_preserve_reader_and_jump_follows_later_messages(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (tx, requests) = std::sync::mpsc::channel();
    let stored = std::rc::Rc::new(std::cell::RefCell::new(None));
    let saved = stored.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(PagedAuth(tx))));
        *saved.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = stored.borrow().as_ref().unwrap().clone();
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
    let m = |ix: i32| crate::conversation::Message {
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
        created_at: "same".into(),
    };
    reply
        .send_blocking(Ok(crate::conversation::Page {
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
        assert_eq!(view.read(cx).history_list.is_scrolled_to_end(), Some(false));
        window.click("refresh-history", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("refresh-history").label(),
            Some("Refreshing conversation…")
        );
        window.click("refresh-history", cx);
    });
    cx.run_until_parked();
    let (before, first) = requests
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap();
    assert_eq!(before, None);
    assert!(requests.try_recv().is_err());
    first
        .send_blocking(Ok(crate::conversation::Page {
            items: (61..=70).rev().map(m).collect(),
            next_cursor: Some("server opaque".into()),
        }))
        .unwrap();
    cx.run_until_parked();
    let (before, second) = requests
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap();
    assert_eq!(before.as_deref(), Some("server opaque"));
    let anchor = std::rc::Rc::new(std::cell::RefCell::new(None));
    let saved_anchor = anchor.clone();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            view.read(cx)
                .conversation
                .refreshing
                .contains_key("000000000000001")
        );
        let ix = view.read(cx).history_list.logical_scroll_top().item_ix;
        let id = format!("message-{}", ix + 1);
        *saved_anchor.borrow_mut() = Some((id.clone(), window.find(id).bounds().origin.y));
    });
    second
        .send_blocking(Ok(crate::conversation::Page {
            items: (39..=60).rev().map(m).collect(),
            next_cursor: Some("unneeded".into()),
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let (id, y) = anchor.borrow().clone().unwrap();
        assert_eq!(window.find(id).bounds().origin.y, y);
        assert_eq!(view.read(cx).history_list.is_scrolled_to_end(), Some(false));
        window.click("jump-latest", cx);
        window.render_frame(cx);
        assert_eq!(view.read(cx).history_list.is_scrolled_to_end(), Some(true));
        assert!(window.find("message-70").bounds().size.height > px(0.));
        window.click("refresh-history", cx);
    });
    cx.run_until_parked();
    let (_, latest) = requests
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap();
    latest
        .send_blocking(Ok(crate::conversation::Page {
            items: [71, 70, 69].map(m).to_vec(),
            next_cursor: Some("still older".into()),
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).history_list.is_scrolled_to_end(), Some(true));
        assert!(window.find("message-71").bounds().size.height > px(0.));
    });
}

#[gpui_kit::test]
fn production_message_is_selectable_and_copyable(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
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
