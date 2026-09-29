use super::*;

struct CreateAuth {
    results: Arc<
        std::sync::Mutex<
            std::collections::VecDeque<Result<crate::conversation::Channel, AuthError>>,
        >,
    >,
    calls: Arc<AtomicUsize>,
    empty: bool,
}
impl RequestAdapter for CreateAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, AuthError>> {
        use serde_json::json;
        if request.url().path() == "/api/v1/channels" {
            let result =
                if request.method() == reqwest::Method::POST {
                    self.calls.fetch_add(1, Ordering::SeqCst);
                    wire_response(
                        self.results.lock().unwrap().pop_front().unwrap().map(
                            |channel| json!({"id":channel.id,"name":channel.name,"type":"text"}),
                        ),
                        StatusCode::CREATED,
                    )
                } else {
                    Ok(Response::controlled(
                        StatusCode::OK,
                        if self.empty {
                            json!({"items":[]})
                        } else {
                            json!({"items":[{"id":"1","name":"alpha","type":"text"},
                    {"id":"2","name":"zebra","type":"text"}]})
                        }
                        .to_string(),
                    ))
                };
            return Box::pin(async move { result });
        }
        if request.url().path().ends_with("/messages") {
            return Box::pin(async { Ok(Response::controlled(StatusCode::OK, r#"{"items":[]}"#)) });
        }
        BoundAuth.execute(request)
    }
}

#[gpui_kit::test]
fn create_controls_confirm_order_selection_and_empty_history(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for empty in [false, true] {
        let results = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from([
            Ok(crate::conversation::Channel {
                id: "3".into(),
                name: "Middle".into(),
            }),
        ])));
        let calls = Arc::new(AtomicUsize::new(0));
        let probe = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = probe.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                Hamlet::new(
                    window,
                    cx,
                    Arc::new(CreateAuth {
                        results: results.clone(),
                        calls: calls.clone(),
                        empty,
                    }),
                )
            });
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<Hamlet> = probe.borrow().as_ref().unwrap().clone();
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
            window.click("channel-name", cx);
            window.input("  Middle  ", cx);
            window.click("create-channel", cx);
            window.render_frame(cx);
            assert_eq!(
                window.find("create-channel").label(),
                Some("Creating channel…")
            );
            assert_eq!(window.find("channel-name").value(), Some("  Middle  "));
            assert_eq!(
                view.read(cx).conversation.selected.as_deref(),
                if empty { None } else { Some("1") }
            );
            window.click("create-channel", cx);
            assert_eq!(
                view.read(cx).conversation.selected.as_deref(),
                if empty { None } else { Some("1") }
            );
        });
        cx.run_until_parked();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("channel-name").value(), Some(""));
            assert_eq!(view.read(cx).conversation.selected.as_deref(), Some("3"));
            assert_eq!(window.find("channel-3").label(), Some("# Middle"));
            assert_eq!(
                view.read(cx)
                    .conversation
                    .channels
                    .as_ref()
                    .and_then(|list| match list {
                        crate::conversation::Load::Ready(items) =>
                            Some(items.iter().map(|c| c.id.as_str()).collect::<Vec<_>>()),
                        _ => None,
                    }),
                Some(if empty {
                    vec!["3"]
                } else {
                    vec!["1", "3", "2"]
                })
            );
            assert!(
                view.read(cx).conversation.history.get("3")
                    == Some(&crate::conversation::Load::Ready(vec![]))
            );
        });
    }
}

#[gpui_kit::test]
fn create_controls_keep_input_on_errors_without_replay(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let results = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from([
        Err(AuthError::Conflict),
        Err(AuthError::Unavailable),
    ])));
    let calls = Arc::new(AtomicUsize::new(0));
    let probe = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored = probe.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            Hamlet::new(
                window,
                cx,
                Arc::new(CreateAuth {
                    results: results.clone(),
                    calls: calls.clone(),
                    empty: true,
                }),
            )
        });
        *stored.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = probe.borrow().as_ref().unwrap().clone();
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
        window.click("channel-name", cx);
        window.input("bad!", cx);
        window.click("create-channel", cx);
        window.render_frame(cx);
        assert!(
            window
                .find("channel-feedback")
                .label()
                .unwrap()
                .contains("1–64")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        window.click("channel-name", cx);
        window.press("ctrl-a", cx);
        window.input("Duplicate", cx);
        window.click("create-channel", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("channel-feedback")
                .label()
                .unwrap()
                .contains("already exists")
        );
        assert_eq!(window.find("channel-name").value(), Some("Duplicate"));
        window.click("create-channel", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("channel-feedback")
                .label()
                .unwrap()
                .contains("may have succeeded")
        );
        assert_eq!(window.find("channel-name").value(), Some("Duplicate"));
        assert_eq!(calls.load(Ordering::SeqCst), 2); // no automatic replay
        window.click("logout", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(view.read(cx).conversation.channels.is_none());
        assert!(view.read(cx).conversation.selected.is_none());
        assert_eq!(window.find("login").label(), Some("Log in"));
    });
}

#[gpui_kit::test]
fn create_completion_after_logout_cannot_navigate(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let results = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from([
        Ok(crate::conversation::Channel {
            id: "3".into(),
            name: "Late".into(),
        }),
    ])));
    let probe = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored = probe.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            Hamlet::new(
                window,
                cx,
                Arc::new(CreateAuth {
                    results,
                    calls: Arc::new(AtomicUsize::new(0)),
                    empty: true,
                }),
            )
        });
        *stored.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = probe.borrow().as_ref().unwrap().clone();
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
        window.click("channel-name", cx);
        window.input("Late", cx);
        window.click("create-channel", cx);
        window.click("logout", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Log in"));
        assert!(view.read(cx).conversation.channels.is_none());
        assert!(view.read(cx).conversation.selected.is_none());
    });
}

#[gpui_kit::test]
fn refresh_channels_control_keeps_selected_conversation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
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
        window.click("channel-000000000000002", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000000000000002").label(),
            Some("other channel")
        );
        window.click("refresh-channels", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("refresh-channels").label(),
            Some("Refreshing channels…")
        );
        window.click("refresh-channels", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("refresh-channels").label(),
            Some("Refresh channels")
        );
        assert_eq!(
            window.find("message-000000000000002").label(),
            Some("other channel")
        );
        window.click("refresh-history", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000000000000002").label(),
            Some("other channel")
        );
    });
}

struct RemovingChannel(Arc<AtomicBool>);
impl RequestAdapter for RemovingChannel {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, AuthError>> {
        if request.url().path() == "/api/v1/channels" && self.0.load(Ordering::SeqCst) {
            return Box::pin(async {
                Ok(Response::controlled(
                    StatusCode::OK,
                    r#"{"items":[{"id":"000000000000002","name":"general","type":"text"}]}"#,
                ))
            });
        }
        BoundAuth.execute(request)
    }
}

#[gpui_kit::test]
fn channel_discovery_shows_cached_fallback_after_selected_channel_disappears(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let remove = Arc::new(AtomicBool::new(false));
    let removed = remove.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(RemovingChannel(removed))));
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
        window.click("channel-000000000000002", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000000000000002").label(),
            Some("other channel")
        );
        window.click("channel-000000000000001", cx);
    });
    cx.run_until_parked();
    remove.store(true, Ordering::SeqCst);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("refresh-channels", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-000000000000002").label(),
            Some("other channel")
        );
    });
}
