//! Shared headless history host, HTTP fixture and workspace delivery driver.
use crate::api::test_support::{RequestAdapter, Response};
use crate::api::{ApiError, ApiFuture};
use crate::runtime::Execution;
use crate::views::conversation::message_history::MessageHistoryView;
use crate::workspace::WorkspaceHandle;
use gpui_kit::component::button::Button;
use gpui_kit::*;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) struct HistoryHost {
    pub(super) activity: WorkspaceHandle,
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
pub(super) fn drain(cx: &mut gpui_kit::VisualTestContext, activity: &WorkspaceHandle) {
    loop {
        cx.run_until_parked();
        let Ok(update) = activity.updates().try_recv() else {
            break;
        };
        assert!(activity.apply(update).is_none());
    }
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
