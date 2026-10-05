//! Semantic controls exercise the production session-owned stream and recovery path.
use super::*;

#[gpui_kit::test]
fn recovery_notice_persists_with_content_and_draft_until_required_baseline_succeeds(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (send, pages) = std::sync::mpsc::channel();
    let streams = crate::test_support::live::Streams::default();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_with_streams(window, cx, Arc::new(PagedAuth(send)), &streams);
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
    let (_, initial) = pages.try_recv().unwrap();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("connection-status").label(),
            Some("Connecting…")
        );
        assert!(window.try_find("refresh-channels").is_none());
        assert!(window.try_find("refresh-history").is_none());
    });
    let message = crate::api::Message {
        id: "8".into(),
        channel_id: "000000000000001".into(),
        author_id: "42".into(),
        author_name: "Ada".into(),
        text: "retained row".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    initial
        .try_send(Ok(crate::api::Page {
            items: vec![message.clone()],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("draft survives", cx);
        window.click("channel-name", cx);
        window.input("unfinished room", cx);
    });
    streams.disconnect();
    cx.run_until_parked();
    let assert_stale = |window: &mut Window, cx: &mut gpui_kit::App| {
        window.render_frame(cx);
        assert_eq!(
            window.find("connection-status").label(),
            Some("Reconnecting… messages may be out of date")
        );
        assert_eq!(window.find("message-8").label(), Some("retained row"));
        assert_eq!(composer_text(window, cx), "draft survives");
        assert_eq!(window.find("channel-name").value(), Some("unfinished room"));
        assert!(window.try_find("catchup-incomplete").is_none());
    };
    cx.update(assert_stale);
    advance(cx, 1);
    let (_, failed) = pages.try_recv().unwrap();
    cx.update(assert_stale);
    failed.try_send(Err(ApiError::Unavailable)).unwrap();
    cx.run_until_parked();
    cx.update(assert_stale);
    advance(cx, 1);
    let (_, recovery) = pages.try_recv().unwrap();
    cx.update(assert_stale);
    recovery
        .try_send(Ok(crate::api::Page {
            items: vec![message],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("connection-status").label(), Some(""));
        assert_eq!(composer_text(window, cx), "draft survives");
        window.click("logout", cx);
    });
    cx.run_until_parked();
    let count = streams.count();
    advance(cx, 60);
    assert_eq!(streams.count(), count);
    assert!(pages.try_recv().is_err());
}
