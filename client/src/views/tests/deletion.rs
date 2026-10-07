use super::*;

#[gpui_kit::test]
fn selected_delete_controls_show_fallback_and_discard_deleted_draft(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(window, cx, Arc::new(DeleteAuth(calls.clone())));
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
        window.click("composer", cx);
        window.input("discarded draft", cx);
        window.right_click("channel-000000000000001", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("delete-channel-label", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("confirm-delete-channel", cx);
    });
    cx.run_until_parked();
    calls.lock().unwrap()[0].1.try_send(Ok(())).unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("channel-000000000000001").is_none());
        assert_eq!(window.find("channel-header").label(), Some("# general"));
        assert_eq!(composer_text(window, cx), "");
        assert!(window.try_find("delete-channel-confirmation").is_none());
    });
}

type DeleteCall = (String, async_channel::Sender<Result<(), ApiError>>);
struct DeleteAuth(Arc<Mutex<Vec<DeleteCall>>>);
impl RequestAdapter for DeleteAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.method() != reqwest::Method::DELETE {
            return BoundAuth.execute(request);
        }
        assert!(request.body().is_none());
        let (send, receive) = async_channel::bounded(1);
        self.0
            .lock()
            .unwrap()
            .push((request.url().path().into(), send));
        Box::pin(async move {
            match receive.recv().await.unwrap() {
                Ok(()) => Ok(Response::controlled(StatusCode::NO_CONTENT, "")),
                Err(error) => wire_response(Err(error), StatusCode::NO_CONTENT),
            }
        })
    }
}

#[gpui_kit::test]
fn delete_controls_target_pending_feedback_confirmed_cleanup_and_timeout(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(window, cx, Arc::new(DeleteAuth(calls.clone())));
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
        window.click("composer", cx);
        window.input("preserved draft", cx);
        window.right_click("channel-000000000000002", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("delete-channel-label", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("delete-channel-warning").label(),
            Some("You cannot restore this channel in the app.")
        );
        window.click("confirm-delete-channel", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("confirm-delete-channel").label(),
            Some("Deleting channel…")
        );
        window.click("confirm-delete-channel", cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("delete-channel-confirmation").is_some());
    });
    cx.run_until_parked();
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(
        calls.lock().unwrap()[0].0,
        "/api/v1/channels/000000000000002"
    );
    for (index, error, feedback) in [
        (0, ApiError::Conflict, "last active channel"),
        (1, ApiError::NotFound, "not found"),
        (2, ApiError::Unavailable, "may have succeeded"),
    ] {
        calls.lock().unwrap()[index].1.try_send(Err(error)).unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find("delete-channel-feedback")
                    .label()
                    .unwrap()
                    .contains(feedback)
            );
            assert_eq!(composer_text(window, cx), "preserved draft");
        });
        assert_eq!(
            calls.lock().unwrap().len(),
            index + 1,
            "failure never replays deletion"
        );
        cx.update(|window, cx| {
            window.press("enter", cx);
        });
        cx.run_until_parked();
        assert_eq!(calls.lock().unwrap().len(), index + 2);
    }
    calls.lock().unwrap()[3].1.try_send(Ok(())).unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("delete-channel-confirmation").is_none());
        assert!(window.try_find("channel-000000000000002").is_none());
        assert_eq!(composer_text(window, cx), "preserved draft");
        window.right_click("channel-000000000000001", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("delete-channel-label", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("confirm-delete-channel", cx);
    });
    cx.run_until_parked();
    assert_eq!(calls.lock().unwrap().len(), 5);
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("delete-channel-feedback")
                .label()
                .unwrap()
                .contains("may have succeeded")
        );
        assert!(window.try_find("channel-000000000000001").is_some());
        assert_eq!(composer_text(window, cx), "preserved draft");
        window.click("cancel-channel-action", cx);
    });
    cx.run_until_parked();
    assert_eq!(
        calls.lock().unwrap().len(),
        5,
        "timeout never replays deletion"
    );
    assert!(calls.lock().unwrap()[4].1.try_send(Ok(())).is_err());
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("delete-channel-confirmation").is_none());
    });
}
