//! Conversation layout with an owned history child. Composer ownership remains for #60.
pub(crate) mod message_history;
mod message_row;

use crate::conversation::{ConversationHandle, Load};
use crate::theme;
use gpui_kit::base::Disableable;
use gpui_kit::base::input::{InputBaseState, InputEvent, TextareaMode, TextareaState};
use gpui_kit::component::{button::Button, input::Textarea};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use message_history::MessageHistoryView;

pub(crate) struct ConversationView {
    conversation: ConversationHandle,
    history: Entity<MessageHistoryView>,
    presented_channel: Option<String>,
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
        let history = cx.new(|cx| MessageHistoryView::new(conversation.clone(), window, cx));
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
            history,
            presented_channel: None,
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
    fn present_conversation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.conversation.read().selected.clone();
        let replaced = selected != self.presented_channel;
        if replaced {
            self.store_composer(cx);
        }
        self.presented_channel = selected;
        self.sync_composer(replaced, window, cx);
    }
}

impl Render for ConversationView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.present_conversation(window, cx);
        let conversation = self.conversation.read();
        let view = cx.entity().downgrade();
        let mut pane = div()
            .flex_1()
            .h_full()
            .min_w_0()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.history.clone());
        if let Some(id) = conversation.selected.as_deref() {
            let name = match &conversation.channels {
                Some(Load::Ready(channels)) => channels
                    .iter()
                    .find(|channel| channel.id == id)
                    .map(|channel| channel.name.as_str())
                    .unwrap_or("Channel"),
                _ => "Channel",
            };
            let sending = conversation.send_pending.contains_key(id);
            let composer_focus = self.composer.read(cx).focus_handle(cx);
            let send_view = view.clone();
            pane = pane.child(
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
        }
        pane
    }
}
