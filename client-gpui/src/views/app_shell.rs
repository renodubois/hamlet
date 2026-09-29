//! Temporary authenticated presentation; child controls remain here until #58.
//! Session and conversation owners perform all request, completion and polling policy.
use super::login::LoginView;
use crate::api::HttpTransport;
use crate::conversation::{self, ConversationHandle, Load, Older};
use crate::runtime::Execution;
use crate::session::{Lifecycle, SessionCoordinator, StorageRetry};
use crate::storage::{Config, Persistence};
use crate::theme;
use gpui_kit::base::Disableable;
use gpui_kit::base::SelectableText;
use gpui_kit::base::input::{
    InputBaseState, InputEvent, InputMode, InputState, TextareaMode, TextareaState,
};
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub(crate) fn open(
    window: &mut Window,
    cx: &mut App,
    api: HttpTransport,
    config: Config,
    persistence: Option<Persistence>,
    execution: Execution,
) -> Entity<Hamlet> {
    cx.new(|cx| Hamlet::with_dependencies(window, cx, api, config, persistence, execution))
}
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

pub(crate) struct Hamlet {
    session: Entity<SessionCoordinator>,
    login: Entity<LoginView>,
    _session_subscription: Subscription,
    conversation: Option<ConversationHandle>,
    history_focus: FocusHandle,
    _window_activation: Subscription,
    history_list: ListState,
    presented_channel: Option<String>,
    presented_ids: Vec<String>,
    created_revision: u64,
    channel_name: Entity<InputBaseState<InputMode>>,
    composer: Entity<InputBaseState<TextareaMode>>,
    _composer_subscription: Subscription,
    syncing_composer: bool,
}
impl Hamlet {
    fn with_dependencies(
        window: &mut Window,
        cx: &mut Context<Self>,
        api: HttpTransport,
        config: Config,
        persistence: Option<Persistence>,
        execution: Execution,
    ) -> Self {
        let session = cx.new(|_| SessionCoordinator::new(api, execution, config, persistence));
        let login = cx.new(|cx| LoginView::new(session.clone(), window, cx));
        let session_subscription =
            cx.observe_in(&session, window, |view: &mut Self, _, window, cx| {
                view.session_changed(window, cx);
                cx.notify();
            });
        let channel_name = cx.new(|cx| InputState::new(window, cx));
        let composer = cx.new(|cx| TextareaState::new(window, cx).submit_on_enter(true));
        let composer_subscription = cx.subscribe(&composer, |view: &mut Self, field, event, cx| {
            if view.syncing_composer {
                return;
            }
            let Some(activity) = &view.conversation else {
                return;
            };
            match event {
                InputEvent::Change => {
                    activity.edit_draft(field.read(cx).text().to_string());
                    cx.notify();
                }
                InputEvent::PressEnter {
                    shift: false,
                    secondary: false,
                } => {
                    activity.edit_draft(field.read(cx).text().to_string());
                    activity.send();
                    cx.notify();
                }
                _ => {}
            }
        });
        let window_activation = cx.observe_window_activation(window, |view, window, cx| {
            if let Some(activity) = &view.conversation {
                activity.set_focused(window.is_window_active());
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
        let updates = session.read(cx).updates();
        cx.spawn(async move |weak, cx| {
            while let Ok(update) = updates.recv().await {
                if weak
                    .update_in(cx, |view, window, cx| {
                        view.session.update(cx, |session, cx| {
                            session.apply(update);
                            cx.notify();
                        });
                        view.session_changed(window, cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            session,
            login,
            _session_subscription: session_subscription,
            conversation: None,
            history_focus: cx.focus_handle(),
            _window_activation: window_activation,
            history_list,
            presented_channel: None,
            presented_ids: Vec::new(),
            created_revision: 0,
            channel_name,
            composer,
            _composer_subscription: composer_subscription,
            syncing_composer: false,
        }
    }
    fn store_composer(&mut self, cx: &mut Context<Self>) {
        if let Some(activity) = &self.conversation {
            activity.edit_draft(self.composer.read(cx).text().to_string());
        }
    }
    fn sync_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self
            .conversation
            .as_ref()
            .map(|activity| {
                let state = activity.read();
                state
                    .selected
                    .as_deref()
                    .map(|id| state.draft(id))
                    .unwrap_or("")
                    .to_owned()
            })
            .unwrap_or_default();
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
        if let Some(activity) = &self.conversation {
            activity.send();
        }
        cx.notify();
    }
    fn enter_authenticated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(activity) = self.session.read(cx).conversation() else {
            return;
        };
        let updates = activity.updates();
        self.conversation = Some(activity.clone());
        self.created_revision = 0;
        activity.start(window.is_window_active());
        // Opaque delivery only. No endpoint, request identity, result or scheduling policy.
        cx.spawn(async move |weak, cx| {
            while let Ok(update) = updates.recv().await {
                if weak
                    .update_in(cx, |view, window, cx| {
                        if let Some(end) = activity.apply(update) {
                            view.session.update(cx, |session, cx| {
                                session.conversation_ended(end);
                                cx.notify();
                            });
                        }
                        view.session_changed(window, cx);
                        view.present_conversation(window, cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        self.present_conversation(window, cx);
    }
    /// Presentation-only reconciliation. Feature state is authoritative; these IDs index rows.
    fn present_conversation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (selected, ids, created) = self
            .conversation
            .as_ref()
            .map(|activity| {
                let state = activity.read();
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
                (state.selected.clone(), ids, activity.created())
            })
            .unwrap_or_default();
        let follow = self.history_list.is_scrolled_to_end() != Some(false);
        if selected != self.presented_channel {
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
        if let Some((revision, name)) = created
            && revision != self.created_revision
        {
            self.created_revision = revision;
            if self.channel_name.read(cx).text().to_string().trim() == name {
                self.channel_name
                    .update(cx, |input, cx| input.set_value("", window, cx));
            }
        }
        self.sync_composer(window, cx);
    }
    fn refresh_channels(&mut self, cx: &mut Context<Self>) {
        if let Some(activity) = &self.conversation {
            activity.refresh_channels();
        }
        cx.notify();
    }
    fn request_older(&mut self, cx: &mut Context<Self>) {
        if let Some(activity) = &self.conversation {
            activity.request_older();
        }
        cx.notify();
    }
    fn refresh_history(&mut self, cx: &mut Context<Self>) {
        if let Some(activity) = &self.conversation {
            activity.refresh_history();
        }
        cx.notify();
    }
    fn retry_older(&mut self, cx: &mut Context<Self>) {
        if let Some(activity) = &self.conversation {
            activity.retry_older();
        }
        cx.notify();
    }
    fn jump_latest(&mut self, cx: &mut Context<Self>) {
        self.history_list.scroll_to_end();
        cx.notify();
    }
    fn create_channel(&mut self, cx: &mut Context<Self>) {
        if let Some(activity) = &self.conversation {
            activity.create_channel(&self.channel_name.read(cx).text().to_string());
        }
        cx.notify();
    }
    fn select_channel(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.store_composer(cx);
        if let Some(activity) = &self.conversation {
            activity.select_channel(id);
        }
        self.present_conversation(window, cx);
        cx.notify();
    }
    fn logout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| {
            session.logout();
            cx.notify();
        });
        self.session_changed(window, cx);
        cx.notify();
    }
    fn session_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lifecycle = self
            .session
            .update(cx, |session, _| session.take_lifecycle());
        if lifecycle.is_some() {
            self.login
                .update(cx, |login, cx| login.clear_sensitive(window, cx));
        }
        match lifecycle {
            Some(Lifecycle::Authenticated) => self.enter_authenticated(window, cx),
            Some(Lifecycle::Invalidated | Lifecycle::ServerChanged) => {
                // Session already closed dispatch, cancelled work and wiped all feature state.
                self.conversation = None;
                self.present_conversation(window, cx);
            }
            None => {}
        }
    }
}

impl Render for Hamlet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let mut surface = div()
            .size_full()
            .bg(rgb(theme::BACKGROUND))
            .flex()
            .flex_col()
            .gap_3()
            .text_color(rgb(theme::TEXT));
        if let Some(session) = self.session.read(cx).active() {
            let logout_view = view.clone();
            let label = format!(
                "Logged in as {} at {}",
                session.user.username, session.server
            );
            surface = surface
                .child(
                    div()
                        .id("session-status")
                        .aria_label(label.clone())
                        .test_support()
                        .child(label),
                )
                .child(
                    Button::new("logout")
                        .label("Log out")
                        .on_click(move |_, window, cx| {
                            let _ = logout_view.update(cx, |view, cx| view.logout(window, cx));
                        }),
                )
                .child(
                    div()
                        .id("connection-status")
                        .aria_label(self.conversation.as_ref().unwrap().status())
                        .test_support()
                        .child(self.conversation.as_ref().unwrap().status()),
                )
                .child(self.conversation_panes(cx));
        } else {
            surface = surface
                .justify_center()
                .items_center()
                .child(self.login.clone());
        }
        if let Some(action) = self.session.read(cx).storage_retry() {
            let retry = view.clone();
            surface = surface.child(
                Button::new("retry-storage")
                    .label(match action {
                        StorageRetry::Deletion => "Retry saved-login deletion",
                        StorageRetry::Restoration => "Retry saved-login restoration",
                    })
                    .on_click(move |_, _, cx| {
                        let _ = retry.update(cx, |view, cx| {
                            view.session.update(cx, |session, cx| {
                                session.retry_storage();
                                cx.notify();
                            });
                            cx.notify();
                        });
                    }),
            );
        }
        if let Some(feedback) = self.session.read(cx).storage_feedback() {
            surface = surface.child(
                div()
                    .id("storage-status")
                    .aria_label(feedback.to_owned())
                    .test_support()
                    .child(feedback.to_owned()),
            );
        }
        if self.session.read(cx).active().is_some()
            && let Some(feedback) = self.session.read(cx).feedback()
        {
            surface = surface.child(
                div()
                    .id("auth-feedback")
                    .aria_label(feedback.to_owned())
                    .test_support()
                    .child(feedback.to_owned()),
            );
        }
        surface
    }
}

impl Hamlet {
    fn conversation_panes(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let conversation = self.conversation.as_ref().unwrap().read();
        let view = cx.entity().downgrade();
        let create_view = view.clone();
        let refresh_channels_view = view.clone();
        let mut sidebar = div()
            .id("channels")
            .test_support()
            .w(px(220.))
            .h_full()
            .bg(rgb(theme::SIDEBAR))
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .overflow_y_scroll()
            .child(div().font_weight(FontWeight::BOLD).child("Text channels"))
            .child(
                Button::new("refresh-channels")
                    .label(
                        if conversation.channels.is_some() && conversation.channel_refreshing() {
                            "Refreshing channels…"
                        } else {
                            "Refresh channels"
                        },
                    )
                    .disabled(conversation.channel_refreshing())
                    .on_click(move |_, _, cx| {
                        let _ =
                            refresh_channels_view.update(cx, |view, cx| view.refresh_channels(cx));
                    }),
            )
            .child(
                div().child("Channel name").child(
                    Input::new(&self.channel_name)
                        .id("channel-name")
                        .aria_label("Channel name"),
                ),
            )
            .child(
                Button::new("create-channel")
                    .disabled(
                        conversation.create_pending
                            || !matches!(conversation.channels, Some(Load::Ready(_))),
                    )
                    .label(if conversation.create_pending {
                        "Creating channel…"
                    } else {
                        "Create text channel"
                    })
                    .on_click(move |_, _, cx| {
                        let _ = create_view.update(cx, |view, cx| view.create_channel(cx));
                    }),
            );
        if let Some(feedback) = &conversation.create_feedback {
            sidebar = sidebar.child(
                div()
                    .id("channel-feedback")
                    .aria_label(feedback.clone())
                    .test_support()
                    .child(feedback.clone()),
            );
        }
        if let Some(error) = &conversation.channel_error {
            sidebar = sidebar.child(
                div()
                    .id("channels-refresh-error")
                    .aria_label(error.clone())
                    .test_support()
                    .child(format!("Channel refresh: {error}")),
            );
        }
        match &conversation.channels {
            None | Some(Load::Loading) => sidebar = sidebar.child("Loading channels…"),
            Some(Load::Failed(error)) => sidebar = sidebar.child(format!("Channels: {error}")),
            Some(Load::Ready(channels)) if channels.is_empty() => {
                sidebar = sidebar.child("No text channels yet.")
            }
            Some(Load::Ready(channels)) => {
                for channel in channels {
                    let id = channel.id.clone();
                    let selected = conversation.selected.as_deref() == Some(&id);
                    let label = format!("# {}", channel.name);
                    let target = view.clone();
                    sidebar = sidebar.child(
                        Button::new(format!("channel-{id}"))
                            .label(label)
                            .icon(theme::channel_icon())
                            .on_click(move |_, window, cx| {
                                let _ = target
                                    .update(cx, |view, cx| view.select_channel(&id, window, cx));
                            })
                            .when(selected, |button| button.bg(rgb(theme::SELECTED))),
                    );
                }
            }
        }
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
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(sidebar)
            .child(history)
    }
}

// Descendant test modules retain legacy private access without exporting view internals.
#[cfg(test)]
#[path = "tests/legacy_fixture.rs"]
mod tests;
