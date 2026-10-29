//! Shared headless history host, HTTP fixture and chat delivery driver.
use super::MessageHistoryView;
use crate::api::test_support::{RequestAdapter, Response};
use crate::api::{ApiError, ApiFuture};
use crate::chat::ChatHandle;
use crate::runtime::Execution;
use gpui_kit::component::button::Button;
use gpui_kit::*;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) struct HistoryHost {
    pub(super) activity: ChatHandle,
    pub(super) history: Entity<MessageHistoryView>,
    pub(super) visible: bool,
}
impl Render for HistoryHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut host = div().size_full().flex().flex_col();
        if self.visible {
            host = host.child(self.history.clone());
        }
        host.child(
            Button::new("recreate-history")
                .label("Recreate history")
                .on_click(cx.listener(|host, _, window, cx| {
                    host.history = cx.new(|cx| {
                        MessageHistoryView::new(
                            host.activity.clone(),
                            Execution::controlled(cx.background_executor().clone(), 1_800_000_000),
                            window,
                            cx,
                        )
                    });
                    cx.notify();
                })),
        )
        .child(
            Button::new("toggle-history")
                .label("Toggle history")
                .on_click(cx.listener(|host, _, _, cx| {
                    host.visible = !host.visible;
                    cx.notify();
                })),
        )
    }
}
pub(super) fn drain(cx: &mut gpui_kit::VisualTestContext, activity: &ChatHandle) {
    loop {
        cx.run_until_parked();
        let Ok(update) = activity.updates().try_recv() else {
            break;
        };
        assert!(activity.apply(update).is_none());
    }
}

pub(super) fn mount_history(
    cx: &mut TestAppContext,
    api: std::sync::Arc<dyn RequestAdapter>,
) -> (ChatHandle, &mut VisualTestContext) {
    mount_history_with_transport(cx, crate::test_support::live::transport(api))
}

pub(super) fn mount_history_with_transport(
    cx: &mut TestAppContext,
    transport: crate::api::HttpTransport,
) -> (ChatHandle, &mut VisualTestContext) {
    let (activity, _, cx) = mount_history_with_view(cx, transport);
    (activity, cx)
}

pub(super) fn mount_history_with_view(
    cx: &mut TestAppContext,
    transport: crate::api::HttpTransport,
) -> (
    ChatHandle,
    Entity<MessageHistoryView>,
    &mut VisualTestContext,
) {
    cx.update(gpui_kit::init);
    let client = transport
        .server("https://history.example")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    let activity = ChatHandle::new(
        1,
        "u".into(),
        1_800_001_000,
        client,
        Execution::controlled(cx.background_executor.clone(), 1_800_000_000),
    );
    activity.start();
    let mut history_view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let history = cx.new(|cx| {
            MessageHistoryView::new(
                activity.clone(),
                Execution::controlled(cx.background_executor().clone(), 1_800_000_000),
                window,
                cx,
            )
        });
        history_view = Some(history.clone());
        let host = cx.new(|_| HistoryHost {
            activity: activity.clone(),
            history,
            visible: true,
        });
        gpui_kit::component::Root::new(host, window, cx)
    });
    drain(cx, &activity);
    (activity, history_view.unwrap(), cx)
}

pub(super) struct ReaderHistory {
    pub(super) reads: AtomicUsize,
    pub(super) created_at: &'static str,
}
impl RequestAdapter for ReaderHistory {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        assert_eq!(request.method(), reqwest::Method::GET);
        self.reads.fetch_add(1, Ordering::SeqCst);
        let body = if request.url().path() == "/api/v1/channels" {
            serde_json::json!({"items":[{"id":"1","name":"General","type":"text"}]})
        } else {
            serde_json::json!({"items":(1..=40).rev().map(|id| {
                serde_json::json!({"id":id.to_string(),"channel_id":"1",
                    "author":{"id":"u","display_name":"Ada"},"text":format!("message {id}\nsecond line"),
                    "created_at":self.created_at})
            }).collect::<Vec<_>>()})
        };
        Box::pin(async move {
            Ok(Response::controlled(
                reqwest::StatusCode::OK,
                body.to_string(),
            ))
        })
    }
}
