//! Temporary combined history/composer presentation. Child ownership is extracted in #59/#60.
use crate::conversation::{self, ConversationHandle, Load, Older};
use crate::theme;
use gpui_kit::base::input::{InputBaseState, InputEvent, TextareaMode, TextareaState};
use gpui_kit::base::{Disableable, SelectableText};
use gpui_kit::component::{button::Button, input::Textarea};
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

pub(crate) struct ConversationView {
    conversation: ConversationHandle,
    history_focus: FocusHandle,
    history_list: ListState,
    presented_channel: Option<String>,
    presented_ids: Vec<String>,
    // Last hydrated text only, not an authoritative draft; protects queued Kit edits.
    presented_draft: String,
    composer: Entity<InputBaseState<TextareaMode>>,
    _composer_subscription: Subscription,
    _notifications: Task<()>,
    syncing_composer: bool,
}
impl ConversationView {
    pub(crate) fn new(
        conversation: ConversationHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer = cx.new(|cx| TextareaState::new(window, cx).submit_on_enter(true));
        let composer_subscription = cx.subscribe(&composer, |view: &mut Self, _, event, cx| {
            if view.syncing_composer {
                return;
            }
            match event {
                InputEvent::Change => view.store_composer(cx),
                InputEvent::PressEnter {
                    shift: false,
                    secondary: false,
                } => {
                    view.store_composer(cx);
                    view.conversation.send();
                }
                _ => {}
            }
            cx.notify();
        });
        let history_list = ListState::new(0, ListAlignment::Top, px(0.));
        let weak = cx.entity().downgrade();
        history_list.set_scroll_handler(move |event, _, cx| {
            if event.is_scrolled {
                let _ = weak.update(cx, |view, cx| {
                    if event.count > 0 && event.visible_range.start <= 1 {
                        view.request_older(cx);
                    }
                    cx.notify();
                });
            }
        });
        let changes = conversation.notifications();
        let notifications = cx.spawn(async move |weak, cx| {
            while changes.recv().await.is_ok() {
                if weak
                    .update_in(cx, |view, window, cx| {
                        view.present_conversation(window, cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut view = Self {
            conversation,
            history_focus: cx.focus_handle(),
            history_list,
            presented_channel: None,
            presented_ids: Vec::new(),
            presented_draft: String::new(),
            composer,
            _composer_subscription: composer_subscription,
            _notifications: notifications,
            syncing_composer: false,
        };
        view.present_conversation(window, cx);
        view
    }
    fn store_composer(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().to_string();
        if text != self.presented_draft
            && let Some(channel) = &self.presented_channel
        {
            let is_selected = self.conversation.read().selected.as_deref() == Some(channel);
            if is_selected {
                self.conversation.edit_draft(text.clone());
            } else {
                self.conversation.edit_channel_draft(channel, text.clone());
            }
            self.presented_draft = text;
        }
    }
    fn sync_composer(&mut self, replaced: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = {
            let state = self.conversation.read();
            state
                .selected
                .as_deref()
                .map(|id| state.draft(id))
                .unwrap_or("")
                .to_owned()
        };
        if !replaced && text == self.presented_draft {
            return;
        }
        self.presented_draft = text.clone();
        if self.composer.read(cx).text() == text.as_str() {
            return;
        }
        self.syncing_composer = true;
        self.composer
            .update(cx, |input, cx| input.set_value(&text, window, cx));
        self.syncing_composer = false;
    }
    fn send_message(&mut self, cx: &mut Context<Self>) {
        self.store_composer(cx);
        self.conversation.send();
        cx.notify();
    }
    /// Presentation-only reconciliation. Feature state is authoritative; these IDs index rows.
    fn present_conversation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (selected, ids) = {
            let state = self.conversation.read();
            let ids = state
                .selected
                .as_ref()
                .and_then(|id| state.history.get(id))
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
        let follow = self.history_list.is_scrolled_to_end() != Some(false);
        let replaced = selected != self.presented_channel;
        if replaced {
            self.store_composer(cx);
            self.history_list.reset(ids.len());
            hint_history_row_heights(&self.history_list);
            self.history_list.scroll_to_end();
        } else if ids != self.presented_ids {
            reconcile_history_list(&self.history_list, &self.presented_ids, &ids);
            hint_history_row_heights(&self.history_list);
            if self.presented_ids.is_empty() || (follow && self.presented_ids.last() != ids.last())
            {
                self.history_list.scroll_to_end();
            }
        }
        self.presented_channel = selected;
        self.presented_ids = ids;
        self.sync_composer(replaced, window, cx);
    }
    fn request_older(&mut self, cx: &mut Context<Self>) {
        self.conversation.request_older();
        cx.notify();
    }
    fn refresh_history(&mut self, cx: &mut Context<Self>) {
        self.conversation.refresh_history();
        cx.notify();
    }
    fn retry_older(&mut self, cx: &mut Context<Self>) {
        self.conversation.retry_older();
        cx.notify();
    }
    fn jump_latest(&mut self, cx: &mut Context<Self>) {
        self.history_list.scroll_to_end();
        cx.notify();
    }
}

impl Render for ConversationView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.present_conversation(window, cx);
        let conversation = self.conversation.read();
        let view = cx.entity().downgrade();
        let selected = conversation.selected.as_deref();
        let focus = self.history_focus.clone();
        let list_for_wheel = self.history_list.clone();
        let mut history = div()
            .id("history-pane")
            .test_support()
            .track_focus(&self.history_focus)
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
            .flex_1()
            .h_full()
            .min_w_0()
            .p_4()
            .flex()
            .flex_col()
            .gap_2();
        if let Some(id) = selected {
            let name = match &conversation.channels {
                Some(Load::Ready(channels)) => channels
                    .iter()
                    .find(|channel| channel.id == id)
                    .map(|channel| channel.name.as_str())
                    .unwrap_or("Channel"),
                _ => "Channel",
            };
            history = history.child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::BOLD)
                    .child(format!("# {name}")),
            );
            let refreshing = matches!(
                conversation.refreshing.get(id),
                Some(conversation::Refresh::Running)
            );
            let refresh_view = view.clone();
            history = history.child(
                Button::new("refresh-history")
                    .label(if refreshing {
                        "Refreshing conversation…"
                    } else {
                        "Refresh conversation"
                    })
                    .disabled(
                        refreshing || matches!(conversation.history.get(id), Some(Load::Loading)),
                    )
                    .on_click(move |_, _, cx| {
                        let _ = refresh_view.update(cx, |view, cx| view.refresh_history(cx));
                    }),
            );
            if let Some(conversation::Refresh::Incomplete(error)) = conversation.refreshing.get(id)
            {
                history = history.child(
                    div()
                        .id("catchup-incomplete")
                        .aria_label(error.clone())
                        .test_support()
                        .child(format!("Catch-up incomplete: {error}")),
                );
            }
            match conversation.history.get(id) {
                None | Some(Load::Loading) => history = history.child("Loading conversation…"),
                Some(Load::Failed(error)) => {
                    history = history.child(format!("Conversation: {error}"))
                }
                Some(Load::Ready(messages)) if messages.is_empty() => {
                    history = history.child("No messages in this channel yet.")
                }
                Some(Load::Ready(messages)) => {
                    match conversation.older.get(id) {
                        Some(Older::Loading) => history = history.child("Loading older messages…"),
                        Some(Older::Failed(error)) => {
                            let target = view.clone();
                            history = history.child(
                                div().child(format!("Older messages: {error}")).child(
                                    Button::new("retry-older")
                                        .label("Retry older messages")
                                        .on_click(move |_, _, cx| {
                                            let _ =
                                                target.update(cx, |view, cx| view.retry_older(cx));
                                        }),
                                ),
                            );
                        }
                        Some(Older::Exhausted) => history = history.child("Start of conversation."),
                        _ => {}
                    }
                    let jump = view.clone();
                    history = history.child(
                        Button::new("jump-latest")
                            .label("Jump to latest")
                            .disabled(self.history_list.is_scrolled_to_end() != Some(false))
                            .on_click(move |_, _, cx| {
                                let _ = jump.update(cx, |view, cx| view.jump_latest(cx));
                            }),
                    );
                    // Server order is newest-first. Reverse for a chronological scroll surface;
                    // ListState::splice preserves the reader's anchor across variable heights.
                    let rows = messages.iter().rev().cloned().collect::<Vec<_>>();
                    history = history.child(
                        div().id("history").test_support().flex_1().min_h_0().child(
                            list(self.history_list.clone(), move |ix, _, _| {
                                let message = &rows[ix];
                                div()
                                    .id(format!("message-{}", message.id))
                                    .aria_label(message.text.clone())
                                    .test_support()
                                    .w_full()
                                    .p_2()
                                    .flex()
                                    .flex_col()
                                    .child(div().text_color(rgb(theme::MUTED)).child(format!(
                                        "{} · {}",
                                        message.author_name, message.created_at
                                    )))
                                    .child(
                                        div()
                                            .id(format!("message-text-{}", message.id))
                                            .test_support()
                                            .child(SelectableText::new(
                                                format!("text-{}", message.id),
                                                message.text.clone(),
                                            )),
                                    )
                                    .into_any_element()
                            })
                            .size_full(),
                        ),
                    );
                }
            }
            let sending = conversation.send_pending.contains_key(id);
            let composer_focus = self.composer.read(cx).focus_handle(cx);
            let send_view = view.clone();
            history = history.child(
                div()
                    .id("composer-panel")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .id("composer")
                            .test_support()
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                window.focus(&composer_focus, cx);
                                cx.stop_propagation();
                            })
                            .child(
                                Textarea::new(&self.composer)
                                    .aria_label(format!("Message to {name}"))
                                    .readonly(sending)
                                    .h(px(90.)),
                            ),
                    )
                    .child(
                        Button::new("send-message")
                            .icon(theme::send_icon())
                            .tooltip("Send message (Enter); Shift+Enter adds a line")
                            .label(if sending {
                                "Sending…"
                            } else {
                                "Send message"
                            })
                            .disabled(sending)
                            .on_click(move |_, _, cx| {
                                let _ = send_view.update(cx, |view, cx| view.send_message(cx));
                            }),
                    )
                    .when_some(conversation.send_feedback.get(id), |pane, feedback| {
                        pane.child(
                            div()
                                .id("send-feedback")
                                .aria_label(feedback.clone())
                                .test_support()
                                .child(feedback.clone()),
                        )
                    }),
            );
        } else if !matches!(conversation.channels, Some(Load::Ready(ref items)) if items.is_empty())
        {
            history = history.child("Select a text channel to read its conversation.");
        }
        history
    }
}
