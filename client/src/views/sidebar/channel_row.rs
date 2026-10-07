//! Channel row presentation, context menus, and their rename/delete dialogs.
use super::SidebarView;
use crate::{api::Channel, theme};
use gpui_kit::base::{Disableable, input::InputState};
use gpui_kit::component::{
    ActiveTheme, Icon, WindowExt as _,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::Input,
    menu::{ContextMenuExt, PopupMenuItem},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

enum ChannelDialog {
    Rename(String, String),
    Delete,
}

impl SidebarView {
    pub(super) fn channel_row(
        channel: &Channel,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = channel.id.clone();
        let menu_id = id.clone();
        let label = channel.name.clone();
        let target = cx.entity().downgrade();
        let menu_view = target.clone();
        let channel_name = channel.name.clone();
        let label_id = format!("channel-label-{id}");
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
                let _ = target.update(cx, |view, cx| view.select_channel(&id, window, cx));
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
                let id = menu_id.clone();
                menu.item(
                    PopupMenuItem::new("Rename channel").on_click(move |_, window, cx| {
                        let rename = rename.clone();
                        let name = name.clone();
                        let id = id.clone();
                        // Let the menu restore focus before opening the modal.
                        window.defer(cx, move |window, cx| {
                            let _ = rename.update(cx, |view, cx| {
                                view.open_channel_dialog(
                                    ChannelDialog::Rename(id, name),
                                    window,
                                    cx,
                                );
                            });
                        });
                    }),
                )
                .item(
                    PopupMenuItem::element(|_, cx| {
                        div()
                            .id("delete-channel-label")
                            .aria_label("Delete channel")
                            .test_support()
                            .text_color(cx.theme().danger)
                            .child("Delete channel")
                    })
                    .on_click(move |_, window, cx| {
                        let delete = delete.clone();
                        window.defer(cx, move |window, cx| {
                            let _ = delete.update(cx, |view, cx| {
                                view.open_channel_dialog(ChannelDialog::Delete, window, cx);
                            });
                        });
                    }),
                )
            })
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
        if self.workspace.read().rename_pending {
            return;
        }
        self.workspace.reset_rename_feedback();
        self.rename_revision = match &action {
            ChannelDialog::Rename(..) => Some(self.workspace.read().rename_confirmed),
            ChannelDialog::Delete => None,
        };
        self.dialog_open = true;
        // This input belongs only to the dialog; closing it discards the edit.
        let input = match &action {
            ChannelDialog::Rename(_, name) => Some(cx.new(|cx| {
                let mut input = InputState::new(window, cx);
                input.set_value(name.clone(), window, cx);
                input
            })),
            ChannelDialog::Delete => None,
        };
        let rename_input = input.clone();
        let view = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(sidebar) = view.upgrade() else {
                return dialog;
            };
            let workspace = sidebar.read(cx).workspace.read();
            let pending = matches!(action, ChannelDialog::Rename(..)) && workspace.rename_pending;
            let feedback = if matches!(action, ChannelDialog::Rename(..)) {
                workspace.rename_feedback.clone()
            } else {
                None
            };
            let target = match &action {
                ChannelDialog::Rename(id, _) => Some(id.clone()),
                _ => None,
            };
            let submit_input = input.clone();
            let (title, confirm_id, confirm_label) = match &action {
                ChannelDialog::Rename(..) => ("Rename channel", "confirm-rename-channel", "Rename"),
                ChannelDialog::Delete => ("Delete channel", "confirm-delete-channel", "Delete"),
            };
            let mut content = if let Some(input) = &input {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child("Channel name")
                    .child(
                        Input::new(input)
                            .id("rename-channel-name")
                            .aria_label("Channel name")
                            .disabled(pending),
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
            if let Some(feedback) = feedback {
                content = div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(content)
                    .child(
                        div()
                            .id("rename-channel-feedback")
                            .aria_label(feedback.clone())
                            .test_support()
                            .child(feedback),
                    )
                    .into_any_element();
            }
            let cancel = view.clone();
            let confirm = view.clone();
            let closed = view.clone();
            let keyboard_submit = view.clone();
            let keyboard_input = input.clone();
            let keyboard_target = target.clone();
            dialog
                .title(title)
                .close_button(!pending)
                .overlay_closable(!pending)
                .on_cancel(move |_, _, _| !pending)
                .on_ok(move |_, _, cx| {
                    if let (Some(id), Some(input)) = (&keyboard_target, &keyboard_input) {
                        if !pending {
                            let name = input.read(cx).text().to_string();
                            let _ = keyboard_submit.update(cx, |view, cx| {
                                view.workspace.rename_channel(id, &name);
                                cx.notify();
                            });
                        }
                        false
                    } else {
                        true
                    }
                })
                .child(content)
                .footer(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("cancel-channel-action")
                                .disabled(pending)
                                .label("Cancel")
                                .on_click(move |_, window, cx| {
                                    let _ = cancel.update(cx, |view, cx| {
                                        if pending {
                                            return;
                                        }
                                        view.dialog_open = false;
                                        view.rename_revision = None;
                                        cx.notify();
                                    });
                                    if !pending {
                                        window.close_dialog(cx);
                                    }
                                }),
                        )
                        .child(
                            Button::new(confirm_id)
                                .disabled(pending)
                                .label(if pending {
                                    "Renaming channel…"
                                } else {
                                    confirm_label
                                })
                                .when(matches!(action, ChannelDialog::Delete), |button| {
                                    button.danger()
                                })
                                .on_click(move |_, window, cx| {
                                    if pending {
                                        return;
                                    }
                                    if let (Some(id), Some(input)) = (&target, &submit_input) {
                                        let name = input.read(cx).text().to_string();
                                        let _ = confirm.update(cx, |view, cx| {
                                            view.workspace.rename_channel(id, &name);
                                            cx.notify();
                                        });
                                    } else {
                                        let _ = confirm.update(cx, |view, cx| {
                                            view.dialog_open = false;
                                            cx.notify();
                                        });
                                        window.close_dialog(cx);
                                    }
                                }),
                        ),
                )
                .on_close(move |_, _, cx| {
                    let _ = closed.update(cx, |view, cx| {
                        view.dialog_open = false;
                        view.rename_revision = None;
                        cx.notify();
                    });
                })
        });
        if let Some(input) = rename_input {
            input.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }
    fn select_channel(&mut self, id: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.workspace.select_channel(id);
        cx.notify();
    }
}
