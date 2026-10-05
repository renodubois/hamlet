use super::*;

struct CreateAuth {
    results:
        Arc<std::sync::Mutex<std::collections::VecDeque<Result<crate::api::Channel, ApiError>>>>,
    calls: Arc<AtomicUsize>,
    histories: Arc<Mutex<Vec<String>>>,
    empty: bool,
}
impl RequestAdapter for CreateAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
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
            self.histories
                .lock()
                .unwrap()
                .push(request.url().path().to_owned());
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
            Ok(crate::api::Channel {
                id: "3".into(),
                name: "Middle".into(),
            }),
        ])));
        let calls = Arc::new(AtomicUsize::new(0));
        let histories = Arc::new(Mutex::new(Vec::new()));

        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = open_controlled(
                window,
                cx,
                Arc::new(CreateAuth {
                    results: results.clone(),
                    calls: calls.clone(),
                    histories: histories.clone(),
                    empty,
                }),
            );

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
            window.click("channel-name", cx);
            window.input("  Middle  ", cx);
            window.click("create-channel", cx);
            window.render_frame(cx);
            assert_eq!(
                window.find("create-channel").label(),
                Some("Creating channel…")
            );
            assert_eq!(window.find("channel-name").value(), Some("  Middle  "));
            assert_eq!(window.try_find("composer").is_none(), empty);
            assert_eq!(
                histories.lock().unwrap().as_slice(),
                if empty {
                    vec![]
                } else {
                    vec!["/api/v1/channels/1/messages"]
                }
            );
            window.click("create-channel", cx);
            assert!(window.try_find("channel-3").is_none());
        });
        cx.run_until_parked();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("channel-name").value(), Some(""));
            assert!(window.try_find("composer").is_some());
            assert_eq!(
                histories.lock().unwrap().last().map(String::as_str),
                Some("/api/v1/channels/3/messages")
            );
            assert_eq!(window.find("channel-3").label(), Some("# Middle"));
            if empty {
                assert!(window.try_find("channel-1").is_none());
                assert!(window.try_find("channel-2").is_none());
            } else {
                let middle = window.find("channel-3").bounds().origin.y;
                assert!(window.find("channel-1").bounds().origin.y < middle);
                assert!(middle < window.find("channel-2").bounds().origin.y);
            }
            assert!(window.try_find("refresh-history").is_none());
            assert!(
                window.try_find("history").is_none(),
                "confirmed empty history has no rows"
            );
        });
    }
}

#[gpui_kit::test]
fn create_controls_keep_input_on_errors_without_replay(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let results = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from([
        Err(ApiError::Conflict),
        Err(ApiError::Unavailable),
    ])));
    let calls = Arc::new(AtomicUsize::new(0));

    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(
            window,
            cx,
            Arc::new(CreateAuth {
                results: results.clone(),
                calls: calls.clone(),
                histories: Default::default(),
                empty: true,
            }),
        );

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
        assert!(window.try_find("composer").is_none());
        assert_eq!(window.find("login").label(), Some("Log in"));
    });
}

#[gpui_kit::test]
fn create_completion_after_logout_cannot_navigate(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let results = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from([
        Ok(crate::api::Channel {
            id: "3".into(),
            name: "Late".into(),
        }),
    ])));

    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(
            window,
            cx,
            Arc::new(CreateAuth {
                results,
                calls: Arc::new(AtomicUsize::new(0)),
                histories: Default::default(),
                empty: true,
            }),
        );

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
        window.click("channel-name", cx);
        window.input("Late", cx);
        window.click("create-channel", cx);
        window.click("logout", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Log in"));
        assert!(window.try_find("channel-3").is_none());
        assert!(window.try_find("composer").is_none());
    });
}

#[gpui_kit::test]
fn remote_channel_creation_keeps_selected_conversation_without_refresh_controls(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let streams = crate::test_support::live::Streams::default();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_with_streams(window, cx, Arc::new(BoundAuth), &streams);
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
        assert!(window.try_find("refresh-channels").is_none());
    });
    streams.change(serde_json::json!({"type":"channel_created","channel":{"id":"remote","name":"Remote","type":"text"}}));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("channel-remote").label(), Some("# Remote"));
        assert_eq!(
            window.find("message-000000000000002").label(),
            Some("other channel")
        );
        assert!(window.try_find("refresh-history").is_none());
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
