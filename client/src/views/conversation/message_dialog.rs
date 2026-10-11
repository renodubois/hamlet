//! Message menu/tray presentation and the shared confirmation opening seam.
use super::{MessageHistoryView, message_row_at};
use crate::api::Message;
use chrono::{DateTime, Local};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    ActiveTheme, Icon, WindowExt as _,
    button::{Button, ButtonVariants},
    tooltip::Tooltip,
};
use gpui_kit::*;

impl MessageHistoryView {
    pub(super) fn present_message_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_open && self.chat.read().message_deletion.is_none() {
            self.dialog_open = false;
            window.close_dialog(cx);
        }
    }

    pub(super) fn message_with_menu(
        message: &Message,
        now: &DateTime<Local>,
        chat: &crate::chat::ChatHandle,
        view: WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        let own = chat.owns_message(message);
        let channel = message.channel_id.clone();
        let id = message.id.clone();
        let row = div()
            .id(format!("message-actions-{id}"))
            .child(message_row_at(message, now, cx));
        if !own {
            return row.into_any_element();
        }
        let tray_view = view.clone();
        let tray_channel = channel.clone();
        let tray_id = id.clone();
        let group = SharedString::from(format!("message-hover-{id}"));
        let ring = cx.theme().ring;
        let tooltip_id = format!("trash-message-tooltip-{id}");
        // Absolute, row-local geometry keeps the tray inside the list's pane and
        // inside the row's hover hitbox. Opacity keeps its keyboard tab stop
        // available even before hover; focus reveals the same real control.
        row.relative()
            .group(group.clone())
            .child(
                gpui_kit::base::Button::new(format!("trash-message-{id}"))
                    .accessibility_label("Delete Message")
                    .absolute()
                    .top_0()
                    .right_2()
                    .size_8()
                    .rounded_md()
                    .bg(cx.theme().popover)
                    .text_color(cx.theme().danger)
                    .opacity(0.)
                    .group_hover(group, |style| style.opacity(1.))
                    .focus(|style| style.opacity(1.))
                    .focus_visible(|style| style.border_2().border_color(ring))
                    .tooltip(move |window, cx| {
                        let id = tooltip_id.clone();
                        Tooltip::element(move |_, _| {
                            div()
                                .id(id.clone())
                                .aria_label("Delete Message")
                                .test_support()
                                .child("Delete Message")
                        })
                        .build(window, cx)
                    })
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        window.prevent_default();
                        gpui_kit::base::GlobalState::suppress_text_selection(cx);
                        cx.stop_propagation();
                    })
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        let _ = tray_view.update(cx, |view, cx| {
                            view.open_message_dialog(&tray_channel, &tray_id, window, cx)
                        });
                    })
                    .child(Icon::new(gpui_kit::assets::IconName::Trash).size_4()),
            )
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                let _ = view.update(cx, |view, cx| {
                    view.open_message_menu(channel.clone(), id.clone(), event.position, window, cx);
                });
            })
            .into_any_element()
    }

    /// Shared opening seam for the context menu and tray; Chat captures and validates.
    pub(super) fn open_message_dialog(
        &mut self,
        channel: &str,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog_open
            || window.has_active_dialog(cx)
            || !self.chat.open_message_deletion(channel, id)
        {
            return;
        }
        self.dialog_open = true;
        let view = cx.entity().downgrade();
        let cancel_focus = self.cancel_focus.clone();
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(entity) = view.upgrade() else {
                return dialog;
            };
            let chat = entity.read(cx).chat.clone();
            let state = chat.read();
            let Some(target) = &state.message_deletion else {
                return dialog;
            };
            let pending = target.pending;
            let cancel = chat.clone();
            let confirm = chat.clone();
            let keyboard = chat.clone();
            let closed = view.clone();
            let keyboard_focus = cancel_focus.clone();
            let mut content = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .id("delete-message-preview")
                        .aria_label(target.preview.clone())
                        .test_support()
                        .max_h(px(120.))
                        .overflow_hidden()
                        .child(target.preview.clone()),
                )
                .child(
                    div()
                        .id("delete-message-warning")
                        .aria_label("You can't restore this message in the app.")
                        .test_support()
                        .child("You can't restore this message in the app."),
                );
            if let Some(feedback) = &target.feedback {
                content = content.child(
                    div()
                        .id("delete-message-feedback")
                        .aria_label(feedback.clone())
                        .test_support()
                        .child(feedback.clone()),
                );
            }
            dialog
                .title("Delete Message")
                .close_button(!pending)
                .overlay_closable(!pending)
                .on_cancel(move |_, _, _| !pending)
                .on_ok(move |_, window, cx| {
                    if pending {
                        return false;
                    }
                    if keyboard_focus.is_focused(window) {
                        keyboard.cancel_message_deletion();
                        true
                    } else {
                        keyboard.confirm_message_deletion();
                        window.refresh();
                        let _ = cx;
                        false
                    }
                })
                .child(content)
                .footer(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            gpui_kit::base::Button::new("cancel-message-action")
                                .track_focus(&cancel_focus)
                                .accessibility_label("Cancel")
                                .disabled(pending)
                                .px_4()
                                .py_2()
                                .rounded_md()
                                .bg(cx.theme().secondary)
                                .text_color(cx.theme().secondary_foreground)
                                .child("Cancel")
                                .on_click(move |_, window, cx| {
                                    cancel.cancel_message_deletion();
                                    if cancel.read().message_deletion.is_none() {
                                        window.close_dialog(cx);
                                    }
                                }),
                        )
                        .child(
                            Button::new("confirm-delete-message")
                                .label(if pending {
                                    "Deleting message…"
                                } else {
                                    "Delete"
                                })
                                .danger()
                                .disabled(pending)
                                .on_click(move |_, window, _| {
                                    confirm.confirm_message_deletion();
                                    window.refresh();
                                }),
                        ),
                )
                .on_close(move |_, _, cx| {
                    let _ = closed.update(cx, |view, cx| {
                        view.chat.cancel_message_deletion();
                        view.dialog_open = false;
                        cx.notify();
                    });
                })
        });
        self.cancel_focus.focus(window, cx);
        cx.notify();
    }
}
