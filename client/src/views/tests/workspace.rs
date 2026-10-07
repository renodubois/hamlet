//! Composition journeys through Kit Root, semantic controls and the owned workspace seam.
use crate::api::test_support::{RequestAdapter, Response};
use crate::api::{ApiError, ApiFuture};
use crate::runtime::Execution;
use crate::views::workspace::WorkspaceView;
use crate::workspace::WorkspaceHandle;
use gpui_kit::component::{Root, button::Button};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, Window, div,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Channels(Arc<AtomicUsize>);
impl RequestAdapter for Channels {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let body = if request.url().path() == "/api/v1/channels" {
            r#"{"items":[{"id":"2","name":"Zebra","type":"text"},{"id":"1","name":"Alpha","type":"text"}]}"#
        } else if request.url().path().contains("/2/") {
            r#"{"items":[{"id":"zebra-message","channel_id":"2","author":{"id":"u","display_name":"Ada"},"text":"cached zebra history","created_at":"2026-01-01T00:00:00Z"}]}"#
        } else if request.url().path().contains("/1/") {
            r#"{"items":[{"id":"alpha-message","channel_id":"1","author":{"id":"u","display_name":"Ada"},"text":"cached alpha history","created_at":"2026-01-01T00:00:00Z"}]}"#
        } else {
            r#"{"items":[]}"#
        };
        Box::pin(async move { Ok(Response::controlled(reqwest::StatusCode::OK, body)) })
    }
}
struct WorkspaceHost {
    activity: WorkspaceHandle,
    workspace: Entity<WorkspaceView>,
    visible: bool,
}
impl Render for WorkspaceHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialogs = Root::render_dialog_layer(window, cx);
        let mut host = div().relative().size_full().flex().flex_col();
        if self.visible {
            host = host.child(self.workspace.clone());
        }
        host.children(dialogs.map(|layer| div().absolute().inset_0().child(layer)))
            .child(
                Button::new("recreate-workspace")
                    .label("Recreate workspace")
                    .on_click(cx.listener(|host, _, window, cx| {
                        host.workspace = cx.new(|cx| {
                            WorkspaceView::new(
                                host.activity.clone(),
                                Execution::controlled(
                                    cx.background_executor().clone(),
                                    1_800_000_000,
                                ),
                                window,
                                cx,
                            )
                        });
                        cx.notify();
                    })),
            )
            .child(
                Button::new("toggle-workspace")
                    .label("Toggle workspace")
                    .on_click(cx.listener(|host, _, _, cx| {
                        host.visible = !host.visible;
                        cx.notify();
                    })),
            )
    }
}
fn drain(cx: &mut gpui_kit::VisualTestContext, activity: &WorkspaceHandle) {
    loop {
        cx.run_until_parked();
        let Ok(update) = activity.updates().try_recv() else {
            break;
        };
        assert!(activity.apply(update).is_none());
    }
}
#[gpui_kit::test]
fn workspace_hydrates_cached_selection_and_recreates_without_requests(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut host_entity = None;
    let client = crate::test_support::live::transport(Arc::new(Channels(calls.clone())))
        .server("https://workspace.example")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    let activity = WorkspaceHandle::new(
        1,
        1_800_001_000,
        client,
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
    );
    activity.start();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            WorkspaceView::new(
                activity.clone(),
                Execution::controlled(cx.background_executor().clone(), 1_800_000_000),
                window,
                cx,
            )
        });
        let host = cx.new(|_| WorkspaceHost {
            activity: activity.clone(),
            workspace,
            visible: true,
        });
        host_entity = Some(host.clone());
        Root::new(host, window, cx)
    });
    drain(cx, &activity);
    assert_eq!(activity.read().selected.as_deref(), Some("2"));
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("channel-2").label(), Some("Zebra"));
        assert_eq!(window.find("channel-header").label(), Some("# Zebra"));
        assert!(
            window.find("channel-2").bounds().origin.y < window.find("channel-1").bounds().origin.y
        );
        window.click("composer", cx);
        window.input("zebra draft", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.click("channel-1", cx);
    });
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("alpha draft", cx);
        assert_eq!(window.find("channel-header").label(), Some("# Alpha"));
        window.click("open-create-channel", cx);
        window.render_frame(cx);
        window.click("channel-name", cx);
        window.input("local unsent name", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        // Native modals block background controls; simulate external recreation.
        host_entity.as_ref().unwrap().update(cx, |host, cx| {
            host.workspace = cx.new(|cx| {
                WorkspaceView::new(
                    host.activity.clone(),
                    Execution::controlled(cx.background_executor().clone(), 1_800_000_000),
                    window,
                    cx,
                )
            });
            cx.notify();
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("channel-name").is_none());
        assert_eq!(window.find("channel-header").label(), Some("# Alpha"));
        assert_eq!(
            window.find("message-alpha-message").label(),
            Some("cached alpha history")
        );
        window.click("composer", cx);
        window.press("ctrl-a", cx);
        window.press("ctrl-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "alpha draft"
        );
        window.click("channel-2", cx);
    });
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("message-zebra-message").label(),
            Some("cached zebra history")
        );
        window.click("composer", cx);
        window.press("ctrl-a", cx);
        window.press("ctrl-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "zebra draft"
        );
        window.click("open-create-channel", cx);
        window.render_frame(cx);
        window.click("channel-name", cx);
        window.input("clear even when hidden", cx);
        window.click("cancel-channel", cx);
        window.click("toggle-workspace", cx);
    });
    activity.close();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.click("toggle-workspace", cx);
        window.render_frame(cx);
        assert!(window.try_find("composer").is_none());
        assert!(window.try_find("channel-2").is_none());
        assert!(window.try_find("channel-header").is_none());
        assert!(window.try_find("channel-name").is_none());
        assert!(window.try_find("refresh-channels").is_none());
    });
    drain(cx, &activity);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "recreation, cache and shutdown must not dispatch"
    );
}

type Creation = (String, async_channel::Sender<Result<Response, ApiError>>);
struct DelayedCreation(std::sync::Mutex<Vec<Creation>>);
impl RequestAdapter for DelayedCreation {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.method() == reqwest::Method::POST {
            let body: serde_json::Value =
                serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
            let (send, receive) = async_channel::bounded(1);
            self.0
                .lock()
                .unwrap()
                .push((body["name"].as_str().unwrap().into(), send));
            return Box::pin(async move { receive.recv().await.unwrap() });
        }
        Channels(Arc::new(AtomicUsize::new(0))).execute(request)
    }
}

#[gpui_kit::test]
fn sidebar_dialog_creation_tracks_pending_state_across_recreation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let api = Arc::new(DelayedCreation(std::sync::Mutex::new(Vec::new())));
    let client = crate::test_support::live::transport(api.clone())
        .server("https://workspace.example")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    let activity = WorkspaceHandle::new(
        1,
        1_800_001_000,
        client,
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
    );
    activity.start();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            WorkspaceView::new(
                activity.clone(),
                Execution::controlled(cx.background_executor().clone(), 1_800_000_000),
                window,
                cx,
            )
        });
        let host = cx.new(|_| WorkspaceHost {
            activity: activity.clone(),
            workspace,
            visible: true,
        });
        Root::new(host, window, cx)
    });
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("channel-name").is_none());
        window.click("open-create-channel", cx);
        window.render_frame(cx);
        window.input("Room", cx); // Opening focuses the input.
        window.click("create-channel", cx);
        window.render_frame(cx);
        window.click("create-channel", cx);
        assert_eq!(
            window.find("create-channel").label(),
            Some("Creating channel…")
        );
        window.click("cancel-channel", cx);
        window.render_frame(cx);
        window.click("recreate-workspace", cx);
    });
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("channel-name").is_none());
        assert_eq!(
            window.find("open-create-channel").label(),
            Some("Creating channel…")
        );
        window.click("open-create-channel", cx);
        window.render_frame(cx);
        assert!(window.try_find("channel-name").is_none());
    });
    assert_eq!(
        api.0.lock().unwrap().len(),
        1,
        "recreation cannot replay a pending write"
    );
    let (name, reply) = api.0.lock().unwrap().remove(0);
    assert_eq!(name, "Room");
    reply
        .try_send(Ok(Response::controlled(
            reqwest::StatusCode::CREATED,
            r#"{"id":"3","name":"Room","type":"text"}"#,
        )))
        .unwrap();
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("channel-3").label(), Some("Room"));
        window.click("open-create-channel", cx);
        window.render_frame(cx);
        assert_eq!(window.find("channel-name").value(), Some(""));
        window.input("Next room", cx);
        window.click("create-channel", cx);
    });
    drain(cx, &activity);
    let (name, reply) = api.0.lock().unwrap().remove(0);
    assert_eq!(name, "Next room");
    reply.try_send(Err(ApiError::Unavailable)).unwrap();
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("create-channel").label(),
            Some("Create text channel")
        );
        assert!(
            window
                .find("channel-feedback")
                .label()
                .unwrap()
                .contains("may have succeeded")
        );
        assert_eq!(window.find("channel-name").value(), Some("Next room"));
    });
    assert_eq!(activity.read().selected.as_deref(), Some("3"));
    assert!(api.0.lock().unwrap().is_empty());
    activity.close();
    drain(cx, &activity);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("channel-name").is_none());
    });
}
