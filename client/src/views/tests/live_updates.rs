//! Reconnect preserves controls and conversation state without triggering reads.
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
fn reconnect_retains_rows_draft_and_creation_input_without_http_reads(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        // Modal hit targets must not depend on wall-time animation under suite load.
        cx.set_reduce_motion(true);
    });
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
        assert!(window.try_find("refresh-channels").is_none());
        assert!(window.try_find("refresh-history").is_none());
    });
    api.ready();
    cx.run_until_parked();
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
        window.click("open-create-channel", cx);
        window.render_frame(cx);
        window.click("channel-name", cx);
        window.input("unfinished room", cx);
    });
    cx.run_until_parked();
    // Shared renames reorder both selected and unselected rows while the
    // conversation and local modal input remain intact.
    let before_y = cx.update(|window, cx| {
        window.render_frame(cx);
        window.find("message-8").bounds().origin.y
    });
    for (id, name) in [("000000000000001", "ZZZ"), ("000000000000002", "AAA")] {
        let data = serde_json::json!({"type":"channel_renamed","channel":{"id":id,"name":name,"type":"text"}});
        api.streams
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .try_send(Ok(format!("event: change\ndata: {data}\n\n").into_bytes()))
            .unwrap();
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("channel-000000000000001").label(), Some("ZZZ"));
        assert_eq!(window.find("channel-000000000000002").label(), Some("AAA"));
        assert!(
            window.find("channel-000000000000002").bounds().origin.y
                < window.find("channel-000000000000001").bounds().origin.y
        );
        assert_eq!(window.find("message-8").bounds().origin.y, before_y);
        assert_eq!(composer_text(window, cx), "draft survives");
        assert_eq!(window.find("channel-name").value(), Some("unfinished room"));
    });
    assert_eq!(api.reads.load(Ordering::SeqCst), 2);
    api.disconnect();
    cx.run_until_parked();
    let assert_retained = |window: &mut Window, cx: &mut gpui_kit::App| {
        window.render_frame(cx);
        assert_eq!(window.find("message-8").label(), Some("retained row"));
        assert_eq!(window.find("channel-name").value(), Some("unfinished room"));
        assert_eq!(composer_text(window, cx), "draft survives");
        // Draft inspection must not dismiss the modal or steal its input focus.
        assert_eq!(window.find("channel-name").value(), Some("unfinished room"));
        assert_eq!(window.find("channel-name").focused(), Some(true));
    };
    cx.update(assert_retained);
    advance(cx, 2);
    assert_eq!(api.stream_count(), 1);
    cx.update(assert_retained);
    advance(cx, 1);
    assert_eq!(api.stream_count(), 2);
    cx.update(assert_retained);
    assert_eq!(api.reads.load(Ordering::SeqCst), 2);
    assert!(pages.try_recv().is_err(), "reconnect cannot read history");
    api.ready();
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_retained(window, cx);
        window.click("cancel-channel", cx);
        window.render_frame(cx);
        assert!(window.try_find("channel-name").is_none());
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
