use super::message_row_at;
use crate::api::Message;
use chrono::DateTime;
use gpui_kit::component::Root;
use gpui_kit::test::{TestAppContextExt, TestWindowExt};
use gpui_kit::{AppContext, Context, IntoElement, Render, TestAppContext, Window, px, size};
use std::time::Duration;

struct Row {
    message: Message,
    now: DateTime<chrono::FixedOffset>,
}
impl Render for Row {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        message_row_at(&self.message, &self.now, cx)
    }
}

#[gpui_kit::test]
async fn message_row_displays_label_and_exact_time_on_hover(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut row_entity = None;
    let handle = cx.open_window(size(px(640.), px(480.)), |window, cx| {
        let row = cx.new(|_| Row {
            message: Message {
                id: "example".into(),
                channel_id: "channel".into(),
                author_id: "user".into(),
                author_name: "Ada".into(),
                text: "Hello\nworld".into(),
                created_at: "2026-03-14T18:41:08Z".into(),
            },
            now: DateTime::parse_from_rfc3339("2026-03-14T14:00:00-05:00").unwrap(),
        });
        row_entity = Some(row.clone());
        Root::new(row, window, cx)
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-timestamp-example").label(),
            Some("18 minutes ago")
        );
        assert!(window.try_find("message-exact-time-example").is_none());
        window.hover("message-timestamp-example", cx);
    })
    .unwrap();
    cx.wait_for(handle.into(), Duration::from_secs(2), |window, _| {
        window
            .try_find("message-exact-time-example")
            .is_some_and(|snapshot| snapshot.visible())
    })
    .await;
    cx.update_window(handle.into(), |_, window, _| {
        assert_eq!(
            window.find("message-exact-time-example").label(),
            Some("03/14/2026 1:41:08 PM")
        );
        assert_eq!(window.find("message-example").label(), Some("Hello\nworld"));
    })
    .unwrap();
    // Normal redraws recalculate labels without changing the message or tooltip instant.
    let row = row_entity.unwrap();
    for (now, label) in [
        ("2026-03-14T15:41:08-05:00", "2 hours ago"),
        ("2026-03-15T00:00:00-05:00", "yesterday at 1:41 PM"),
        ("2026-03-16T00:00:00-05:00", "03/14/2026 1:41 PM"),
        ("2026-03-13T00:00:00-05:00", "just now"),
    ] {
        cx.update_window(handle.into(), |_, window, cx| {
            row.update(cx, |row, cx| {
                row.now = DateTime::parse_from_rfc3339(now).unwrap();
                cx.notify();
            });
            window.render_frame(cx);
            assert_eq!(
                window.find("message-timestamp-example").label(),
                Some(label)
            );
            assert_eq!(
                window.find("message-exact-time-example").label(),
                Some("03/14/2026 1:41:08 PM")
            );
        })
        .unwrap();
    }
}
