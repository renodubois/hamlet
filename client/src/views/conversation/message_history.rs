//! Displays history and owns scroll anchoring, selection and jump-to-latest controls.
//! Chat owns message data, pagination cursors and request execution.
use super::message_row::message_row_at;
mod dialog;
mod geometry;
mod menu;
use crate::api::Message;
use crate::chat::{ChatHandle, Load, OlderPages};
use crate::runtime::Execution;
use chrono::{DateTime, Local};
use geometry::{
    TailPadding, hint_history_row_heights, known_fitting_viewport, reconcile_history_list,
    tail_anchor_padding,
};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use menu::MessageMenu;
use std::time::Duration;

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
    // Explicit reader intent: layout (including height-only resize) cannot change it.
    follow_latest: bool,
    viewport: Option<Size<Pixels>>,
    presentation_revision: u64,
    tail_padding: Option<TailPadding>,
    // GPUI otherwise backfills preceding rows when a reconciled tail is shorter
    // than the viewport. This is presentation-only space, never a history row.
    protect_tail_anchor: bool,
    #[cfg(test)]
    tail_padding_layouts: usize,
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
        let notifications =
            crate::views::present_on_chat_change(&chat, cx, |view: &mut Self, window, cx| {
                view.presentation_revision = view.presentation_revision.wrapping_add(1);
                view.present_message_dialog(window, cx);
                view.present_history();
            });
        // Only invalidate presentation: do not splice/reset the list or dispatch reads.
        // Weak ownership plus the retained Task makes closing the view cancel the loop.
        let clock = execution.clone();
        let mut tick = clock.sleep(Duration::from_secs(60));
        let timestamp_refresh = cx.spawn(async move |weak, cx| {
            loop {
                tick.await;
                if weak
                    .update(cx, |view, cx| {
                        view.presentation_revision = view.presentation_revision.wrapping_add(1);
                        cx.notify();
                    })
                    .is_err()
                {
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
            follow_latest: true,
            viewport: None,
            presentation_revision: 0,
            tail_padding: None,
            protect_tail_anchor: false,
            #[cfg(test)]
            tail_padding_layouts: 0,
            presented_channel: None,
            presented_ids: Vec::new(),
            _notifications: notifications,
        };
        view.present_history();
        view
    }
    // Revealing a focused control is viewport presentation, not a message action.
    fn reveal_message_tray(&mut self, channel: &str, id: &str, cx: &mut Context<Self>) {
        if self.presented_channel.as_deref() == Some(channel)
            && let Some(ix) = self.presented_ids.iter().position(|row| row == id)
        {
            self.protect_tail_anchor = false;
            self.list.scroll_to_reveal_item(ix);
            self.follow_latest = self.list.is_scrolled_to_end().unwrap_or(false);
            cx.notify();
        }
    }

    fn present_history(&mut self) {
        let (selected, ids) = {
            let state = self.chat.read();
            let ids = state
                .selected
                .as_ref()
                .and_then(|id| state.history(id))
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
        if selected != self.presented_channel || ids != self.presented_ids {
            self.presentation_revision = self.presentation_revision.wrapping_add(1);
        }
        if selected != self.presented_channel {
            self.list.reset(ids.len());
            hint_history_row_heights(&self.list);
            self.list.scroll_to_end();
            self.follow_latest = true;
            self.protect_tail_anchor = false;
        } else if ids != self.presented_ids {
            reconcile_history_list(&self.list, &self.presented_ids, &ids);
            hint_history_row_heights(&self.list);
            if self.presented_ids.is_empty() || self.follow_latest {
                self.list.scroll_to_end();
                self.follow_latest = true;
                self.protect_tail_anchor = false;
            } else {
                self.protect_tail_anchor = !ids.is_empty();
            }
        }
        self.presented_channel = selected;
        self.presented_ids = ids;
    }

    fn viewport_padding(
        &mut self,
        rows: &[Message],
        now: &DateTime<Local>,
        viewport: Size<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Pixels {
        if self.viewport != Some(viewport) {
            self.viewport = Some(viewport);
            if self.follow_latest {
                // GPUI retains its old top on height-only resize. Apply intent
                // before this layout, even when no history IDs changed.
                self.list.scroll_to_end();
            } else {
                self.protect_tail_anchor = true;
            }
        }
        if !self.protect_tail_anchor {
            return px(0.);
        }
        let anchor = self.list.logical_scroll_top();
        if let Some(cache) = &self.tail_padding
            && cache.viewport == viewport
            && cache.revision == self.presentation_revision
            && cache.anchor.item_ix == anchor.item_ix
            && cache.anchor.offset_in_item == anchor.offset_in_item
        {
            return cache.padding;
        }
        #[cfg(test)]
        {
            self.tail_padding_layouts += 1;
        }
        let padding = tail_anchor_padding(&self.list, rows, now, viewport, window, cx);
        self.tail_padding = Some(TailPadding {
            viewport,
            revision: self.presentation_revision,
            anchor: self.list.logical_scroll_top(),
            padding,
        });
        // With no gap there is nothing to protect on unchanged frames. Future
        // splices/resizes can activate protection again without resuming follow.
        if padding == px(0.) {
            self.protect_tail_anchor = false;
        }
        padding
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
            .and_then(|id| chat.history(id))
            .is_some_and(|load| matches!(load, Load::Ready(messages) if !messages.is_empty()));
        let focus = self.focus.clone();
        let mut history = div()
            .id("history-pane")
            .test_support()
            .track_focus(&self.focus)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx)
            })
            .on_scroll_wheel(cx.listener(|view, event: &ScrollWheelEvent, _, cx| {
                let at_end = view.list.is_scrolled_to_end() == Some(true)
                    || (view.list.is_scrolled_to_end().is_none()
                        && known_fitting_viewport(&view.list));
                // GPUI invalidates all height hints when the list first learns its width.
                // Re-seed offscreen estimates after that layout, without measuring all rows.
                if view.list.is_scrolled_to_end().is_none() {
                    hint_history_row_heights(&view.list);
                }
                let down = event.delta.pixel_delta(px(20.)).y < px(0.);
                // Upward wheels retain a protected reader's position and reduce
                // the gap. A deliberate downward wheel at either a padded end or
                // a known fitting viewport resumes following.
                if down && at_end {
                    view.protect_tail_anchor = false;
                    view.follow_latest = true;
                    view.list.scroll_to_end();
                } else if !view.protect_tail_anchor
                    && let Some(follow) = view.list.is_scrolled_to_end()
                {
                    view.follow_latest = follow;
                }
                cx.notify();
            }))
            .when(has_rows, |history| history.flex_1())
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2();
        if let Some(id) = chat.selected.as_deref() {
            match chat.history(id) {
                None | Some(Load::Loading) => history = history.child("Loading conversation…"),
                Some(Load::Failed(error)) => {
                    history = history.child(format!("Conversation: {error}"))
                }
                Some(Load::Ready(messages)) => {
                    match chat.older(id) {
                        Some(OlderPages::Loading) if messages.is_empty() => {
                            history = history.child(
                                div()
                                    .id("older-loading")
                                    .aria_label("Loading older messages…")
                                    .test_support()
                                    .child("Loading older messages…"),
                            );
                        }
                        Some(OlderPages::Loading) => {}
                        Some(OlderPages::Failed(error)) => {
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
                        Some(OlderPages::Available) if messages.is_empty() => {
                            history = history.child(
                                Button::new("load-older")
                                    .label("Load older messages")
                                    .on_click(
                                        cx.listener(|view, _, _, _| view.chat.request_older()),
                                    ),
                            );
                        }
                        Some(OlderPages::Exhausted) => {}
                        _ => {}
                    }
                    if messages.is_empty() {
                        if matches!(chat.older(id), None | Some(OlderPages::Exhausted)) {
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
                    let list_state = self.list.clone();
                    history = history.child(
                        div()
                            .id("history")
                            .test_support()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .child(
                                canvas(
                                    move |bounds, window, cx| {
                                        let padding = view
                                            .update(cx, |view, cx| {
                                                view.viewport_padding(
                                                    &rows,
                                                    &now,
                                                    bounds.size,
                                                    window,
                                                    cx,
                                                )
                                            })
                                            .unwrap_or(px(0.));
                                        let mut element =
                                            list(list_state, move |ix, window, cx| {
                                                Self::message_with_menu(
                                                    &rows[ix],
                                                    &now,
                                                    &handle,
                                                    view.clone(),
                                                    window,
                                                    cx,
                                                )
                                            })
                                            .w(bounds.size.width)
                                            .h(bounds.size.height)
                                            .pb(padding)
                                            .into_any_element();
                                        element.layout_as_root(
                                            bounds.size.map(AvailableSpace::Definite),
                                            window,
                                            cx,
                                        );
                                        element.prepaint_at(bounds.origin, window, cx);
                                        element
                                    },
                                    |_, mut element, window, cx| element.paint(window, cx),
                                )
                                .size_full(),
                            )
                            .when(!self.follow_latest, |history| {
                                // Stay out of layout so visibility cannot move the reader's viewport.
                                history.child(div().absolute().bottom_3().right_3().child(
                                    Button::new("jump-latest").label("Jump to latest").on_click(
                                        cx.listener(|view, _, _, cx| {
                                            view.list.scroll_to_end();
                                            view.follow_latest = true;
                                            view.protect_tail_anchor = false;
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
#[path = "tests/message_delete.rs"]
mod message_delete_tests;

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
