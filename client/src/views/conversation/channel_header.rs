//! Presents the selected channel independently of message history.
use crate::chat::{ChatHandle, Load};
use gpui_kit::*;

pub(super) struct ChannelHeaderView {
    chat: ChatHandle,
    _notifications: Task<()>,
}

impl ChannelHeaderView {
    pub(super) fn new(chat: ChatHandle, cx: &mut Context<Self>) -> Self {
        let changes = chat.notifications();
        let notifications = cx.spawn(async move |weak, cx| {
            while changes.recv().await.is_ok() {
                if weak.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        Self {
            chat,
            _notifications: notifications,
        }
    }
}

impl Render for ChannelHeaderView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let chat = self.chat.read();
        let header = div().flex_none().min_w_0().flex().items_center();
        if let Some(id) = chat.selected.as_deref() {
            let name = match &chat.channels {
                Some(Load::Ready(channels)) => channels
                    .iter()
                    .find(|channel| channel.id == id)
                    .map(|channel| channel.name.as_str())
                    .unwrap_or("Channel"),
                _ => "Channel",
            };
            let label = format!("# {name}");
            return header
                .id("channel-header")
                .aria_label(label.clone())
                .test_support()
                .child(div().text_lg().font_weight(FontWeight::BOLD).child(label))
                .into_any_element();
        }
        header.into_any_element()
    }
}
