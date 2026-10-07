//! Switches between login and workspace, displays session/storage feedback and delivers updates.
//! Session and workspace coordinators interpret those updates and dispatch requests.
use super::{login::LoginView, session_footer::SessionFooterView, workspace::WorkspaceView};
use crate::api::HttpTransport;
use crate::runtime::Execution;
use crate::session::{Lifecycle, SessionCoordinator, StorageRetry};
use crate::storage::{Config, Persistence};
use gpui_kit::component::{ActiveTheme, Root, button::Button};
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
        let session =
            cx.new(|_| SessionCoordinator::new(api, execution.clone(), config, persistence));
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
            execution,
            session,
            login,
            workspace: None,
            _session_subscription: session_subscription,
        }
    }
    fn enter_authenticated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(activity) = self.session.read(cx).workspace() else {
            return;
        };
        let updates = activity.updates();
        activity.start();
        let footer = cx.new(|cx| SessionFooterView::new(self.session.clone(), cx));
        self.workspace = Some(cx.new(|cx| {
            WorkspaceView::new(activity.clone(), self.execution.clone(), window, cx)
                .with_footer(footer)
        }));
        // Opaque delivery only. Views subscribe separately to feature invalidations;
        // recreating a workspace never creates another competing delivery loop.
        cx.spawn(async move |weak, cx| {
            while let Ok(update) = updates.recv().await {
                if weak
                    .update_in(cx, |view, window, cx| {
                        if let Some(end) = activity.apply(update) {
                            view.session.update(cx, |session, cx| {
                                session.workspace_ended(end);
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialogs = Root::render_dialog_layer(window, cx);
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
        // Kit's dialog layer has an in-flow wrapper. Keep it out of the flex
        // column so opening a modal cannot add a gap or shrink the workspace.
        surface.children(dialogs.map(|layer| div().absolute().inset_0().child(layer)))
    }
}
