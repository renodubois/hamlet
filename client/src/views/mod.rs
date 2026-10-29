pub(crate) mod app_shell;
mod chat;
mod conversation;
pub(crate) mod login;
mod session_footer;
mod sidebar;

use crate::chat::ChatHandle;
use gpui_kit::{Context, Task, Window};

/// Re-renders the view on each coalesced chat invalidation while it lives. Notifications
/// carry no state; views hydrate from `ChatHandle::read`. Dropping the task unsubscribes.
fn rerender_on_chat_change<V: 'static>(chat: &ChatHandle, cx: &mut Context<V>) -> Task<()> {
    let changes = chat.notifications();
    cx.spawn(async move |weak, cx| {
        while changes.recv().await.is_ok() {
            if weak.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        }
    })
}

/// Like `rerender_on_chat_change`, first letting the view reconcile local presentation
/// (inputs, dialogs, focus) with the new chat state.
fn present_on_chat_change<V: 'static>(
    chat: &ChatHandle,
    cx: &mut Context<V>,
    mut present: impl FnMut(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> Task<()> {
    let changes = chat.notifications();
    cx.spawn(async move |weak, cx| {
        while changes.recv().await.is_ok() {
            if weak
                .update_in(cx, |view, window, cx| {
                    present(view, window, cx);
                    cx.notify();
                })
                .is_err()
            {
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tests/support/menu_text.rs"]
mod menu_text_probe;

#[cfg(test)]
#[path = "tests/login.rs"]
mod login_tests;
