//! Switches between login and chat, displays session/storage feedback and delivers updates.
//! Session and chat coordinators interpret those updates and dispatch requests.
use super::{chat::ChatView, login::LoginView};
use crate::api::HttpTransport;
use crate::runtime::Execution;
use crate::session::{Lifecycle, SessionCoordinator, StorageRetry};
use crate::storage::{Config, Persistence};
use gpui_kit::component::{ActiveTheme, button::Button};
use gpui_kit::*;

pub(crate) fn open(
    window: &mut Window,
    cx: &mut App,
    api: HttpTransport,
    config: Config,
    persistence: Option<Persistence>,
    execution: Execution,
) -> Entity<AppShell> {
    cx.new(|cx| AppShell::new(window, cx, api, config, persistence, execution))
}

pub(crate) struct AppShell {
    execution: Execution,
    session: Entity<SessionCoordinator>,
    login: Entity<LoginView>,
    chat: Option<Entity<ChatView>>,
    _session_subscription: Subscription,
}
impl AppShell {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        api: HttpTransport,
        config: Config,
        persistence: Option<Persistence>,
        execution: Execution,
    ) -> Self {
        let session =
            cx.new(|_| SessionCoordinator::new(api, execution.clone(), config, persistence));
        let login = cx.new(|cx| LoginView::new(session.clone(), window, cx));
        let session_subscription =
            cx.observe_in(&session, window, |view: &mut Self, _, window, cx| {
                view.session_changed(window, cx);
                cx.notify();
            });
        let updates = session.read(cx).updates();
        Self::deliver(
            cx,
            move || {
                let updates = updates.clone();
                async move { updates.recv().await.ok() }
            },
            |view, update, cx| {
                view.session.update(cx, |session, cx| {
                    session.apply(update);
                    cx.notify();
                });
            },
        );
        Self {
            execution,
            session,
            login,
            chat: None,
            _session_subscription: session_subscription,
        }
    }
    /// Applies each opaque executor delivery on the UI thread, then reconciles the screen,
    /// until the source closes or the shell is dropped. Coordinators interpret updates.
    fn deliver<T: 'static, F: Future<Output = Option<T>> + 'static>(
        cx: &mut Context<Self>,
        mut next: impl FnMut() -> F + 'static,
        mut apply: impl FnMut(&mut Self, T, &mut Context<Self>) + 'static,
    ) {
        cx.spawn(async move |weak, cx| {
            while let Some(update) = next().await {
                if weak
                    .update_in(cx, |view, window, cx| {
                        apply(view, update, cx);
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
    }
    fn enter_authenticated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(chat) = self.session.read(cx).chat() else {
            return;
        };
        let updates = chat.updates();
        chat.start();
        self.chat = Some(cx.new(|cx| {
            ChatView::new(
                chat.clone(),
                self.session.clone(),
                self.execution.clone(),
                window,
                cx,
            )
        }));
        // Opaque delivery only. Views subscribe separately to feature invalidations;
        // recreating a chat never creates another competing delivery loop.
        Self::deliver(
            cx,
            move || {
                let updates = updates.clone();
                async move { updates.recv().await.ok() }
            },
            move |view, update, cx| {
                if let Some(end) = chat.apply(update) {
                    view.session.update(cx, |session, cx| {
                        session.chat_ended(end);
                        cx.notify();
                    });
                }
            },
        );
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
                // The session already closed protected dispatch and wiped authoritative state.
                self.chat = None;
            }
            None => {}
        }
    }
}
impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let mut surface = div()
            .relative()
            .size_full()
            .bg(cx.theme().tokens.background.background)
            .flex()
            .flex_col()
            .gap_3()
            .text_color(cx.theme().foreground);
        if self.session.read(cx).active().is_some() {
            if let Some(chat) = &self.chat {
                surface = surface.child(chat.clone());
            }
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
        surface
    }
}
