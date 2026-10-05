//! Semantic controls exercise connection notices without reconnect-triggered reads.
use super::*;
use crate::api::test_support::{StreamAdapter, StreamResponse};

type StreamBody = async_channel::Sender<Result<Vec<u8>, ApiError>>;
struct LiveUpdatesApi {
    pages: PagedAuth,
    reads: AtomicUsize,
    streams: Mutex<Vec<StreamBody>>,
}
impl RequestAdapter for LiveUpdatesApi {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.method() == reqwest::Method::GET {
            self.reads.fetch_add(1, Ordering::SeqCst);
        }
        self.pages.execute(request)
    }
}
impl StreamAdapter for LiveUpdatesApi {
    fn open(&self, _: Request) -> ApiFuture<Result<StreamResponse, ApiError>> {
        let (send, body) = async_channel::bounded(8);
        self.streams.lock().unwrap().push(send);
        Box::pin(async move {
            Ok(StreamResponse::Controlled {
                status: StatusCode::OK,
                content_type: "text/event-stream".into(),
                body,
            })
        })
    }
}
impl LiveUpdatesApi {
    fn ready(&self) {
        self.streams
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .try_send(Ok(b"event: ready\ndata: {}\n\n".to_vec()))
            .unwrap();
    }
    fn disconnect(&self) {
        self.streams.lock().unwrap().last().unwrap().close();
    }
    fn stream_count(&self) -> usize {
        self.streams.lock().unwrap().len()
    }
}

#[gpui_kit::test]
fn reconnect_notice_retains_rows_draft_and_creation_input_without_http_reads(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (send, pages) = std::sync::mpsc::channel();
    let api = Arc::new(LiveUpdatesApi {
        pages: PagedAuth(send),
        reads: AtomicUsize::new(0),
        streams: Mutex::new(Vec::new()),
    });
    let (_, cx) = cx.add_window_view(|window, cx| {
        let execution =
            crate::runtime::Execution::controlled(cx.background_executor().clone(), 1_800_000_000);
        let view = crate::views::app_shell::open(
            window,
            cx,
            HttpTransport::with_adapters(api.clone(), api.clone()),
            crate::storage::Config::default(),
            None,
            execution,
        );
        Root::new(view, window, cx)
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
    // HTTP history starts even while the stream has not declared readiness.
    let (_, initial) = pages.try_recv().unwrap();
    assert_eq!(api.reads.load(Ordering::SeqCst), 2);
    assert_eq!(api.stream_count(), 1);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("connection-status").label(),
            Some("Connecting…")
        );
        assert!(window.try_find("refresh-channels").is_none());
        assert!(window.try_find("refresh-history").is_none());
    });
    api.ready();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("connection-status").label(), Some(""));
    });
    initial
        .try_send(Ok(crate::api::Page {
            items: vec![crate::api::Message {
                id: "8".into(),
                channel_id: "000000000000001".into(),
                author_id: "42".into(),
                author_name: "Ada".into(),
                text: "retained row".into(),
                created_at: "2026-01-01T00:00:00Z".into(),
            }],
            next_cursor: None,
        }))
        .unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("draft survives", cx);
        window.click("channel-name", cx);
        window.input("unfinished room", cx);
    });
    api.disconnect();
    cx.run_until_parked();
    let assert_retained = |window: &mut Window, cx: &mut gpui_kit::App| {
        window.render_frame(cx);
        assert_eq!(window.find("message-8").label(), Some("retained row"));
        assert_eq!(composer_text(window, cx), "draft survives");
        assert_eq!(window.find("channel-name").value(), Some("unfinished room"));
    };
    let assert_disconnected = |window: &mut Window, cx: &mut gpui_kit::App| {
        assert_retained(window, cx);
        assert_eq!(
            window.find("connection-status").label(),
            Some("Live updates disconnected — reconnecting.")
        );
    };
    cx.update(assert_disconnected);
    advance(cx, 2);
    assert_eq!(api.stream_count(), 1);
    cx.update(assert_disconnected);
    advance(cx, 1);
    assert_eq!(api.stream_count(), 2);
    cx.update(assert_disconnected);
    assert_eq!(api.reads.load(Ordering::SeqCst), 2);
    assert!(pages.try_recv().is_err(), "reconnect cannot read history");
    api.ready();
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_retained(window, cx);
        assert_eq!(window.find("connection-status").label(), Some(""));
        window.click("logout", cx);
    });
    assert_eq!(api.reads.load(Ordering::SeqCst), 2);
    cx.run_until_parked();
    let count = api.stream_count();
    advance(cx, 60);
    assert_eq!(api.stream_count(), count);
    assert_eq!(api.reads.load(Ordering::SeqCst), 2);
    assert!(pages.try_recv().is_err());
}
