//! Channel controls own their input/focus and subscription; policy belongs to workspace.
use crate::theme;
use crate::workspace::{Load, WorkspaceHandle};
use gpui_kit::base::Disableable;
use gpui_kit::base::input::{InputBaseState, InputMode, InputState};
use gpui_kit::component::{
    ActiveTheme, Icon, WindowExt as _,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::Input,
    menu::{ContextMenuExt, PopupMenuItem},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

enum ChannelDialog {
    Rename(String),
    Delete,
}

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
    fn open_channel_dialog(
        &mut self,
        action: ChannelDialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog_open {
            return;
        }
        self.dialog_open = true;
        // This input belongs only to the dialog; closing it discards the edit.
        let input = match &action {
            ChannelDialog::Rename(name) => Some(cx.new(|cx| {
                let mut input = InputState::new(window, cx);
                input.set_value(name.clone(), window, cx);
                input
            })),
            ChannelDialog::Delete => None,
        };
        let rename_input = input.clone();
        let view = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let (title, confirm_id, confirm_label) = match &action {
                ChannelDialog::Rename(_) => ("Rename channel", "confirm-rename-channel", "Rename"),
                ChannelDialog::Delete => ("Delete channel", "confirm-delete-channel", "Delete"),
            };
            let content = if let Some(input) = &input {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child("Channel name")
                    .child(
                        Input::new(input)
                            .id("rename-channel-name")
                            .aria_label("Channel name"),
                    )
                    .into_any_element()
            } else {
                div()
                    .id("delete-channel-confirmation")
                    .aria_label("Are you sure you want to delete this channel?")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .child("Are you sure you want to delete this channel?"),
                    )
                    .child(
                        div()
                            .id("delete-channel-warning")
                            .aria_label("This action cannot be undone.")
                            .test_support()
                            .font_weight(FontWeight::BOLD)
                            .text_color(cx.theme().danger)
                            .child("This action cannot be undone."),
                    )
                    .into_any_element()
            };
            let cancel = view.clone();
            let confirm = view.clone();
            let closed = view.clone();
            dialog
                .title(title)
                // Backend support is absent: confirmation only dismisses the dialog.
                .on_ok(|_, _, _| true)
                .child(content)
                .footer(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("cancel-channel-action")
                                .label("Cancel")
                                .on_click(move |_, window, cx| {
                                    let _ = cancel.update(cx, |view, cx| {
                                        view.dialog_open = false;
                                        cx.notify();
                                    });
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new(confirm_id)
                                .label(confirm_label)
                                .when(matches!(action, ChannelDialog::Delete), |button| {
                                    button.danger()
                                })
                                .on_click(move |_, window, cx| {
                                    let _ = confirm.update(cx, |view, cx| {
                                        view.dialog_open = false;
                                        cx.notify();
                                    });
                                    window.close_dialog(cx);
                                }),
                        ),
                )
                .on_close(move |_, _, cx| {
                    let _ = closed.update(cx, |view, cx| {
                        view.dialog_open = false;
                        cx.notify();
                    });
                })
        });
        if let Some(input) = rename_input {
            input.update(cx, |input, cx| input.focus(window, cx));
        }
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
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .overflow_y_scroll();
        let mut channel_container = div().flex_1();
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
                    let label = channel.name.clone();
                    let target = view.clone();
                    let menu_view = view.clone();
                    let channel_name = channel.name.clone();
                    let label_id = format!("channel-label-{id}");
                    channel_container = channel_container.child(
                        Button::new(format!("channel-{id}"))
                            .px_1()
                            .icon(Icon::new(theme::channel_icon()))
                            .custom(
                                ButtonCustomVariant::new(cx)
                                    .color(cx.theme().sidebar)
                                    .foreground(cx.theme().sidebar_foreground)
                                    .hover(cx.theme().sidebar_accent)
                                    .active(cx.theme().sidebar_accent),
                            )
                            .accessibility_label(label.clone())
                            // Kit centers its built-in icon/label slot. A full-width
                            // child keeps navigation content aligned to the left.
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .justify_start()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id(label_id)
                                            .test_support()
                                            .min_w_0()
                                            .text_ellipsis()
                                            .line_height(relative(1.))
                                            .child(label),
                                    ),
                            )
                            .on_click(move |_, window, cx| {
                                let _ = target
                                    .update(cx, |view, cx| view.select_channel(&id, window, cx));
                            })
                            .when(selected, |button| {
                                button
                                    .bg(cx.theme().tokens.sidebar_accent.background)
                                    .text_color(cx.theme().sidebar_accent_foreground)
                            })
                            .context_menu(move |menu, _, _| {
                                let rename = menu_view.clone();
                                let delete = menu_view.clone();
                                let name = channel_name.clone();
                                menu.item(PopupMenuItem::new("Rename channel").on_click(
                                    move |_, window, cx| {
                                        let rename = rename.clone();
                                        let name = name.clone();
                                        // Let the menu restore focus before opening the modal.
                                        window.defer(cx, move |window, cx| {
                                            let _ = rename.update(cx, |view, cx| {
                                                view.open_channel_dialog(
                                                    ChannelDialog::Rename(name),
                                                    window,
                                                    cx,
                                                );
                                            });
                                        });
                                    },
                                ))
                                .item(
                                    PopupMenuItem::element(|_, cx| {
                                        div()
                                            .id("delete-channel-label")
                                            .aria_label("Delete channel")
                                            .test_support()
                                            .text_color(cx.theme().danger)
                                            .child("Delete channel")
                                    })
                                    .on_click(
                                        move |_, window, cx| {
                                            let delete = delete.clone();
                                            window.defer(cx, move |window, cx| {
                                                let _ = delete.update(cx, |view, cx| {
                                                    view.open_channel_dialog(
                                                        ChannelDialog::Delete,
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            });
                                        },
                                    ),
                                )
                            }),
                    );
                }
            }
        }
        sidebar = sidebar.child(channel_container);

        sidebar = sidebar.child(
            Button::new("open-create-channel")
                .disabled(
                    workspace.create_pending || !matches!(workspace.channels, Some(Load::Ready(_))),
                )
                .label(if workspace.create_pending {
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
