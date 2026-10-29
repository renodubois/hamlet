use super::*;
use gpui_kit::component::ActiveTheme as _;

type Reply = async_channel::Sender<Result<Response, ApiError>>;
fn login(cx: &mut gpui_kit::VisualTestContext) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
}
fn open_dialog(cx: &mut gpui_kit::VisualTestContext, message: &str) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.right_click(format!("message-{message}"), cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("delete-message-label", cx);
    });
    cx.run_until_parked();
}
struct MessageAuth(Arc<Mutex<Vec<(Request, Reply)>>>);
impl RequestAdapter for MessageAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.method() == reqwest::Method::DELETE {
            let (reply, receive) = async_channel::bounded(1);
            self.0.lock().unwrap().push((request, reply));
            return Box::pin(async move { receive.recv().await.unwrap() });
        }
        if request.url().path().ends_with("/messages") {
            return Box::pin(async {
                Ok(Response::controlled(StatusCode::OK, serde_json::json!({"items":[
                {"id":"000000000000008","channel_id":"000000000000001","author":{"id":"42","display_name":"renamed author"},"text":"é".repeat(4000),"created_at":"2026-01-01T00:00:00Z"},
                {"id":"000000000000009","channel_id":"000000000000001","author":{"id":"other","display_name":"Ada"},"text":"not mine","created_at":"2026-01-01T00:00:00Z"}
            ],"next_cursor":null}).to_string()))
            });
        }
        BoundAuth.execute(request)
    }
}

#[gpui_kit::test]
fn message_menu_exposes_named_actionable_delete_with_actual_red_text(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        Root::new(
            open_controlled(window, cx, Arc::new(MessageAuth(calls.clone()))),
            window,
            cx,
        )
    });
    login(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.right_click("message-000000000000008", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let painted = cx.global::<crate::views::menu_text_probe::PaintedMenuText>();
        assert_eq!(painted.text, "Delete Message");
        assert_eq!(painted.color, cx.theme().danger);
        let red: gpui_kit::Rgba = painted.color.into();
        assert!(
            red.r > red.g && red.r > red.b,
            "actual glyph color must be red"
        );
        let action = window.find("delete-message-label");
        assert_eq!(action.role(), Some(gpui_kit::Role::MenuItem));
        assert_eq!(action.label(), Some("Delete Message"));
        assert_eq!(
            window.find("message-context-menu").role(),
            Some(gpui_kit::Role::Menu)
        );
    });
    assert!(calls.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn message_menu_is_pointer_anchored_and_dismissal_restores_interrupted_focus(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        Root::new(
            open_controlled(window, cx, Arc::new(MessageAuth(calls.clone()))),
            window,
            cx,
        )
    });
    login(cx);
    for dismiss in ["escape", "outside"] {
        let (pointer, interrupted_focus) = cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            let focus = window.focused(cx).expect("composer takes focus");
            let pointer = window.find("message-000000000000008").bounds().center();
            window.right_click("message-000000000000008", cx);
            (pointer, focus)
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            let menu = window.find("message-context-menu");
            assert_eq!(
                menu.bounds().origin,
                pointer,
                "menu follows the window-local pointer"
            );
            assert_eq!(menu.focused(), Some(true));
            window.press("down", cx);
            window.render_frame(cx);
            let action = window.find("delete-message-label");
            assert_eq!(action.focused(), Some(true));
            assert_eq!(action.role(), Some(gpui_kit::Role::MenuItem));
            if dismiss == "escape" {
                window.press("escape", cx);
            } else {
                window.click("history-pane", cx);
            }
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("message-context-menu").is_none());
            assert!(window.try_find("delete-message-label").is_none());
            assert!(window.try_find("delete-message-preview").is_none());
            assert!(interrupted_focus.is_focused(window));
            assert!(!gpui_kit::base::GlobalState::is_in_deferred_context(cx));
        });
    }
    assert!(calls.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn real_message_menu_and_dialog_capture_author_identity_preview_cancel_focus_and_pending(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        Root::new(
            open_controlled(window, cx, Arc::new(MessageAuth(calls.clone()))),
            window,
            cx,
        )
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.right_click("message-000000000000009", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("delete-message-label").is_none());
        window.press("escape", cx);
        window.right_click("message-000000000000008", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("delete-message-label").label(),
            Some("Delete Message")
        );
        window.press("down", cx);
        window.press("enter", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("delete-message-warning").label(),
            Some("You can't restore this message in the app.")
        );
        assert_eq!(
            window
                .find("delete-message-preview")
                .label()
                .unwrap()
                .chars()
                .count(),
            241
        );
        assert_eq!(window.find("cancel-message-action").focused(), Some(true));
        assert_eq!(
            window.find("confirm-delete-message").label(),
            Some("Delete")
        );
        window.press("enter", cx);
    });
    cx.run_until_parked();
    assert!(calls.lock().unwrap().is_empty());
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("delete-message-preview").is_none());
        window.right_click("message-000000000000008", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("delete-message-label", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("confirm-delete-message", cx);
        window.press("escape", cx);
        window.render_frame(cx);
        window.click("confirm-delete-message", cx);
        window.click("cancel-message-action", cx);
        window.press("escape", cx);
    });
    cx.run_until_parked();
    assert_eq!(calls.lock().unwrap().len(), 1);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("delete-message-preview").is_some());
        assert!(window.try_find("message-000000000000008").is_some());
    });
    calls.lock().unwrap()[0]
        .1
        .try_send(Ok(Response::controlled(StatusCode::NO_CONTENT, "")))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("delete-message-preview").is_none());
        assert!(window.try_find("message-000000000000008").is_none());
        assert!(window.try_find("message-000000000000009").is_some());
    });
}

#[gpui_kit::test]
fn message_dialog_feedback_requires_deliberate_retry_and_event_wins_over_late_failure(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let streams = crate::test_support::live::Streams::default();
    let (_, cx) = cx.add_window_view(|window, cx| {
        Root::new(
            open_with_streams(window, cx, Arc::new(MessageAuth(calls.clone())), &streams),
            window,
            cx,
        )
    });
    login(cx);
    open_dialog(cx, "000000000000008");
    for (index, response, feedback) in [
        (
            0,
            Ok(Response::controlled(
                StatusCode::FORBIDDEN,
                r#"{"error":{"code":"forbidden"}}"#,
            )),
            "own messages",
        ),
        (1, Err(ApiError::Unavailable), "may have succeeded"),
    ] {
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("confirm-delete-message", cx);
        });
        cx.run_until_parked();
        calls.lock().unwrap()[index].1.try_send(response).unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find("delete-message-feedback")
                    .label()
                    .unwrap()
                    .contains(feedback)
            );
            assert!(window.try_find("message-000000000000008").is_some());
            assert_ne!(window.find("confirm-delete-message").disabled(), Some(true));
        });
        assert_eq!(
            calls.lock().unwrap().len(),
            index + 1,
            "feedback does not automatically retry"
        );
    }
    cx.update(|window, cx| {
        window.click("confirm-delete-message", cx);
    });
    cx.run_until_parked();
    assert_eq!(calls.lock().unwrap().len(), 3);
    streams.change(serde_json::json!({"type":"message_deleted","channel_id":"000000000000001","message_id":"000000000000008"}));
    cx.run_until_parked();
    calls.lock().unwrap()[2]
        .1
        .try_send(Err(ApiError::Unavailable))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("delete-message-preview").is_none());
        assert!(window.try_find("delete-message-feedback").is_none());
        assert!(window.try_find("message-000000000000008").is_none());
    });
}

#[gpui_kit::test]
fn authored_trash_tray_hover_is_reachable_pane_local_and_does_not_shift_layout(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        Root::new(
            open_controlled(window, cx, Arc::new(MessageAuth(calls.clone()))),
            window,
            cx,
        )
    });
    login(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.hover("composer", cx);
        let row = window.find("message-000000000000008").bounds();
        let pane = window.find("history").bounds();
        let composer = window.find("composer-panel").bounds();
        assert!(!window.find("trash-message-000000000000008").visible());
        window.hover("message-000000000000009", cx);
        assert!(window.try_find("trash-message-000000000000009").is_none());
        window.hover("message-000000000000008", cx);
        let trash = window.find("trash-message-000000000000008");
        assert!(trash.visible());
        assert_eq!(trash.label(), Some("Delete Message"));
        assert_eq!(trash.role(), Some(gpui_kit::Role::Button));
        assert!(pane.contains(&trash.bounds().origin));
        assert!(pane.contains(&trash.bounds().bottom_right()));
        assert_eq!(trash.bounds().right(), row.right() - px(8.));
        assert_eq!(trash.bounds().origin.y, row.origin.y);
        window.hover("trash-message-000000000000008", cx);
        assert!(window.find("trash-message-000000000000008").visible());
        assert_eq!(window.find("message-000000000000008").bounds(), row);
        assert_eq!(window.find("history").bounds(), pane);
        assert_eq!(window.find("composer-panel").bounds(), composer);
        window.hover("composer", cx);
        assert!(!window.find("trash-message-000000000000008").visible());
        assert_eq!(window.find("message-000000000000008").bounds(), row);
        assert_eq!(window.find("history").bounds(), pane);
        window.hover("message-000000000000008", cx);
        window.click("trash-message-000000000000008", cx);
    });
    cx.run_until_parked();
    assert!(calls.lock().unwrap().is_empty());
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("delete-message-preview").is_some());
        assert_eq!(window.find("cancel-message-action").focused(), Some(true));
        window.click("cancel-message-action", cx);
    });
    cx.run_until_parked();
    assert!(calls.lock().unwrap().is_empty());
}

struct TallMessageAuth(Arc<Mutex<Vec<(Request, Reply)>>>);
impl RequestAdapter for TallMessageAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.method() == reqwest::Method::GET && request.url().path().ends_with("/messages") {
            return Box::pin(async {
                Ok(Response::controlled(
                    StatusCode::OK,
                    serde_json::json!({"items":[
                        {"id":"000000000000008","channel_id":"000000000000001",
                         "author":{"id":"42","display_name":"Ada"},
                         "text":"tall authored message\n".repeat(100),"created_at":"2026-01-01T00:00:00Z"},
                        {"id":"000000000000009","channel_id":"000000000000001",
                         "author":{"id":"other","display_name":"Other"},
                         "text":"latest message","created_at":"2026-01-01T00:00:00Z"}
                    ],"next_cursor":null}).to_string(),
                ))
            });
        }
        MessageAuth(self.0.clone()).execute(request)
    }
}

#[gpui_kit::test]
fn partially_clipped_authored_tray_remains_reachable_without_moving_history(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        Root::new(
            open_controlled(window, cx, Arc::new(TallMessageAuth(calls.clone()))),
            window,
            cx,
        )
    });
    login(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.scroll(
            "history",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(83.))),
            cx,
        );
        window.render_frame(cx);
        let row = window.find("message-000000000000008").bounds();
        let pane = window.find("history").bounds();
        let composer = window.find("composer-panel").bounds();
        assert!(
            row.top() < pane.top() && row.bottom() > pane.bottom(),
            "fixture straddles the pane"
        );
        // Hover the visible remainder, not the offscreen row center.
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: pane.center(),
                pressed_button: None,
                modifiers: gpui_kit::Modifiers::default(),
            }),
            cx,
        );
        window.render_frame(cx);
        let trash = window.find("trash-message-000000000000008");
        assert!(
            trash.visible(),
            "hovering the visible remainder must expose the actual tray"
        );
        assert!(pane.contains(&trash.bounds().origin));
        assert!(pane.contains(&trash.bounds().bottom_right()));
        assert_eq!(trash.bounds().top(), pane.top());
        assert_eq!(trash.bounds().right(), row.right() - px(8.));
        let destination = trash.bounds().center();
        for step in 1..=8 {
            let position = pane.center() + (destination - pane.center()) * (step as f32 / 8.);
            window.dispatch_event(
                gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                    position,
                    pressed_button: None,
                    modifiers: gpui_kit::Modifiers::default(),
                }),
                cx,
            );
            window.render_frame(cx);
            assert!(
                window.find("trash-message-000000000000008").visible(),
                "tray stays exposed throughout pointer traversal"
            );
        }
        window.hover("trash-message-000000000000008", cx);
        assert!(window.find("trash-message-000000000000008").visible());
        assert_eq!(window.find("message-000000000000008").bounds(), row);
        assert_eq!(window.find("history").bounds(), pane);
        assert_eq!(window.find("composer-panel").bounds(), composer);
        window.click("trash-message-000000000000008", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("delete-message-preview").is_some());
        window.click("cancel-message-action", cx);
    });
    cx.run_until_parked();
    assert!(calls.lock().unwrap().is_empty());
    for key in ["enter", "space"] {
        cx.update(|window, cx| {
            window.render_frame(cx);
            // Reposition with a real wheel event: the clamp must use this frame's
            // pane/row geometry, not the previously hovered position.
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-21.))),
                cx,
            );
            window.click("history-pane", cx);
            window.hover("composer", cx);
            let row = window.find("message-000000000000008").bounds();
            let pane = window.find("history").bounds();
            let composer = window.find("composer-panel").bounds();
            assert!(row.top() < pane.top());
            assert!(!window.find("trash-message-000000000000008").visible());
            for _ in 0..12 {
                window.press("tab", cx);
                window.render_frame(cx);
                if window.find("trash-message-000000000000008").focused() == Some(true) {
                    break;
                }
            }
            let trash = window.find("trash-message-000000000000008");
            assert_eq!(
                trash.focused(),
                Some(true),
                "partially clipped row keeps a reachable tab stop"
            );
            assert!(
                trash.visible(),
                "focus reveals the clamped tray without hover"
            );
            assert_eq!(trash.bounds().top(), pane.top());
            assert!(pane.contains(&trash.bounds().bottom_right()));
            assert_eq!(window.find("message-000000000000008").bounds(), row);
            assert_eq!(window.find("history").bounds(), pane);
            assert_eq!(window.find("composer-panel").bounds(), composer);
            window.press(key, cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find("delete-message-preview")
                    .label()
                    .unwrap()
                    .starts_with("tall authored message\n")
            );
            assert_eq!(window.find("cancel-message-action").focused(), Some(true));
            window.press("escape", cx);
        });
        cx.run_until_parked();
        assert!(calls.lock().unwrap().is_empty());
    }
}

struct SliverMessageAuth(Arc<Mutex<Vec<(Request, Reply)>>>);
impl RequestAdapter for SliverMessageAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.method() == reqwest::Method::GET && request.url().path().ends_with("/messages") {
            let items = (1..=40).rev().map(|id| serde_json::json!({
                "id":format!("{id:015}"), "channel_id":"000000000000001",
                "author":{"id":if id == 20 { "42" } else { "other" },"display_name":"Ada"},
                "text":if id == 20 { "sliver target\n".repeat(4) } else { format!("neighbor {id}") },
                "created_at":"2026-01-01T00:00:00Z"
            })).collect::<Vec<_>>();
            return Box::pin(async move {
                Ok(Response::controlled(
                    StatusCode::OK,
                    serde_json::json!({"items":items,"next_cursor":null}).to_string(),
                ))
            });
        }
        MessageAuth(self.0.clone()).execute(request)
    }
}

fn move_pointer(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
    position: gpui_kit::Point<gpui_kit::Pixels>,
) {
    window.dispatch_event(
        gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers: gpui_kit::Modifiers::default(),
        }),
        cx,
    );
    window.render_frame(cx);
}
fn click_pointer(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
    position: gpui_kit::Point<gpui_kit::Pixels>,
) {
    move_pointer(window, cx, position);
    window.dispatch_event(
        gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
            button: gpui_kit::MouseButton::Left,
            position,
            modifiers: gpui_kit::Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        }),
        cx,
    );
    window.dispatch_event(
        gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
            button: gpui_kit::MouseButton::Left,
            position,
            modifiers: gpui_kit::Modifiers::default(),
            click_count: 1,
        }),
        cx,
    );
    window.render_frame(cx);
}

#[gpui_kit::test]
fn top_sliver_tray_cannot_hit_neighbor_and_focus_reveals_target(cx: &mut TestAppContext) {
    sliver_tray(cx, true, "enter");
}
#[gpui_kit::test]
fn bottom_sliver_tray_cannot_hit_neighbor_and_focus_reveals_target(cx: &mut TestAppContext) {
    sliver_tray(cx, false, "space");
}
fn sliver_tray(cx: &mut TestAppContext, top: bool, key: &str) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        Root::new(
            open_controlled(window, cx, Arc::new(SliverMessageAuth(calls.clone()))),
            window,
            cx,
        )
    });
    login(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        for _ in 0..40 {
            if window.try_find("message-000000000000020").is_some() {
                break;
            }
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(100.))),
                cx,
            );
            window.render_frame(cx);
        }
        let pane = window.find("history").bounds();
        for _ in 0..40 {
            let row = window.find("message-000000000000020").bounds();
            let delta = if top {
                pane.top() + px(8.) - row.bottom()
            } else {
                pane.bottom() - px(8.) - row.top()
            };
            if delta == px(0.) {
                break;
            }
            window.scroll(
                "history",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
                    px(0.),
                    delta.min(px(100.)).max(px(-100.)),
                )),
                cx,
            );
            window.render_frame(cx);
        }
        let row = window.find("message-000000000000020").bounds();
        let composer = window.find("composer-panel").bounds();
        let edge = if top { pane.top() } else { pane.bottom() };
        assert_eq!(
            if top {
                row.bottom() - edge
            } else {
                edge - row.top()
            },
            px(8.),
            "fixture has only 8px visible"
        );
        let x = row.right() - px(24.);
        let owner_point = gpui_kit::point(x, if top { edge + px(4.) } else { edge - px(4.) });
        let start = gpui_kit::point(pane.center().x, owner_point.y);
        for step in 0..=8 {
            move_pointer(
                window,
                cx,
                start + (owner_point - start) * (step as f32 / 8.),
            );
            assert!(
                window.find("trash-message-000000000000020").visible(),
                "tray remains exposed while traversing its owner's sliver"
            );
        }
        // Stay below Jump to latest's bottom margin for the bottom-edge case.
        let neighbor_point = gpui_kit::point(x, if top { edge + px(10.) } else { edge - px(10.) });
        let neighbor = window
            .find(if top {
                "message-000000000000021"
            } else {
                "message-000000000000019"
            })
            .bounds();
        assert!(neighbor.contains(&neighbor_point));
        // The old pane-only clamp exposes an opacity-zero target in this other row.
        move_pointer(window, cx, neighbor_point);
        assert!(!window.find("trash-message-000000000000020").visible());
        click_pointer(window, cx, neighbor_point);
        assert!(
            window.try_find("delete-message-preview").is_none(),
            "neighbor click must not delete the clipped owner's message"
        );
        let trash = window.find("trash-message-000000000000020").bounds();
        assert!(
            trash.top() >= row.top() && trash.bottom() <= row.bottom(),
            "control must remain inside its owning row"
        );
        assert_eq!(window.find("message-000000000000020").bounds(), row);
        assert_eq!(window.find("history").bounds(), pane);
        assert_eq!(window.find("composer-panel").bounds(), composer);
        // The safely clipped 8px fragment is still clickable for its own target.
        click_pointer(window, cx, owner_point);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("delete-message-preview")
                .label()
                .unwrap()
                .starts_with("sliver target\n")
        );
        window.press("escape", cx);
    });
    cx.run_until_parked();
    let before_focus = cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("history-pane", cx);
        window.hover("composer", cx);
        let row = window.find("message-000000000000020").bounds();
        let pane = window.find("history").bounds();
        let composer = window.find("composer-panel").bounds();
        assert_eq!(
            if top {
                row.bottom() - pane.top()
            } else {
                pane.bottom() - row.top()
            },
            px(8.),
            "pointer activation and cancellation preserve the clipped viewport"
        );
        assert!(!window.find("trash-message-000000000000020").visible());
        for _ in 0..12 {
            window.press("tab", cx);
            window.render_frame(cx);
            if window.find("trash-message-000000000000020").focused() == Some(true) {
                break;
            }
        }
        (row, pane, composer)
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let pane = window.find("history").bounds();
        let row = window.find("message-000000000000020").bounds();
        let trash = window.find("trash-message-000000000000020");
        assert_eq!(trash.focused(), Some(true));
        assert!(trash.visible());
        assert!(
            pane.contains(&trash.bounds().origin) && pane.contains(&trash.bounds().bottom_right()),
            "keyboard focus reveals a complete usable tray"
        );
        assert!(
            row.contains(&trash.bounds().origin) && row.contains(&trash.bounds().bottom_right())
        );
        assert_ne!(
            row, before_focus.0,
            "focus deliberately reveals the clipped row"
        );
        assert_eq!(pane, before_focus.1);
        assert_eq!(window.find("composer-panel").bounds(), before_focus.2);
        window.press(key, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("delete-message-preview")
                .label()
                .unwrap()
                .starts_with("sliver target\n")
        );
        assert_eq!(window.find("cancel-message-action").focused(), Some(true));
    });
    assert!(calls.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn authored_trash_keyboard_focus_tooltip_and_activation_share_captured_confirmation(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        Root::new(
            open_controlled(window, cx, Arc::new(MessageAuth(calls.clone()))),
            window,
            cx,
        )
    });
    login(cx);
    for key in ["space", "enter"] {
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.hover("composer", cx);
            window.click("history-pane", cx);
            window.hover("composer", cx);
            let row = window.find("message-000000000000008").bounds();
            let pane = window.find("history").bounds();
            for _ in 0..8 {
                window.press("tab", cx);
                window.render_frame(cx);
                if window.find("trash-message-000000000000008").focused() == Some(true) {
                    break;
                }
            }
            let trash = window.find("trash-message-000000000000008");
            assert_eq!(trash.focused(), Some(true));
            assert!(
                trash.visible(),
                "keyboard focus reveals the unhovered action"
            );
            assert_eq!(window.find("message-000000000000008").bounds(), row);
            assert_eq!(window.find("history").bounds(), pane);
            window.press(key, cx);
        });
        cx.run_until_parked();
        assert!(calls.lock().unwrap().is_empty());
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("delete-message-preview").is_some());
            assert_eq!(window.find("cancel-message-action").focused(), Some(true));
            window.press("enter", cx);
        });
        cx.run_until_parked();
        assert!(calls.lock().unwrap().is_empty());
    }
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.hover("message-000000000000008", cx);
        window.hover("trash-message-000000000000008", cx);
    });
    cx.background_executor.advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("trash-message-tooltip-000000000000008").label(),
            Some("Delete Message")
        );
        window.click("trash-message-000000000000008", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.hover("message-000000000000009", cx);
        window.click("confirm-delete-message", cx);
        window.click("confirm-delete-message", cx);
    });
    cx.run_until_parked();
    let requests = calls.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].0.method(), reqwest::Method::DELETE);
    assert_eq!(
        requests[0].0.url().path(),
        "/api/v1/channels/000000000000001/messages/000000000000008"
    );
    assert!(requests[0].0.body().is_none());
    assert!(requests[0].0.headers().contains_key("authorization"));
    requests[0]
        .1
        .try_send(Ok(Response::controlled(StatusCode::NO_CONTENT, "")))
        .unwrap();
    drop(requests);
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("delete-message-preview").is_none());
        assert!(window.try_find("message-000000000000008").is_none());
    });
}

struct ShortMessageAuth(Arc<Mutex<Vec<(Request, Reply)>>>);
impl RequestAdapter for ShortMessageAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.url().path() == "/api/v1/auth/login" {
            return Box::pin(async {
                Ok(Response::controlled(StatusCode::OK, serde_json::json!({
                "user":{"id":"42","username":"Ada"},"access_token":"synthetic",
                "expires_at":chrono::DateTime::from_timestamp(1_800_000_003, 0).unwrap().to_rfc3339()
            }).to_string()))
            });
        }
        MessageAuth(self.0.clone()).execute(request)
    }
}
#[gpui_kit::test]
fn real_message_confirmation_closes_on_originating_channel_removal_and_session_expiry(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let streams = crate::test_support::live::Streams::default();
    let (_, cx) = cx.add_window_view(|window, cx| {
        Root::new(
            open_with_streams(
                window,
                cx,
                Arc::new(ShortMessageAuth(calls.clone())),
                &streams,
            ),
            window,
            cx,
        )
    });
    login(cx);
    open_dialog(cx, "000000000000008");
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("confirm-delete-message", cx);
    });
    cx.run_until_parked();
    streams.change(serde_json::json!({"type":"channel_deleted","channel_id":"000000000000001"}));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("delete-message-preview").is_none());
        assert!(window.try_find("channel-000000000000001").is_none());
    });
    calls.lock().unwrap()[0]
        .1
        .try_send(Err(ApiError::Unavailable))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("delete-message-feedback").is_none());
        window.click("logout", cx);
    });
    cx.run_until_parked();
    login(cx);
    open_dialog(cx, "000000000000008");
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("confirm-delete-message", cx);
    });
    cx.run_until_parked();
    cx.background_executor.advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("delete-message-preview").is_none());
        assert!(window.try_find("delete-message-feedback").is_none());
        assert!(window.try_find("login").is_some());
        assert!(window.try_find("composer").is_none());
    });
    assert!(
        calls.lock().unwrap()[1]
            .1
            .try_send(Ok(Response::controlled(StatusCode::NO_CONTENT, "")))
            .is_err()
    );
}

struct SelectionComposerAuth {
    deletes: Arc<AtomicUsize>,
    sends: Arc<Mutex<Vec<Sent>>>,
}
impl RequestAdapter for SelectionComposerAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.method() == reqwest::Method::DELETE {
            self.deletes.fetch_add(1, Ordering::SeqCst);
            return Box::pin(async { Err(ApiError::Unavailable) });
        }
        SendAuth(self.sends.clone()).execute(request)
    }
}
#[gpui_kit::test]
fn message_menu_and_tray_cancellation_preserve_selection_copy_and_delete_key_edits_only_composer(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let deletes = Arc::new(AtomicUsize::new(0));
    let sends = Arc::new(Mutex::new(Vec::new()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        Root::new(
            open_controlled(
                window,
                cx,
                Arc::new(SelectionComposerAuth {
                    deletes: deletes.clone(),
                    sends: sends.clone(),
                }),
            ),
            window,
            cx,
        )
    });
    login(cx);
    for (entry, cancel) in [
        ("menu", "escape"),
        ("menu", "button"),
        ("tray", "escape"),
        ("tray", "button"),
    ] {
        if entry == "menu" {
            open_dialog(cx, "000000000000001");
        } else {
            cx.update(|window, cx| {
                window.render_frame(cx);
                let text = window.find("message-text-000000000000001").bounds();
                window.hover("message-000000000000001", cx);
                let trash = window.find("trash-message-000000000000001").bounds();
                window.drag(
                    trash.center(),
                    text.bottom_right() - gpui_kit::point(px(2.), px(2.)),
                    cx,
                );
                assert!(
                    gpui_kit::base::TextSelection::selected_text(window, cx).is_empty(),
                    "dragging from the tray must not start text selection"
                );
                window.hover("message-000000000000001", cx);
                window.click("trash-message-000000000000001", cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            window.render_frame(cx);
            if cancel == "escape" {
                window.press("escape", cx);
            } else {
                window.click("cancel-message-action", cx);
            }
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("delete-message-preview").is_none());
            let bounds = window.find("message-text-000000000000001").bounds();
            window.drag(
                bounds.origin + gpui_kit::point(px(2.), px(2.)),
                bounds.origin
                    + gpui_kit::point(bounds.size.width - px(2.), bounds.size.height - px(2.)),
                cx,
            );
            assert_eq!(
                gpui_kit::base::TextSelection::selected_text(window, cx),
                "first line\nsecond line"
            );
            window.press("ctrl-c", cx);
            assert_eq!(
                cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
                Some("first line\nsecond line")
            );
            window.press("delete", cx);
            window.render_frame(cx);
            assert!(window.try_find("delete-message-preview").is_none());
            assert!(window.try_find("message-000000000000001").is_some());
        });
    }
    cx.update(|window, cx| {
        window.click("composer", cx);
        window.input("draft", cx);
        window.press("home", cx);
        window.press("delete", cx);
        assert_eq!(composer_text(window, cx), "raft");
        window.press("end", cx);
        window.press("shift-enter", cx);
        window.input("line", cx);
        assert_eq!(composer_text(window, cx), "raft\nline");
        assert!(window.try_find("delete-message-preview").is_none());
        window.press("enter", cx);
    });
    cx.run_until_parked();
    assert_eq!(deletes.load(Ordering::SeqCst), 0);
    assert_eq!(sends.lock().unwrap().len(), 1);
    assert_eq!(sends.lock().unwrap()[0].1, "raft\nline");
}
