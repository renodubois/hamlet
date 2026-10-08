//! Channel controls own their input/focus and subscription; policy belongs to chat.
mod channel_row;

use crate::chat::{ChatHandle, Load};
use gpui_kit::base::Disableable;
use gpui_kit::base::input::{InputBaseState, InputMode, InputState};
use gpui_kit::component::{ActiveTheme, WindowExt as _, button::Button, input::Input};
use gpui_kit::*;

pub(super) struct SidebarView {
    chat: ChatHandle,
    channel_name: Entity<InputBaseState<InputMode>>,
    created_revision: u64,
    dialog_open: bool,
    rename_revision: Option<u64>,
    delete_revision: Option<u64>,
    _release: Subscription,
    _notifications: Task<()>,
}
impl SidebarView {
    pub(super) fn new(chat: ChatHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let channel_name = cx.new(|cx| InputState::new(window, cx));
        // A newly mounted input must not consume a confirmation from before its lifetime.
        let created_revision = chat.created().map(|(revision, _)| revision).unwrap_or(0);
        let changes = chat.notifications();
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
        let release = cx.on_release_in(window, |view, window, cx| {
            if view.dialog_open {
                window.close_dialog(cx);
            }
        });
        Self {
            chat,
            channel_name,
            created_revision,
            dialog_open: false,
            rename_revision: None,
            delete_revision: None,
            _release: release,
            _notifications: notifications,
        }
    }
    fn present(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(revision) = self.delete_revision
            && revision != self.chat.read().delete_confirmed
        {
            self.delete_revision = None;
            self.dialog_open = false;
            window.close_dialog(cx);
        }
        if let Some(revision) = self.rename_revision
            && revision != self.chat.read().rename_confirmed
        {
            self.rename_revision = None;
            self.dialog_open = false;
            window.close_dialog(cx);
        }
        if self.chat.read().channels.is_none() {
            if self.dialog_open {
                self.dialog_open = false;
                window.close_dialog(cx);
            }
            // A retained but hidden sidebar is cleared by shutdown notifications too.
            if !self.channel_name.read(cx).text().to_string().is_empty() {
                self.channel_name
                    .update(cx, |input, cx| input.set_value("", window, cx));
            }
        }
        if let Some((revision, _)) = self.chat.created()
            && revision != self.created_revision
        {
            self.created_revision = revision;
            self.channel_name
                .update(cx, |input, cx| input.set_value("", window, cx));
            if self.dialog_open && self.rename_revision.is_none() && self.delete_revision.is_none()
            {
                self.dialog_open = false;
                window.close_dialog(cx);
            }
        }
    }
    fn open_create_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_open {
            return;
        }
        self.dialog_open = true;
        let view = cx.entity().downgrade();
        let closed = view.clone();
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(view) = view.upgrade() else {
                return dialog;
            };
            let sidebar = view.read(cx);
            let chat = sidebar.chat.read();
            let pending = chat.create_pending;
            let submit = view.downgrade();
            let cancel = view.downgrade();
            let confirm = view.downgrade();
            let mut content = div().flex().flex_col().gap_2().child("Channel name").child(
                Input::new(&sidebar.channel_name)
                    .id("channel-name")
                    .aria_label("Channel name")
                    .disabled(pending),
            );
            if let Some(feedback) = &chat.create_feedback {
                content = content.child(
                    div()
                        .id("channel-feedback")
                        .aria_label(feedback.clone())
                        .test_support()
                        .child(feedback.clone()),
                );
            }
            dialog
                .title("Create text channel")
                .on_ok(move |_, _, cx| {
                    if !pending {
                        let _ = confirm.update(cx, |view, cx| view.create_channel(cx));
                    }
                    false // HTTP confirmation, not the click, closes the dialog.
                })
                .child(content)
                .footer(
                    div()
                        .flex()
                        .gap_2()
                        .child(Button::new("cancel-channel").label("Cancel").on_click(
                            move |_, window, cx| {
                                let _ = cancel.update(cx, |view, cx| {
                                    view.dialog_open = false;
                                    view.channel_name
                                        .update(cx, |input, cx| input.set_value("", window, cx));
                                    cx.notify();
                                });
                                window.close_dialog(cx);
                            },
                        ))
                        .child(
                            Button::new("create-channel")
                                .disabled(pending)
                                .label(if pending {
                                    "Creating channel…"
                                } else {
                                    "Create text channel"
                                })
                                .on_click(move |_, _, cx| {
                                    let _ = submit.update(cx, |view, cx| view.create_channel(cx));
                                }),
                        ),
                )
                .on_close({
                    let closed = closed.clone();
                    move |_, window, cx| {
                        let _ = closed.update(cx, |view, cx| {
                            view.dialog_open = false;
                            view.channel_name
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            cx.notify();
                        });
                    }
                })
        });
        self.channel_name
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }
    fn create_channel(&mut self, cx: &mut Context<Self>) {
        self.chat
            .create_channel(&self.channel_name.read(cx).text().to_string());
        cx.notify();
    }
}
impl Render for SidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.present(window, cx);
        let chat = self.chat.read();
        let view = cx.entity().downgrade();
        let create_view = view.clone();
        let mut sidebar = div()
            .id("channels")
            .test_support()
            .w_full()
            .flex_1()
            .min_h_0()
            .bg(cx.theme().tokens.sidebar.background)
            .text_color(cx.theme().sidebar_foreground)
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .overflow_y_scroll();
        let mut channel_container = div().flex_1();
        match &chat.channels {
            None | Some(Load::Loading) => sidebar = sidebar.child("Loading channels…"),
            Some(Load::Failed(error)) => sidebar = sidebar.child(format!("Channels: {error}")),
            Some(Load::Ready(channels)) if channels.is_empty() => {
                sidebar = sidebar.child("No text channels yet.")
            }
            Some(Load::Ready(channels)) => {
                for channel in channels {
                    let selected = chat.selected.as_deref() == Some(&channel.id);
                    channel_container =
                        channel_container.child(Self::channel_row(channel, selected, cx));
                }
            }
        }
        sidebar = sidebar.child(channel_container);

        sidebar = sidebar.child(
            Button::new("open-create-channel")
                .disabled(chat.create_pending || !matches!(chat.channels, Some(Load::Ready(_))))
                .label(if chat.create_pending {
                    "Creating channel…"
                } else {
                    "Create text channel"
                })
                .on_click(move |_, window, cx| {
                    let _ = create_view.update(cx, |view, cx| view.open_create_dialog(window, cx));
                }),
        );

        sidebar
    }
}
