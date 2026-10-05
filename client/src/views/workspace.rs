//! Composes the channel sidebar and conversation view against one shared conversation handle.
use super::{channel_sidebar::ChannelSidebarView, conversation::ConversationView};
use crate::conversation::ConversationHandle;
use gpui_kit::*;

pub(crate) struct WorkspaceView {
    conversation: ConversationHandle,
    sidebar: Entity<ChannelSidebarView>,
    layout: Entity<ConversationView>,
    _notifications: Task<()>,
}
impl WorkspaceView {
    pub(crate) fn new(
        conversation: ConversationHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let sidebar = cx.new(|cx| ChannelSidebarView::new(conversation.clone(), window, cx));
        let layout = cx.new(|cx| ConversationView::new(conversation.clone(), window, cx));
        let changes = conversation.notifications();
        let notifications = cx.spawn(async move |weak, cx| {
            while changes.recv().await.is_ok() {
                if weak.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        Self {
            conversation,
            sidebar,
            layout,
            _notifications: notifications,
        }
    }
}
impl Render for WorkspaceView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .gap_3()
            .child(
                div()
                    .id("connection-status")
                    .aria_label(self.conversation.status())
                    .test_support()
                    .child(self.conversation.status()),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(self.sidebar.clone())
                    .child(self.layout.clone()),
            )
    }
}
