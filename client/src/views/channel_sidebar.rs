//! Channel controls own their input/focus and subscription; policy belongs to conversation.
use crate::conversation::{ConversationHandle, Load};
use crate::theme;
use gpui_kit::base::Disableable;
use gpui_kit::base::input::{InputBaseState, InputMode, InputState};
use gpui_kit::component::{button::Button, input::Input};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub(crate) struct ChannelSidebarView {
    conversation: ConversationHandle,
    channel_name: Entity<InputBaseState<InputMode>>,
    created_revision: u64,
    _notifications: Task<()>,
}
impl ChannelSidebarView {
    pub(crate) fn new(
        conversation: ConversationHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let channel_name = cx.new(|cx| InputState::new(window, cx));
        // A newly mounted input must not consume a confirmation from before its lifetime.
        let created_revision = conversation
            .created()
            .map(|(revision, _)| revision)
            .unwrap_or(0);
        let changes = conversation.notifications();
        let notifications = cx.spawn(async move |weak, cx| {
            while changes.recv().await.is_ok() {
                if weak
                    .update_in(cx, |view: &mut Self, window, cx| {
                        view.present(window, cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            conversation,
            channel_name,
            created_revision,
            _notifications: notifications,
        }
    }
    fn present(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.conversation.read().channels.is_none() {
            // A retained but hidden sidebar is cleared by shutdown notifications too.
            if !self.channel_name.read(cx).text().to_string().is_empty() {
                self.channel_name
                    .update(cx, |input, cx| input.set_value("", window, cx));
            }
        }
        if let Some((revision, name)) = self.conversation.created()
            && revision != self.created_revision
        {
            self.created_revision = revision;
            if self.channel_name.read(cx).text().to_string().trim() == name {
                self.channel_name
                    .update(cx, |input, cx| input.set_value("", window, cx));
            }
        }
    }
    fn create_channel(&mut self, cx: &mut Context<Self>) {
        self.conversation
            .create_channel(&self.channel_name.read(cx).text().to_string());
        cx.notify();
    }
    fn select_channel(&mut self, id: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.conversation.select_channel(id);
        cx.notify();
    }
}
impl Render for ChannelSidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.present(window, cx);
        let conversation = self.conversation.read();
        let view = cx.entity().downgrade();
        let create_view = view.clone();
        let mut sidebar = div()
            .id("channels")
            .test_support()
            .w(px(220.))
            .h_full()
            .bg(rgb(theme::SIDEBAR))
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .overflow_y_scroll()
            .child(div().font_weight(FontWeight::BOLD).child("Text channels"))
            .child(
                div().child("Channel name").child(
                    Input::new(&self.channel_name)
                        .id("channel-name")
                        .aria_label("Channel name"),
                ),
            )
            .child(
                Button::new("create-channel")
                    .disabled(
                        conversation.create_pending
                            || !matches!(conversation.channels, Some(Load::Ready(_))),
                    )
                    .label(if conversation.create_pending {
                        "Creating channel…"
                    } else {
                        "Create text channel"
                    })
                    .on_click(move |_, _, cx| {
                        let _ = create_view.update(cx, |view, cx| view.create_channel(cx));
                    }),
            );
        if let Some(feedback) = &conversation.create_feedback {
            sidebar = sidebar.child(
                div()
                    .id("channel-feedback")
                    .aria_label(feedback.clone())
                    .test_support()
                    .child(feedback.clone()),
            );
        }
        match &conversation.channels {
            None | Some(Load::Loading) => sidebar = sidebar.child("Loading channels…"),
            Some(Load::Failed(error)) => sidebar = sidebar.child(format!("Channels: {error}")),
            Some(Load::Ready(channels)) if channels.is_empty() => {
                sidebar = sidebar.child("No text channels yet.")
            }
            Some(Load::Ready(channels)) => {
                for channel in channels {
                    let id = channel.id.clone();
                    let selected = conversation.selected.as_deref() == Some(&id);
                    let label = format!("# {}", channel.name);
                    let target = view.clone();
                    sidebar = sidebar.child(
                        Button::new(format!("channel-{id}"))
                            .label(label)
                            .icon(theme::channel_icon())
                            .on_click(move |_, window, cx| {
                                let _ = target
                                    .update(cx, |view, cx| view.select_channel(&id, window, cx));
                            })
                            .when(selected, |button| button.bg(rgb(theme::SELECTED))),
                    );
                }
            }
        }
        sidebar
    }
}
