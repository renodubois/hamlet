//! Controlled SSE bytes through the same API binding/parser as production.
use crate::api::test_support::{RequestAdapter, StreamAdapter, StreamResponse};
use crate::api::{ApiError, ApiFuture, HttpTransport};
use std::sync::{Arc, Mutex};

type Body = async_channel::Sender<Result<Vec<u8>, ApiError>>;
#[derive(Clone, Default)]
pub(crate) struct Streams(Arc<Mutex<Vec<Body>>>);
impl StreamAdapter for Streams {
    fn open(&self, _: reqwest::Request) -> ApiFuture<Result<StreamResponse, ApiError>> {
        let (send, body) = async_channel::bounded(512);
        send.try_send(Ok(b"event: ready\ndata: {}\n\n".to_vec()))
            .unwrap();
        self.0.lock().unwrap().push(send);
        Box::pin(async move {
            Ok(StreamResponse::Controlled {
                status: reqwest::StatusCode::OK,
                content_type: "text/event-stream".into(),
                body,
            })
        })
    }
}
impl Streams {
    pub fn transport(&self, requests: Arc<dyn RequestAdapter>) -> HttpTransport {
        HttpTransport::with_adapters(requests, Arc::new(self.clone()))
    }
    pub fn change(&self, data: serde_json::Value) {
        self.0
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .try_send(Ok(format!("event: change\ndata: {data}\n\n").into_bytes()))
            .unwrap();
    }
    pub fn disconnect(&self) {
        self.0.lock().unwrap().last().unwrap().close();
    }
    pub fn count(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}
pub(crate) fn transport(requests: Arc<dyn RequestAdapter>) -> HttpTransport {
    Streams::default().transport(requests)
}
