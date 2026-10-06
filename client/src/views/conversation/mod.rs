//! Conversation layout composes independently owned header, history and composer children.
mod channel_header;
pub(crate) mod composer;
pub(crate) mod message_history;
mod message_row;

use crate::conversation::ConversationHandle;
use channel_header::ChannelHeaderView;
use composer::ComposerView;
use gpui_kit::*;
use message_history::MessageHistoryView;

pub(crate) struct ConversationView {
    header: Entity<ChannelHeaderView>,
    history: Entity<MessageHistoryView>,
    composer: Entity<ComposerView>,
}
impl ConversationView {
    pub(crate) fn new(
        conversation: ConversationHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            header: cx.new(|cx| ChannelHeaderView::new(conversation.clone(), cx)),
            history: cx.new(|cx| MessageHistoryView::new(conversation.clone(), window, cx)),
            composer: cx.new(|cx| ComposerView::new(conversation, window, cx)),
        }
    }
}

impl Render for ConversationView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .h_full()
            .min_w_0()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.header.clone())
            .child(self.history.clone())
            .child(self.composer.clone())
    }
}
