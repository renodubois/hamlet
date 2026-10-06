//! Account presentation and logout intention; session policy remains in session/.
use crate::session::SessionCoordinator;
use gpui_kit::component::{ActiveTheme, button::Button};
use gpui_kit::*;

pub(crate) struct SessionFooterView {
    session: Entity<SessionCoordinator>,
    _subscription: Subscription,
}

impl SessionFooterView {
    pub(crate) fn new(session: Entity<SessionCoordinator>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&session, |_, _, cx| cx.notify());
        Self {
            session,
            _subscription: subscription,
        }
    }
}

impl Render for SessionFooterView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut footer = div()
            .id("sidebar-footer")
            .test_support()
            .w_full()
            .flex_shrink_0()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().tokens.sidebar.background)
            .text_color(cx.theme().sidebar_foreground);
        if let Some(session) = self.session.read(cx).active() {
            let label = format!(
                "Logged in as {} at {}",
                session.user.username, session.server
            );
            footer = footer
                .child(
                    div()
                        .id("session-status")
                        .aria_label(label)
                        .test_support()
                        .overflow_hidden()
                        .child(session.user.username.clone()),
                )
                .child(Button::new("logout").label("Log out").on_click(cx.listener(
                    |view, _, _, cx| {
                        view.session.update(cx, |session, cx| {
                            session.logout();
                            cx.notify();
                        });
                    },
                )));
        }
        footer
    }
}
