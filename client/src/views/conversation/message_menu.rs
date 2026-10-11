//! A single authored-message action, not a generic menu framework. The history
//! view owns only pointer geometry/focus/presentation; Chat still validates and
//! captures the deletion target when the shared confirmation seam is invoked.
use super::MessageHistoryView;
use gpui_kit::base::{Button, DeferredPopover, GlobalState, POPUP_PRIORITY};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub(super) struct MessageMenu {
    channel: String,
    id: String,
    position: Point<Pixels>,
    previous_focus: Option<FocusHandle>,
    _interaction: DeferredPopover,
}

fn message_menu_text() -> AnyElement {
    #[cfg(test)]
    return crate::views::menu_text_probe::MenuText("Delete Message").into_any_element();
    #[cfg(not(test))]
    "Delete Message".into_any_element()
}

impl MessageHistoryView {
    pub(super) fn open_message_menu(
        &mut self,
        channel: String,
        id: String,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog_open {
            return;
        }
        // Reopening must retain the interrupted focus, not the old menu focus.
        let previous_focus = self
            .message_menu
            .take()
            .map(|menu| menu.previous_focus)
            .unwrap_or_else(|| window.focused(cx));
        self.message_menu = Some(MessageMenu {
            channel,
            id,
            position,
            previous_focus,
            _interaction: GlobalState::register_deferred_popover(cx),
        });
        window.focus(&self.menu_focus, cx);
        cx.notify();
    }

    pub(super) fn dismiss_message_menu(&mut self, window: &mut Window, cx: &mut App) {
        let Some(menu) = self.message_menu.take() else {
            return;
        };
        // Never steal focus from a dialog or another control that already took it.
        if (self.menu_focus.contains_focused(window, cx) || self.menu_item_focus.is_focused(window))
            && let Some(focus) = menu.previous_focus
        {
            window.focus(&focus, cx);
        }
    }

    fn activate_message_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.message_menu.as_ref() else {
            return;
        };
        let channel = menu.channel.clone();
        let id = menu.id.clone();
        self.dismiss_message_menu(window, cx);
        cx.notify();
        let view = cx.entity().downgrade();
        // Let menu removal/focus restoration finish before opening confirmation.
        window.defer(cx, move |window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.open_message_dialog(&channel, &id, window, cx)
            });
        });
    }

    pub(super) fn present_message_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let invalid = self.message_menu.as_ref().is_some_and(|menu| {
            let state = self.chat.read();
            state.selected.as_deref() != Some(menu.channel.as_str())
                || !state.history_for_display(&menu.channel).is_some_and(|load| {
                    matches!(load, crate::chat::Load::Ready(messages)
                        if messages.iter().any(|message| message.id == menu.id && self.chat.owns_message(message)))
                })
        });
        if invalid {
            self.dismiss_message_menu(window, cx);
        }
    }

    pub(super) fn render_message_menu(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.message_menu.as_ref()?;
        let selected = self.menu_item_focus.is_focused(window);
        let surface = div()
            .id("message-context-menu")
            .role(Role::Menu)
            .aria_label("Message actions")
            .test_support()
            .track_focus(&self.menu_focus)
            .tab_group()
            .occlude()
            .w(px(180.))
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .text_sm()
            .shadow_md()
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "down" | "up" | "home" | "end" | "tab" => {
                        window.focus(&view.menu_item_focus, cx);
                        window.prevent_default();
                        cx.stop_propagation();
                        cx.notify();
                    }
                    "escape" => {
                        view.dismiss_message_menu(window, cx);
                        cx.notify();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    _ => {}
                }
            }))
            .child(
                Button::new("delete-message-label")
                    .role(Role::MenuItem)
                    .accessibility_label("Delete Message")
                    .track_focus(&self.menu_item_focus)
                    .tab_stop(false)
                    .w_full()
                    .justify_start()
                    .px_2()
                    .py_2()
                    .rounded_sm()
                    .text_color(cx.theme().danger)
                    .when(selected, |button| button.bg(cx.theme().accent))
                    .hover(|style| style.bg(cx.theme().accent))
                    .on_mouse_move(cx.listener(|view, _, window, cx| {
                        window.focus(&view.menu_item_focus, cx);
                        cx.notify();
                    }))
                    .on_click(cx.listener(|view, _, window, cx| {
                        cx.stop_propagation();
                        view.activate_message_menu(window, cx);
                    }))
                    .child(message_menu_text()),
            );
        // Pointer coordinates are window-local, not row-local. Only this overlay
        // is window-anchored; the separate tray remains relative to its row.
        Some(
            deferred(
                anchored()
                    .position(point(px(0.), px(0.)))
                    .position_mode(AnchoredPositionMode::Window)
                    .child(
                        div()
                            .w(window.bounds().size.width)
                            .h(window.bounds().size.height)
                            .occlude()
                            .on_any_mouse_down(cx.listener(|view, _, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                view.dismiss_message_menu(window, cx);
                                cx.notify();
                            }))
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                            .child(
                                anchored()
                                    .position(menu.position)
                                    .position_mode(AnchoredPositionMode::Window)
                                    .snap_to_window_with_margin(px(8.))
                                    .child(surface),
                            ),
                    ),
            )
            .with_priority(POPUP_PRIORITY)
            .into_any_element(),
        )
    }
}
