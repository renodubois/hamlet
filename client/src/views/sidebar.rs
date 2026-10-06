//! Channel controls own their input/focus and subscription; policy belongs to workspace.
use crate::theme;
use crate::workspace::{Load, WorkspaceHandle};
use gpui_kit::base::Disableable;
use gpui_kit::base::input::{InputBaseState, InputMode, InputState};
use gpui_kit::component::{ActiveTheme, WindowExt as _, button::Button, input::Input};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub(crate) struct SidebarView {
    workspace: WorkspaceHandle,
    channel_name: Entity<InputBaseState<InputMode>>,
    created_revision: u64,
    dialog_open: bool,
    _release: Subscription,
    _notifications: Task<()>,
}
impl SidebarView {
    pub(crate) fn new(
        workspace: WorkspaceHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let channel_name = cx.new(|cx| InputState::new(window, cx));
        // A newly mounted input must not consume a confirmation from before its lifetime.
        let created_revision = workspace
            .created()
            .map(|(revision, _)| revision)
            .unwrap_or(0);
        let changes = workspace.notifications();
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
            workspace,
            channel_name,
            created_revision,
            dialog_open: false,
            _release: release,
            _notifications: notifications,
        }
    }
    fn present(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace.read().channels.is_none() {
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
        if let Some((revision, _)) = self.workspace.created()
            && revision != self.created_revision
        {
            self.created_revision = revision;
            self.channel_name
                .update(cx, |input, cx| input.set_value("", window, cx));
            if self.dialog_open {
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
            let workspace = sidebar.workspace.read();
            let pending = workspace.create_pending;
            let submit = view.downgrade();
            let cancel = view.downgrade();
            let confirm = view.downgrade();
            let mut content = div().flex().flex_col().gap_2().child("Channel name").child(
                Input::new(&sidebar.channel_name)
                    .id("channel-name")
                    .aria_label("Channel name")
                    .disabled(pending),
            );
            if let Some(feedback) = &workspace.create_feedback {
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
        self.workspace
            .create_channel(&self.channel_name.read(cx).text().to_string());
        cx.notify();
    }
    fn select_channel(&mut self, id: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.workspace.select_channel(id);
        cx.notify();
    }
}
impl Render for SidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.present(window, cx);
        let workspace = self.workspace.read();
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
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .overflow_y_scroll()
            .child(div().font_weight(FontWeight::BOLD).child("Text channels"))
            .child(
                Button::new("open-create-channel")
                    .disabled(
                        workspace.create_pending
                            || !matches!(workspace.channels, Some(Load::Ready(_))),
                    )
                    .label(if workspace.create_pending {
                        "Creating channel…"
                    } else {
                        "Create text channel"
                    })
                    .on_click(move |_, window, cx| {
                        let _ =
                            create_view.update(cx, |view, cx| view.open_create_dialog(window, cx));
                    }),
            );
        match &workspace.channels {
            None | Some(Load::Loading) => sidebar = sidebar.child("Loading channels…"),
            Some(Load::Failed(error)) => sidebar = sidebar.child(format!("Channels: {error}")),
            Some(Load::Ready(channels)) if channels.is_empty() => {
                sidebar = sidebar.child("No text channels yet.")
            }
            Some(Load::Ready(channels)) => {
                for channel in channels {
                    let id = channel.id.clone();
                    let selected = workspace.selected.as_deref() == Some(&id);
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
                            .when(selected, |button| {
                                button
                                    .bg(cx.theme().tokens.sidebar_accent.background)
                                    .text_color(cx.theme().sidebar_accent_foreground)
                            }),
                    );
                }
            }
        }
        sidebar
    }
}
