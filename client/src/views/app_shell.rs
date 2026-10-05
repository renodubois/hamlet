//! Screen composition and opaque feature delivery. Children own authenticated presentation.
use super::{login::LoginView, workspace::WorkspaceView};
use crate::api::HttpTransport;
use crate::runtime::Execution;
use crate::session::{Lifecycle, SessionCoordinator, StorageRetry};
use crate::storage::{Config, Persistence};
use crate::theme;
use gpui_kit::component::button::Button;
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
    session: Entity<SessionCoordinator>,
    login: Entity<LoginView>,
    workspace: Option<Entity<WorkspaceView>>,
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
        let session = cx.new(|_| SessionCoordinator::new(api, execution, config, persistence));
        let login = cx.new(|cx| LoginView::new(session.clone(), window, cx));
        let session_subscription =
            cx.observe_in(&session, window, |view: &mut Self, _, window, cx| {
                view.session_changed(window, cx);
                cx.notify();
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
            workspace: None,
            _session_subscription: session_subscription,
        }
    }
    fn enter_authenticated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(activity) = self.session.read(cx).conversation() else {
            return;
        };
        let updates = activity.updates();
        activity.start();
        self.workspace = Some(cx.new(|cx| WorkspaceView::new(activity.clone(), window, cx)));
        // Opaque delivery only. Views subscribe separately to feature invalidations;
        // recreating a workspace never creates another competing delivery loop.
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
                // The session already closed protected dispatch and wiped authoritative state.
                self.workspace = None;
            }
            None => {}
        }
    }
}
impl Render for AppShell {
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
            surface =
                surface
                    .child(
                        div()
                            .id("session-status")
                            .aria_label(label.clone())
                            .test_support()
                            .child(label),
                    )
                    .child(Button::new("logout").label("Log out").on_click(
                        move |_, window, cx| {
                            let _ = logout_view.update(cx, |view, cx| view.logout(window, cx));
                        },
                    ));
            if let Some(workspace) = &self.workspace {
                surface = surface.child(workspace.clone());
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
