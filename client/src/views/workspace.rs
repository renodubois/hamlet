//! Composes the channel sidebar and conversation view against one shared workspace handle.
use super::{
    conversation::ConversationView, session_footer::SessionFooterView, sidebar::SidebarView,
};
use crate::runtime::Execution;
use crate::workspace::WorkspaceHandle;
use gpui_kit::*;

pub(crate) struct WorkspaceView {
    sidebar: Entity<SidebarView>,
    footer: Option<Entity<SessionFooterView>>,
    layout: Entity<ConversationView>,
    _notifications: Task<()>,
}
impl WorkspaceView {
    pub(crate) fn new(
        workspace: WorkspaceHandle,
        execution: Execution,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let sidebar = cx.new(|cx| SidebarView::new(workspace.clone(), window, cx));
        let layout = cx.new(|cx| ConversationView::new(workspace.clone(), execution, window, cx));
        let changes = workspace.notifications();
        let notifications = cx.spawn(async move |weak, cx| {
            while changes.recv().await.is_ok() {
                if weak.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        Self {
            sidebar,
            footer: None,
            layout,
            _notifications: notifications,
        }
    }
    pub(crate) fn with_footer(mut self, footer: Entity<SessionFooterView>) -> Self {
        self.footer = Some(footer);
        self
    }
}
impl Render for WorkspaceView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut sidebar = div()
            .w(px(220.))
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(self.sidebar.clone());
        if let Some(footer) = &self.footer {
            sidebar = sidebar.child(footer.clone());
        }
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
