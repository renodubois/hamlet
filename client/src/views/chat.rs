//! Composes the authenticated screen from sibling view modules.
//! Session and chat behavior remain independent of the rendered screen's lifetime.
use super::{
    conversation::ConversationView, session_footer::SessionFooterView, sidebar::SidebarView,
};

use crate::chat::ChatHandle;
use crate::runtime::Execution;
use crate::session::SessionCoordinator;
use gpui_kit::*;

pub(super) struct ChatView {
    sidebar: Entity<SidebarView>,
    footer: Entity<SessionFooterView>,
    layout: Entity<ConversationView>,
    _notifications: Task<()>,
}
impl ChatView {
    pub(super) fn new(
        chat: ChatHandle,
        session: Entity<SessionCoordinator>,
        execution: Execution,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let footer = cx.new(|cx| SessionFooterView::new(session, cx));
        let sidebar = cx.new(|cx| SidebarView::new(chat.clone(), window, cx));
        let layout = cx.new(|cx| ConversationView::new(chat.clone(), execution, window, cx));
        let changes = chat.notifications();
        let notifications = cx.spawn(async move |weak, cx| {
            while changes.recv().await.is_ok() {
                if weak.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        Self {
            sidebar,
            footer,
            layout,
            _notifications: notifications,
        }
    }
}
impl Render for ChatView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let sidebar = div()
            .w(px(220.))
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(self.sidebar.clone())
            .child(self.footer.clone());
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(sidebar)
                    .child(self.layout.clone()),
            )
    }
}

#[cfg(test)]
#[path = "tests/chat.rs"]
mod chat_tests;
