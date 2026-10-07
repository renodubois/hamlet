use super::*;

type RenameCall = (
    String,
    serde_json::Value,
    async_channel::Sender<Result<serde_json::Value, ApiError>>,
);
type RenameStreamBody = async_channel::Sender<Result<Vec<u8>, ApiError>>;
struct RenameAuth {
    calls: Arc<Mutex<Vec<RenameCall>>>,
    streams: Mutex<Vec<RenameStreamBody>>,
}
impl crate::api::test_support::StreamAdapter for RenameAuth {
    fn open(
        &self,
        _: Request,
    ) -> ApiFuture<Result<crate::api::test_support::StreamResponse, ApiError>> {
        let (send, body) = async_channel::bounded(8);
        send.try_send(Ok(b"event: ready\ndata: {}\n\n".to_vec()))
            .unwrap();
        self.streams.lock().unwrap().push(send);
        Box::pin(async move {
            Ok(crate::api::test_support::StreamResponse::Controlled {
                status: StatusCode::OK,
                content_type: "text/event-stream".into(),
                body,
            })
        })
    }
}
impl RequestAdapter for RenameAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.method() != reqwest::Method::PATCH {
            return BoundAuth.execute(request);
        }
        let body = serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
        let (send, receive) = async_channel::bounded(1);
        self.calls
            .lock()
            .unwrap()
            .push((request.url().path().into(), body, send));
        Box::pin(async move { wire_response(receive.recv().await.unwrap(), StatusCode::OK) })
    }
}

#[gpui_kit::test]
fn rename_controls_target_pending_feedback_and_confirmation(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let api = Arc::new(RenameAuth {
        calls: calls.clone(),
        streams: Mutex::new(Vec::new()),
    });
    let (_, cx) = cx.add_window_view(|window, cx| {
        let execution =
            crate::runtime::Execution::controlled(cx.background_executor().clone(), 1_800_000_000);
        let view = crate::views::app_shell::open(
            window,
            cx,
            HttpTransport::with_adapters(api.clone(), api.clone()),
            crate::storage::Config::default(),
            None,
            execution,
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
        window.click("composer", cx);
        window.input("preserved draft", cx);
        window.right_click("channel-000000000000002", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.press("down", cx);
        window.press("enter", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("rename-channel-name").value(), Some("general"));
        window.click("rename-channel-name", cx);
        window.press("ctrl-a", cx);
        window.input("invalid!", cx);
        window.click("confirm-rename-channel", cx);
        window.render_frame(cx);
        assert!(
            window
                .find("rename-channel-feedback")
                .label()
                .unwrap()
                .contains("1–64")
        );
        assert_eq!(window.find("rename-channel-name").value(), Some("invalid!"));
        window.click("rename-channel-name", cx);
        window.press("ctrl-a", cx);
        window.input("  AAA  ", cx);
        window.click("confirm-rename-channel", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("confirm-rename-channel").label(),
            Some("Renaming channel…")
        );
        window.click("confirm-rename-channel", cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("rename-channel-name").is_some());
    });
    cx.run_until_parked();
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(
        calls.lock().unwrap()[0].0,
        "/api/v1/channels/000000000000002"
    );
    assert_eq!(
        calls.lock().unwrap()[0].1,
        serde_json::json!({"name":"AAA"})
    );
    let data = serde_json::json!({"type":"channel_renamed","channel":{"id":"000000000000002","name":"AAA","type":"text"}});
    api.streams.lock().unwrap()[0]
        .try_send(Ok(format!("event: change\ndata: {data}\n\n").into_bytes()))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("channel-000000000000002").label(), Some("AAA"));
        assert_eq!(window.find("rename-channel-name").value(), Some("  AAA  "));
        assert_eq!(
            window.find("confirm-rename-channel").label(),
            Some("Renaming channel…")
        );
        assert_eq!(composer_text(window, cx), "preserved draft");
    });
    assert_eq!(
        calls.lock().unwrap().len(),
        1,
        "live rename does not confirm or retry the dialog request"
    );
    for (index, error, feedback) in [
        (0, ApiError::Conflict, "already exists"),
        (1, ApiError::NotFound, "not found"),
        (2, ApiError::Unavailable, "may have succeeded"),
    ] {
        calls.lock().unwrap()[index].2.try_send(Err(error)).unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("rename-channel-name").value(), Some("  AAA  "));
            assert!(
                window
                    .find("rename-channel-feedback")
                    .label()
                    .unwrap()
                    .contains(feedback)
            );
            window.click("confirm-rename-channel", cx);
        });
        cx.run_until_parked();
        assert_eq!(calls.lock().unwrap().len(), index + 2);
    }
    calls.lock().unwrap()[3]
        .2
        .try_send(Ok(
            serde_json::json!({"id":"000000000000002", "name":"AAA", "type":"text"}),
        ))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("rename-channel-name").is_none());
        assert_eq!(window.find("channel-000000000000002").label(), Some("AAA"));
        assert_eq!(composer_text(window, cx), "preserved draft");
        assert!(
            window.find("channel-000000000000002").bounds().origin.y
                < window.find("channel-000000000000001").bounds().origin.y
        );
    });
    assert_eq!(calls.lock().unwrap().len(), 4);
    cx.update(|window, cx| {
        window.right_click("channel-000000000000002", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.press("down", cx);
        window.press("enter", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.press("enter", cx);
    });
    cx.run_until_parked();
    assert_eq!(calls.lock().unwrap().len(), 5);
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("rename-channel-name").value(), Some("AAA"));
        assert!(
            window
                .find("rename-channel-feedback")
                .label()
                .unwrap()
                .contains("may have succeeded")
        );
    });
    assert_eq!(
        calls.lock().unwrap().len(),
        5,
        "timeout never replays rename"
    );
    assert!(
        calls.lock().unwrap()[4]
            .2
            .try_send(Err(ApiError::AlreadyInvalid))
            .is_err()
    );
    cx.update(|window, cx| {
        window.click("cancel-channel-action", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("rename-channel-name").is_none());
        assert_eq!(composer_text(window, cx), "preserved draft");
    });
}
