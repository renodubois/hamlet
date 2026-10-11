//! Displays history and owns scroll anchoring, selection and jump-to-latest controls.
//! Chat owns message data, pagination cursors and request execution.
use super::message_row::message_row_at;
#[path = "message_dialog.rs"]
mod message_dialog;
#[path = "message_menu.rs"]
mod message_menu;
use crate::chat::{ChatHandle, Load, Older};
use crate::runtime::Execution;
use chrono::{DateTime, Local};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use message_menu::MessageMenu;
use std::time::Duration;

fn hint_history_row_heights(list: &ListState) {
    list.clone().with_uniform_item_height(px(64.));
}
// Chronological stable-ID splices preserve variable-height viewport anchors.
fn reconcile_history_list(list: &ListState, before: &[String], after: &[String]) {
    let positions: std::collections::HashMap<_, _> = after
        .iter()
        .enumerate()
        .map(|(ix, id)| (id.as_str(), ix))
        .collect();
    let anchor = list.logical_scroll_top();
    let surviving_anchor = before
        .iter()
        .enumerate()
        .filter_map(|(ix, id)| positions.get(id.as_str()).map(|&new_ix| (ix, new_ix)))
        // Prefer the following row on equal distance; otherwise use the nearest
        // survivor in the previously displayed chronology, not a newly inserted row.
        .min_by_key(|&(ix, _)| (ix.abs_diff(anchor.item_ix), ix < anchor.item_ix))
        .filter(|_| anchor.item_ix < before.len())
        .map(|(old_ix, new_ix)| {
            let offset_in_item = if old_ix == anchor.item_ix {
                anchor.offset_in_item
            } else {
                // A deleted row's partial offset may exceed its replacement's
                // height. Keep at least one pixel of the measured survivor visible.
                // bounds_for_item cannot measure an offscreen preceding survivor;
                // start that fallback at its top instead of guessing its height.
                list.bounds_for_item(old_ix)
                    .map(|bounds| {
                        anchor
                            .offset_in_item
                            .min((bounds.size.height - px(1.)).max(px(0.)))
                    })
                    .unwrap_or(px(0.))
            };
            ListOffset {
                item_ix: new_ix,
                offset_in_item,
            }
        });
    let mut old = 0;
    let mut new = 0;
    // Common IDs stay in chronological order. Splice only the gaps between
    // survivors, retaining their measured heights and GPUI viewport anchors.
    for (old_ix, id) in before.iter().enumerate() {
        let Some(&new_ix) = positions.get(id.as_str()) else {
            continue;
        };
        if old_ix > old || new_ix > new {
            list.splice(new..new + old_ix - old, new_ix - new);
        }
        old = old_ix + 1;
        new = new_ix + 1;
    }
    if old < before.len() || new < after.len() {
        list.splice(new..new + before.len() - old, after.len() - new);
    }
    if let Some(anchor) = surviving_anchor {
        list.scroll_to(anchor);
    }
}

pub(super) struct MessageHistoryView {
    chat: ChatHandle,
    execution: Execution,
    _timestamp_refresh: Task<()>,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    menu_focus: FocusHandle,
    menu_item_focus: FocusHandle,
    message_menu: Option<MessageMenu>,
    dialog_open: bool,
    _release: Subscription,
    list: ListState,
    presented_channel: Option<String>,
    // Row indices only, never a second history store or continuity model.
    presented_ids: Vec<String>,
    _notifications: Task<()>,
}
impl MessageHistoryView {
    pub(super) fn new(
        chat: ChatHandle,
        execution: Execution,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let list = ListState::new(0, ListAlignment::Top, px(0.));
        let weak = cx.entity().downgrade();
        list.set_scroll_handler(move |event, _, cx| {
            if event.is_scrolled {
                let _ = weak.update(cx, |view, cx| {
                    if event.count > 0 && event.visible_range.start <= 1 {
                        view.chat.request_older();
                    }
                    cx.notify();
                });
            }
        });
        let changes = chat.notifications();
        let notifications = cx.spawn(async move |weak, cx| {
            while changes.recv().await.is_ok() {
                if weak
                    .update_in(cx, |view, window, cx| {
                        view.present_message_dialog(window, cx);
                        view.present_history();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        // Only invalidate presentation: do not splice/reset the list or dispatch reads.
        // Weak ownership plus the retained Task makes closing the view cancel the loop.
        let clock = execution.clone();
        let mut tick = clock.sleep(Duration::from_secs(60));
        let timestamp_refresh = cx.spawn(async move |weak, cx| {
            loop {
                tick.await;
                if weak.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
                tick = clock.sleep(Duration::from_secs(60));
            }
        });
        let release = cx.on_release_in(window, |view, window, cx| {
            view.dismiss_message_menu(window, cx);
            if view.dialog_open {
                view.chat.cancel_message_deletion();
                window.close_dialog(cx);
            }
        });
        let mut view = Self {
            chat,
            execution,
            _timestamp_refresh: timestamp_refresh,
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            menu_focus: cx.focus_handle(),
            menu_item_focus: cx.focus_handle(),
            message_menu: None,
            dialog_open: false,
            _release: release,
            list,
            presented_channel: None,
            presented_ids: Vec::new(),
            _notifications: notifications,
        };
        view.present_history();
        view
    }
    fn present_history(&mut self) {
        let (selected, ids) = {
            let state = self.chat.read();
            let ids = state
                .selected
                .as_ref()
                .and_then(|id| state.history_for_display(id))
                .and_then(|load| {
                    if let Load::Ready(messages) = load {
                        Some(
                            messages
                                .iter()
                                .rev()
                                .map(|m| m.id.clone())
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            (state.selected.clone(), ids)
        };
        let follow = self.list.is_scrolled_to_end() != Some(false);
        if selected != self.presented_channel {
            self.list.reset(ids.len());
            hint_history_row_heights(&self.list);
            self.list.scroll_to_end();
        } else if ids != self.presented_ids {
            reconcile_history_list(&self.list, &self.presented_ids, &ids);
            hint_history_row_heights(&self.list);
            if self.presented_ids.is_empty() || follow {
                self.list.scroll_to_end();
            }
        }
        self.presented_channel = selected;
        self.presented_ids = ids;
    }
}

impl Render for MessageHistoryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.present_message_menu(window, cx);
        self.present_history();
        let menu = self.render_message_menu(window, cx);
        let chat = self.chat.read();
        // Only a populated list consumes spare height, as in the original combined pane.
        let has_rows = chat
            .selected
            .as_ref()
            .and_then(|id| chat.history_for_display(id))
            .is_some_and(|load| matches!(load, Load::Ready(messages) if !messages.is_empty()));
        let focus = self.focus.clone();
        let list_for_wheel = self.list.clone();
        let mut history = div()
            .id("history-pane")
            .test_support()
            .track_focus(&self.focus)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx)
            })
            .on_scroll_wheel(move |_, _, _| {
                // GPUI invalidates all height hints when the list first learns its width.
                // Re-seed offscreen estimates after that layout, without measuring all rows.
                if list_for_wheel.is_scrolled_to_end().is_none() {
                    hint_history_row_heights(&list_for_wheel);
                }
            })
            .when(has_rows, |history| history.flex_1())
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2();
        if let Some(id) = chat.selected.as_deref() {
            match chat.history_for_display(id) {
                None | Some(Load::Loading) => history = history.child("Loading conversation…"),
                Some(Load::Failed(error)) => {
                    history = history.child(format!("Conversation: {error}"))
                }
                Some(Load::Ready(messages)) => {
                    match chat.older.get(id) {
                        Some(Older::Loading) if messages.is_empty() => {
                            history = history.child(
                                div()
                                    .id("older-loading")
                                    .aria_label("Loading older messages…")
                                    .test_support()
                                    .child("Loading older messages…"),
                            );
                        }
                        Some(Older::Loading) => {}
                        Some(Older::Failed(error)) => {
                            history = history.child(
                                div().child(format!("Older messages: {error}")).child(
                                    Button::new("retry-older")
                                        .label("Retry older messages")
                                        .on_click(
                                            cx.listener(|view, _, _, _| view.chat.retry_older()),
                                        ),
                                ),
                            );
                        }
                        Some(Older::Available) if messages.is_empty() => {
                            history = history.child(
                                Button::new("load-older")
                                    .label("Load older messages")
                                    .on_click(
                                        cx.listener(|view, _, _, _| view.chat.request_older()),
                                    ),
                            );
                        }
                        Some(Older::Exhausted) => {}
                        _ => {}
                    }
                    if messages.is_empty() {
                        if matches!(chat.older.get(id), None | Some(Older::Exhausted)) {
                            history = history.child(
                                div()
                                    .id("history-empty")
                                    .aria_label("No messages in this channel yet.")
                                    .test_support()
                                    .child("No messages in this channel yet."),
                            );
                        }
                        return history;
                    }
                    // Server order is newest-first. Reverse only for chronological presentation.
                    let rows = messages.iter().rev().cloned().collect::<Vec<_>>();
                    let now = DateTime::from_timestamp(self.execution.unix_seconds(), 0)
                        .expect("viewer clock in supported range")
                        .with_timezone(&Local);
                    let handle = self.chat.clone();
                    let view = cx.entity().downgrade();
                    history = history.child(
                        div()
                            .id("history")
                            .test_support()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .child(
                                list(self.list.clone(), move |ix, _, cx| {
                                    Self::message_with_menu(
                                        &rows[ix],
                                        &now,
                                        &handle,
                                        view.clone(),
                                        cx,
                                    )
                                })
                                .size_full(),
                            )
                            .when(self.list.is_scrolled_to_end() == Some(false), |history| {
                                // Stay out of layout so visibility cannot move the reader's viewport.
                                history.child(div().absolute().bottom_3().right_3().child(
                                    Button::new("jump-latest").label("Jump to latest").on_click(
                                        cx.listener(|view, _, _, cx| {
                                            view.list.scroll_to_end();
                                            cx.notify();
                                        }),
                                    ),
                                ))
                            }),
                    );
                }
            }
        } else if !matches!(chat.channels, Some(Load::Ready(ref items)) if items.is_empty()) {
            history = history.child("Select a text channel to read its conversation.");
        }
        history.children(menu)
    }
}

#[cfg(test)]
#[path = "tests/message_deletion.rs"]
mod message_deletion_tests;

#[cfg(test)]
#[path = "tests/history_removals.rs"]
mod history_removal_tests;

#[cfg(test)]
#[path = "tests/history_lifecycle.rs"]
mod history_lifecycle_tests;

#[cfg(test)]
#[path = "tests/timestamp_refresh.rs"]
mod timestamp_refresh_tests;

#[cfg(test)]
#[path = "tests/support/history.rs"]
mod history_test_support;
