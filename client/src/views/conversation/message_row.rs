//! Plain, selectable message presentation; rows do not need independent entities.
use crate::api::Message;
use gpui_kit::base::SelectableText;
use gpui_kit::component::ActiveTheme;
use gpui_kit::*;

pub(super) fn message_row(message: &Message, cx: &App) -> impl IntoElement {
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
                .child(format!("{} · {}", message.author_name, message.created_at)),
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
