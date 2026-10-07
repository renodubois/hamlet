use super::*;

#[gpui_kit::test]
fn channel_context_menu_cancellation_is_request_free(cx: &mut TestAppContext) {
    struct RecordingAuth(Arc<AtomicUsize>);
    impl RequestAdapter for RecordingAuth {
        fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            BoundAuth.execute(request)
        }
    }

    cx.update(|cx| {
        gpui_kit::init(cx);
        // Dialog slide-in animation uses wall time; keep hit targets deterministic.
        cx.set_reduce_motion(true);
    });
    let requests = Arc::new(AtomicUsize::new(0));
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(window, cx, Arc::new(RecordingAuth(requests.clone())));
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
    let initial_requests = requests.load(Ordering::SeqCst);

    for id in ["000000000000001", "000000000000002"] {
        for dismissal in ["confirm", "enter", "cancel", "escape"] {
            let original_name = cx.update(|window, cx| {
                window.render_frame(cx);
                window
                    .find(format!("channel-{id}"))
                    .label()
                    .unwrap()
                    .to_owned()
            });
            cx.update(|window, cx| {
                window.render_frame(cx);
                window.right_click(format!("channel-{id}"), cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.render_frame(cx);
                assert!(
                    gpui_kit::base::test_support::snapshots(window)
                        .iter()
                        .any(|item| item.label() == Some("Rename channel"))
                );
                assert_eq!(
                    window.find("delete-channel-label").label(),
                    Some("Delete channel")
                );
                window.press("down", cx);
                window.press("enter", cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("delete-channel-label").is_none());
                window.render_frame(cx);
                assert_eq!(
                    window.find("rename-channel-name").value(),
                    Some(original_name.as_str())
                );
                window.click("rename-channel-name", cx);
                window.press("ctrl-a", cx);
                window.input("Discard this rename", cx);
                assert_eq!(
                    window.find("rename-channel-name").value(),
                    Some("Discard this rename")
                );
                match dismissal {
                    "confirm" | "enter" | "cancel" => window.click("cancel-channel-action", cx),
                    _ => window.press("escape", cx),
                }
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("rename-channel-name").is_none());
                assert_eq!(
                    window.find(format!("channel-{id}")).label(),
                    Some(original_name.as_str())
                );
                window.right_click(format!("channel-{id}"), cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.render_frame(cx);
                window.click("delete-channel-label", cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("delete-channel-label").is_none());
                window.render_frame(cx);
                assert_eq!(
                    window.find("delete-channel-confirmation").label(),
                    Some("Are you sure you want to delete this channel?")
                );
                assert_eq!(
                    window.find("delete-channel-warning").label(),
                    Some("This action cannot be undone.")
                );
                assert_eq!(
                    window.find("confirm-delete-channel").label(),
                    Some("Delete")
                );
                match dismissal {
                    "confirm" => window.click("confirm-delete-channel", cx),
                    "enter" => window.press("enter", cx),
                    "cancel" => window.click("cancel-channel-action", cx),
                    _ => window.press("escape", cx),
                }
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.render_frame(cx);
                assert!(
                    window.try_find("delete-channel-confirmation").is_none(),
                    "delete dialog for {id} remained open after {dismissal}"
                );
                assert_eq!(
                    window.find(format!("channel-{id}")).label(),
                    Some(original_name.as_str())
                );
            });
            cx.run_until_parked();
            assert_eq!(requests.load(Ordering::SeqCst), initial_requests);
        }
    }
}
