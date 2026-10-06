//! Presents the selected channel independently of message history.
use crate::workspace::{Load, WorkspaceHandle};
use gpui_kit::*;

pub(crate) struct ChannelHeaderView {
    workspace: WorkspaceHandle,
    _notifications: Task<()>,
}

impl ChannelHeaderView {
    pub(crate) fn new(workspace: WorkspaceHandle, cx: &mut Context<Self>) -> Self {
        let changes = workspace.notifications();
        let notifications = cx.spawn(async move |weak, cx| {
            while changes.recv().await.is_ok() {
                if weak.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        Self {
            workspace,
            _notifications: notifications,
        }
    }
}

impl Render for ChannelHeaderView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.read();
        let header = div().flex_none().min_w_0().flex().items_center();
        if let Some(id) = workspace.selected.as_deref() {
            let name = match &workspace.channels {
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
