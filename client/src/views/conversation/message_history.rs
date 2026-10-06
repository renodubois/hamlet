//! Displays history and owns scroll anchoring, selection and jump-to-latest controls.
//! Workspace owns message data, pagination cursors and request execution.
use super::message_row::message_row;
use crate::workspace::{Load, Older, WorkspaceHandle};
use gpui_kit::base::Disableable;
use gpui_kit::component::button::Button;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

fn hint_history_row_heights(list: &ListState) {
    list.clone().with_uniform_item_height(px(64.));
}
// Chronological stable-ID splices preserve variable-height viewport anchors.
fn reconcile_history_list(list: &ListState, before: &[String], after: &[String]) {
    let mut old = 0;
    let mut new = 0;
    while old < before.len() {
        let start = new;
        while new < after.len() && after[new] != before[old] {
            new += 1;
        }
        if new == after.len() {
            list.splice(0..list.item_count(), after.len());
            return;
        }
        if new > start {
            list.splice(start..start, new - start);
        }
        old += 1;
        new += 1;
    }
    if new < after.len() {
        list.splice(new..new, after.len() - new);
    }
}

pub(crate) struct MessageHistoryView {
    workspace: WorkspaceHandle,
    focus: FocusHandle,
    list: ListState,
    presented_channel: Option<String>,
    // Row indices only, never a second history store or continuity model.
    presented_ids: Vec<String>,
    _notifications: Task<()>,
}
impl MessageHistoryView {
    pub(crate) fn new(
        workspace: WorkspaceHandle,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let list = ListState::new(0, ListAlignment::Top, px(0.));
        let weak = cx.entity().downgrade();
        list.set_scroll_handler(move |event, _, cx| {
            if event.is_scrolled {
                let _ = weak.update(cx, |view, cx| {
                    if event.count > 0 && event.visible_range.start <= 1 {
                        view.workspace.request_older();
                    }
                    cx.notify();
                });
            }
        });
        let changes = workspace.notifications();
        let notifications = cx.spawn(async move |weak, cx| {
            while changes.recv().await.is_ok() {
                if weak
                    .update(cx, |view, cx| {
                        view.present_history();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut view = Self {
            workspace,
            focus: cx.focus_handle(),
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
            let state = self.workspace.read();
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
            if self.presented_ids.is_empty() || (follow && self.presented_ids.last() != ids.last())
            {
                self.list.scroll_to_end();
            }
        }
        self.presented_channel = selected;
        self.presented_ids = ids;
    }
}

impl Render for MessageHistoryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.present_history();
        let workspace = self.workspace.read();
        // Only a populated list consumes spare height, as in the original combined pane.
        let has_rows = workspace
            .selected
            .as_ref()
            .and_then(|id| workspace.history_for_display(id))
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
        if let Some(id) = workspace.selected.as_deref() {
            match workspace.history_for_display(id) {
                None | Some(Load::Loading) => history = history.child("Loading conversation…"),
                Some(Load::Failed(error)) => {
                    history = history.child(format!("Conversation: {error}"))
                }
                Some(Load::Ready(messages)) if messages.is_empty() => {
                    history = history.child("No messages in this channel yet.")
                }
                Some(Load::Ready(messages)) => {
                    match workspace.older.get(id) {
                        Some(Older::Loading) => history = history.child("Loading older messages…"),
                        Some(Older::Failed(error)) => {
                            history = history.child(
                                div().child(format!("Older messages: {error}")).child(
                                    Button::new("retry-older")
                                        .label("Retry older messages")
                                        .on_click(cx.listener(|view, _, _, _| {
                                            view.workspace.retry_older()
                                        })),
                                ),
                            );
                        }
                        Some(Older::Exhausted) => history = history.child("Start of conversation."),
                        _ => {}
                    }
                    history = history.child(
                        Button::new("jump-latest")
                            .label("Jump to latest")
                            .disabled(self.list.is_scrolled_to_end() != Some(false))
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.list.scroll_to_end();
                                cx.notify();
                            })),
                    );
                    // Server order is newest-first. Reverse only for chronological presentation.
                    let rows = messages.iter().rev().cloned().collect::<Vec<_>>();
                    history = history.child(
                        div().id("history").test_support().flex_1().min_h_0().child(
                            list(self.list.clone(), move |ix, _, cx| {
                                message_row(&rows[ix], cx).into_any_element()
                            })
                            .size_full(),
                        ),
                    );
                }
            }
        } else if !matches!(workspace.channels, Some(Load::Ready(ref items)) if items.is_empty()) {
            history = history.child("Select a text channel to read its conversation.");
        }
        history
    }
}
