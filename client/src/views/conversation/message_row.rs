//! Plain, selectable message presentation; rows do not need independent entities.
use super::message_timestamp::format_timestamp;
use crate::api::Message;
use chrono::{DateTime, Local, TimeZone};
use gpui_kit::base::SelectableText;
use gpui_kit::component::{ActiveTheme, tooltip::Tooltip};
use gpui_kit::*;

pub(super) fn message_row(message: &Message, cx: &App) -> impl IntoElement {
    message_row_at(message, &Local::now(), cx)
}

fn message_row_at<Tz: TimeZone>(
    message: &Message,
    now: &DateTime<Tz>,
    cx: &App,
) -> impl IntoElement + use<Tz> {
    let timestamp = format_timestamp(&message.created_at, now);
    let tooltip_id = format!("message-exact-time-{}", message.id);
    div()
        .id(format!("message-{}", message.id))
        .aria_label(message.text.clone())
        .test_support()
        .w_full()
        .p_2()
        .flex()
        .flex_col()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .flex()
                .gap_1()
                .child(format!("{} ·", message.author_name))
                .child(
                    div()
                        .id(format!("message-timestamp-{}", message.id))
                        .aria_label(timestamp.label.clone())
                        .test_support()
                        .child(timestamp.label)
                        .tooltip(move |window, cx| {
                            let exact = timestamp.exact.clone();
                            let id = tooltip_id.clone();
                            Tooltip::element(move |_, _| {
                                div()
                                    .id(id.clone())
                                    .aria_label(exact.clone())
                                    .test_support()
                                    .child(exact.clone())
                            })
                            .build(window, cx)
                        }),
                ),
        )
        .child(
            div()
                .id(format!("message-text-{}", message.id))
                .test_support()
                .child(SelectableText::new(
                    format!("text-{}", message.id),
                    message.text.clone(),
                )),
        )
}

#[cfg(test)]
#[path = "../tests/message_row.rs"]
mod tests;
