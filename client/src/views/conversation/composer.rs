//! Textarea interaction and displayed-draft projection. Chat owns all drafts and sends.
use crate::chat::ChatHandle;
use gpui_kit::base::input::{InputBaseState, InputEvent, TextareaMode, TextareaState};
use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub(super) struct ComposerView {
    chat: ChatHandle,
    presented_channel: Option<String>,
    // Last hydrated/forwarded text, not an authoritative draft. Protects queued Kit edits.
    presented_draft: String,
    input: Entity<InputBaseState<TextareaMode>>,
    _input_subscription: Subscription,
    _notifications: Task<()>,
    syncing: bool,
}
impl ComposerView {
    pub(super) fn new(chat: ChatHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .submit_on_enter(true)
                .placeholder("Message")
        });
        let input_subscription = cx.subscribe(&input, |view: &mut Self, _, event, cx| {
            if view.syncing {
                return;
            }
            match event {
                InputEvent::Change => view.store_input(cx),
                InputEvent::PressEnter {
                    shift: false,
                    secondary: false,
                } => view.send_message(cx),
                _ => {}
            }
            cx.notify();
        });
        let changes = chat.notifications();
        let notifications = cx.spawn(async move |weak, cx| {
            while changes.recv().await.is_ok() {
                if weak
                    .update_in(cx, |view, window, cx| {
                        view.present(window, cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut view = Self {
            chat,
            presented_channel: None,
            presented_draft: String::new(),
            input,
            _input_subscription: input_subscription,
            _notifications: notifications,
            syncing: false,
        };
        view.present(window, cx);
        view
    }
    fn store_input(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text().to_string();
        if text != self.presented_draft
            && let Some(channel) = &self.presented_channel
        {
            let is_selected = self.chat.read().selected.as_deref() == Some(channel);
            if is_selected {
                self.chat.edit_draft(text.clone());
            } else {
                self.chat.edit_channel_draft(channel, text.clone());
            }
            self.presented_draft = text;
        }
    }
    fn sync_input(&mut self, replaced: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = {
            let state = self.chat.read();
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
        if self.input.read(cx).text() == text.as_str() {
            return;
        }
        self.syncing = true;
        self.input
            .update(cx, |input, cx| input.set_value(&text, window, cx));
        self.syncing = false;
    }
    fn send_message(&mut self, cx: &mut Context<Self>) {
        self.store_input(cx);
        // Navigation may precede delivery of the old textarea's Enter.
        // Never interpret that event as permission to send the newly selected draft.
        if self.presented_channel.is_some() && self.presented_channel == self.chat.read().selected {
            self.chat.send();
        }
        cx.notify();
    }
    fn present(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.chat.read().selected.clone();
        let replaced = selected != self.presented_channel;
        if replaced {
            self.store_input(cx);
        }
        self.presented_channel = selected;
        self.sync_input(replaced, window, cx);

        let placeholder = self
            .chat
            .read()
            .selected_channel()
            .map(|channel| format!("Message #{}", channel.name))
            .unwrap_or_else(|| "Message".to_owned());
        if self.input.read(cx).presentation().placeholder().as_str() != placeholder.as_str() {
            self.input.update(cx, |input, cx| {
                input.set_placeholder(placeholder, window, cx);
            });
        }
    }
}

impl Render for ComposerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.present(window, cx);
        let chat = self.chat.read();
        let Some(id) = chat.selected.as_deref() else {
            return div().into_any_element();
        };
        let name = chat
            .selected_channel()
            .map(|channel| channel.name.as_str())
            .unwrap_or("Channel");
        let sending = chat.send_pending.contains_key(id);
        let focus = self.input.read(cx).focus_handle(cx);
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
                        window.focus(&focus, cx);
                        cx.stop_propagation();
                    })
                    .child(
                        Textarea::new(&self.input)
                            .aria_label(format!("Message to {name}"))
                            .readonly(sending)
                            .h(px(90.)),
                    ),
            )
            .when_some(chat.send_feedback.get(id), |pane, feedback| {
                pane.child(
                    div()
                        .id("send-feedback")
                        .aria_label(feedback.clone())
                        .test_support()
                        .child(feedback.clone()),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "tests/composer_lifecycle.rs"]
mod composer_lifecycle_tests;
