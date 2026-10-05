//! Real-route conversation verification, including two-client orders and server restart.
use super::{ConversationHandle, Load, Older};
use crate::{
    api::{
        ApiError, ApiFuture, HttpTransport,
        test_support::{RequestAdapter, Response},
    },
    runtime::Execution,
};
use actix_web::{
    App, HttpServer,
    dev::{Server, ServerHandle},
    web,
};
use gpui_kit::TestAppContext;
use std::{
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

struct CountReads {
    reads: Arc<AtomicUsize>,
    client: reqwest::Client,
}
impl RequestAdapter for CountReads {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.method() == reqwest::Method::GET {
            self.reads.fetch_add(1, Ordering::SeqCst);
        }
        RequestAdapter::execute(&self.client, request)
    }
}
fn transport() -> (HttpTransport, Arc<AtomicUsize>) {
    let reads = Arc::new(AtomicUsize::new(0));
    (
        HttpTransport::with_adapter(Arc::new(CountReads {
            reads: reads.clone(),
            client: reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(8))
                .build()
                .unwrap(),
        })),
        reads,
    )
}
struct CountStreams {
    attempts: Arc<AtomicUsize>,
    client: reqwest::Client,
}
impl crate::api::test_support::StreamAdapter for CountStreams {
    fn open(
        &self,
        request: reqwest::Request,
    ) -> ApiFuture<Result<crate::api::test_support::StreamResponse, ApiError>> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        crate::api::test_support::StreamAdapter::open(&self.client, request)
    }
}
fn restart_transport() -> (HttpTransport, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let reads = Arc::new(AtomicUsize::new(0));
    let attempts = Arc::new(AtomicUsize::new(0));
    (
        HttpTransport::with_adapters(
            Arc::new(CountReads {
                reads: reads.clone(),
                client: reqwest::Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(8))
                    .build()
                    .unwrap(),
            }),
            Arc::new(CountStreams {
                attempts: attempts.clone(),
                client: reqwest::Client::builder().no_proxy().build().unwrap(),
            }),
        ),
        reads,
        attempts,
    )
}
fn serve(listener: TcpListener, state: hamlet::AppState) -> Server {
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(hamlet::routes)
    })
    .workers(2)
    .disable_signals()
    .listen(listener)
    .unwrap()
    .run()
}
async fn stop(handle: ServerHandle, task: actix_web::rt::task::JoinHandle<std::io::Result<()>>) {
    tokio::time::timeout(Duration::from_secs(5), handle.stop(false))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
async fn settle(
    cx: &mut TestAppContext,
    clients: &[&ConversationHandle],
    ready: impl Fn() -> bool,
) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            cx.executor().run_until_parked();
            for client in clients {
                while let Ok(update) = client.updates().try_recv() {
                    assert!(client.apply(update).is_none());
                }
            }
            if ready() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("bounded real-route convergence");
}
fn ids(client: &ConversationHandle, channel: &str) -> Vec<String> {
    let state = client.read();
    let Some(Load::Ready(messages)) = state.history.get(channel) else {
        return Vec::new();
    };
    messages.iter().map(|message| message.id.clone()).collect()
}

// Each gate delays delivery, never constructs a response/event. The real routes,
// credential binding, JSON/SSE decoding and coordinator workflow remain in use.
struct Gate {
    hold: std::sync::atomic::AtomicBool,
    reached: std::sync::atomic::AtomicBool,
    release: tokio::sync::Semaphore,
}
impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            hold: std::sync::atomic::AtomicBool::new(false),
            reached: std::sync::atomic::AtomicBool::new(false),
            release: tokio::sync::Semaphore::new(0),
        })
    }
    fn arm(&self) {
        self.reached.store(false, Ordering::SeqCst);
        self.hold.store(true, Ordering::SeqCst);
    }
    async fn wait(&self) {
        if self.hold.load(Ordering::SeqCst) {
            self.reached.store(true, Ordering::SeqCst);
            tokio::time::timeout(Duration::from_secs(5), self.release.acquire())
                .await
                .unwrap()
                .unwrap()
                .forget();
        }
    }
    fn open(&self) {
        self.hold.store(false, Ordering::SeqCst);
        self.release.add_permits(1);
    }
}
struct HeldPost {
    inner: CountReads,
    gate: Arc<Gate>,
}
impl RequestAdapter for HeldPost {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        let held = request.method() == reqwest::Method::POST
            && (request.url().path().ends_with("/messages")
                || request.url().path() == "/api/v1/channels");
        let response = self.inner.execute(request);
        let gate = self.gate.clone();
        Box::pin(async move {
            let response = response.await?;
            if held {
                gate.wait().await;
            }
            Ok(response)
        })
    }
}
struct HeldStream {
    gate: Arc<Gate>,
    tasks: Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    attempts: Arc<AtomicUsize>,
}
impl crate::api::test_support::StreamAdapter for HeldStream {
    fn open(
        &self,
        request: reqwest::Request,
    ) -> ApiFuture<Result<crate::api::test_support::StreamResponse, ApiError>> {
        let gate = self.gate.clone();
        let tasks = self.tasks.clone();
        self.attempts.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let mut response = reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap()
                .execute(request)
                .await
                .map_err(|_| ApiError::Unavailable)?;
            let status = response.status();
            let content_type = response.headers()[reqwest::header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .to_owned();
            let (send, body) = async_channel::bounded(1);
            tasks.lock().unwrap().push(tokio::spawn(async move {
                // Finite relay budget, cancel-on-test-cleanup; no EOF collection.
                for _ in 0..64 {
                    let chunk = tokio::time::timeout(Duration::from_secs(5), response.chunk())
                        .await
                        .unwrap()
                        .unwrap();
                    let Some(chunk) = chunk else { return };
                    gate.wait().await;
                    if send.send(Ok(chunk.to_vec())).await.is_err() {
                        return;
                    }
                }
                panic!("finite test relay budget exceeded");
            }));
            Ok(crate::api::test_support::StreamResponse::Controlled {
                status,
                content_type,
                body,
            })
        })
    }
}
#[derive(Default)]
struct RelayTasks(Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>);
impl RelayTasks {
    fn stream(&self, gate: Arc<Gate>, attempts: Arc<AtomicUsize>) -> HeldStream {
        HeldStream {
            gate,
            tasks: self.0.clone(),
            attempts,
        }
    }
    fn disconnect(&self) {
        self.0.lock().unwrap().last().unwrap().abort();
    }
    async fn cancel_latest(&self) {
        let task = self.0.lock().unwrap().pop().unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }
}
impl Drop for RelayTasks {
    fn drop(&mut self) {
        for task in self.0.lock().unwrap().drain(..) {
            task.abort();
        }
    }
}

// Faults/delays at the HTTP boundary still execute the real route first. Losing
// a successful POST response models an uncertain outcome, not a failed write.
struct ControlledResponses {
    inner: CountReads,
    channels: Arc<Gate>,
    history: Arc<Gate>,
    post: Arc<Gate>,
    fail_history: Arc<std::sync::atomic::AtomicBool>,
    lose_post: Arc<std::sync::atomic::AtomicBool>,
    writes: Arc<AtomicUsize>,
}
impl RequestAdapter for ControlledResponses {
    fn execute(&self, request: reqwest::Request) -> ApiFuture<Result<Response, ApiError>> {
        let get = request.method() == reqwest::Method::GET;
        let history = get && request.url().path().ends_with("/messages");
        let post = request.method() == reqwest::Method::POST
            && request.url().path().ends_with("/messages");
        let gate = if history {
            self.history.clone()
        } else if get {
            self.channels.clone()
        } else {
            self.post.clone()
        };
        let fail = (history && self.fail_history.swap(false, Ordering::SeqCst))
            || (post && self.lose_post.swap(false, Ordering::SeqCst));
        if post {
            self.writes.fetch_add(1, Ordering::SeqCst);
        }
        let response = self.inner.execute(request);
        Box::pin(async move {
            let response = response.await?;
            if get || post {
                gate.wait().await;
            }
            if fail {
                Err(ApiError::Unavailable)
            } else {
                Ok(response)
            }
        })
    }
}

// The host only delivers opaque updates. All HTTP dispatch, completion policy and
// continuation/scheduling decisions stay in the production coordinator.
async fn await_conversation(
    cx: &mut TestAppContext,
    activity: &ConversationHandle,
    ready: impl Fn(&ConversationHandle) -> bool,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.executor().run_until_parked();
        while let Ok(update) = activity.updates().try_recv() {
            assert!(activity.apply(update).is_none());
        }
        if ready(activity) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "conversation workflow did not complete"
        );
        // Yield only for real loopback I/O; application time advances explicitly.
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

#[gpui_kit::test]
fn second_user_creations_arrive_on_session_stream_without_followup_reads(cx: &mut TestAppContext) {
    actix_web::rt::System::new().block_on(async {
        use actix_web::{App, HttpServer, web};
        let dir = tempfile::tempdir().unwrap();
        let db = hamlet::connect_to_database(&format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("live.db").display()
        ))
        .await
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(db.clone()))
                .configure(hamlet::routes)
        })
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);
        let reads = Arc::new(AtomicUsize::new(0));
        let api = HttpTransport::with_adapter(Arc::new(CountReads { reads: reads.clone(), client: reqwest::Client::new() })).server(&url).unwrap();
        let alice = api
            .signup("Alice".into(), "long password".into())
            .await
            .unwrap();
        let bob = api
            .signup("Bob".into(), "long password".into())
            .await
            .unwrap();
        let channel = alice.client.channels().await.unwrap()[0].id.clone();
        let activity = ConversationHandle::new(
            1,
            alice.expires_at,
            alice.client,
            Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600),
        );
        activity.start();
        await_conversation(cx, &activity, |activity| {
            activity.status().is_empty() && matches!(
                activity.read().history.get(&channel),
                Some(Load::Ready(messages)) if messages.is_empty()
            )
        })
        .await;
        let baseline_reads = reads.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(activity.status(), "");
        let posted = bob
            .client
            .send_message(
                channel.clone(),
                "Bob published while Alice was reading".into(),
            )
            .await
            .unwrap();
        let new_channel = bob.client.create_channel("Bob room".into()).await.unwrap();
        await_conversation(cx, &activity, |activity| {
            matches!(
                activity.read().history.get(&channel),
                Some(Load::Ready(messages)) if messages.iter().any(|m| m.id == posted.id && m.author_name == "Bob")
            )
        })
        .await;
        await_conversation(cx, &activity, |activity| {
            matches!(
                activity.read().channels,
                Some(Load::Ready(ref channels)) if channels.contains(&new_channel)
            )
        })
        .await;
        assert_eq!(activity.read().selected.as_deref(), Some(channel.as_str()));
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), baseline_reads, "both creations delivered without a follow-up HTTP read");
        activity.close();
        cx.executor().run_until_parked();
        handle.stop(true).await;
    });
}

#[gpui_kit::test]
fn real_route_independent_reads_pending_and_uncertain_writes_survive_reconnect(
    cx: &mut TestAppContext,
) {
    actix_web::rt::System::new().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let state = hamlet::connect_to_database(&format!("sqlite://{}?mode=rwc", dir.path().join("local-work.db").display())).await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = serve(listener, state);
        let handle = server.handle();
        let task = actix_web::rt::spawn(server);
        let channels_gate = Gate::new();
        let history_gate = Gate::new();
        let post_gate = Gate::new();
        let stream_gate = Gate::new();
        let fail_history = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let lose_post = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reads = Arc::new(AtomicUsize::new(0));
        let writes = Arc::new(AtomicUsize::new(0));
        let attempts = Arc::new(AtomicUsize::new(0));
        let relay = RelayTasks::default();
        let api = HttpTransport::with_adapters(Arc::new(ControlledResponses {
            inner: CountReads { reads: reads.clone(), client: reqwest::Client::new() },
            channels: channels_gate.clone(), history: history_gate.clone(), post: post_gate.clone(),
            fail_history: fail_history.clone(), lose_post: lose_post.clone(), writes: writes.clone(),
        }), Arc::new(relay.stream(stream_gate.clone(), attempts.clone()))).server(&url).unwrap();
        let alice = api.signup("Alice".into(), "long password".into()).await.unwrap();
        let bob = HttpTransport::new().server(&url).unwrap().signup("Bob".into(), "long password".into()).await.unwrap();
        let other = bob.client.create_channel("zz-other".into()).await.unwrap();
        channels_gate.arm();
        stream_gate.arm();
        let a = ConversationHandle::new(1, alice.expires_at, alice.client.clone(), Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        a.start();
        settle(cx, &[&a], || channels_gate.reached.load(Ordering::SeqCst) && stream_gate.reached.load(Ordering::SeqCst)).await;
        channels_gate.open();
        settle(cx, &[&a], || a.read().selected.as_ref().is_some_and(|id| matches!(a.read().history.get(id), Some(Load::Ready(_))))).await;
        let original = a.read().selected.clone().unwrap();
        assert_ne!(original, other.id);
        assert_eq!(a.status(), "Connecting…", "initial HTTP loads finish before stream readiness");
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        stream_gate.open();
        settle(cx, &[&a], || a.status().is_empty()).await;

        fail_history.store(true, Ordering::SeqCst);
        a.select_channel(&other.id);
        settle(cx, &[&a], || matches!(a.read().history.get(&other.id), Some(Load::Failed(_)))).await;
        assert_eq!(a.status(), "");
        assert_eq!(attempts.load(Ordering::SeqCst), 1, "a local read failure does not restart live delivery");
        a.select_channel(&original);
        history_gate.arm();
        a.select_channel(&other.id);
        settle(cx, &[&a], || history_gate.reached.load(Ordering::SeqCst)).await;
        let overlap = bob.client.send_message(other.id.clone(), "read overlap may be lost".into()).await.unwrap();
        let barrier = bob.client.send_message(original.clone(), "overlap delivery barrier".into()).await.unwrap();
        settle(cx, &[&a], || ids(&a, &original).contains(&barrier.id)).await;
        a.edit_draft("pending across reconnect".into());
        post_gate.arm();
        a.send();
        settle(cx, &[&a], || post_gate.reached.load(Ordering::SeqCst)).await;
        let before_reads = reads.load(Ordering::SeqCst);
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        relay.disconnect();
        settle(cx, &[&a], || a.status() == "Live updates disconnected — reconnecting.").await;
        cx.background_executor.advance_clock(Duration::from_millis(2999));
        cx.executor().run_until_parked();
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        cx.background_executor.advance_clock(Duration::from_millis(1));
        settle(cx, &[&a], || a.status().is_empty()).await;
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(reads.load(Ordering::SeqCst), before_reads);
        assert_eq!(writes.load(Ordering::SeqCst), 1, "pending write is not resent");
        assert_eq!(a.read().selected.as_deref(), Some(other.id.as_str()));
        assert_eq!(a.read().draft(&other.id), "pending across reconnect");
        assert!(a.read().send_pending.contains_key(&other.id));
        assert!(matches!(a.read().history.get(&other.id), Some(Load::Loading)));
        history_gate.open();
        settle(cx, &[&a], || matches!(a.read().history.get(&other.id), Some(Load::Ready(_)))).await;
        assert!(ids(&a, &other.id).is_empty(), "overlap event was not staged into the old HTTP snapshot");
        assert!(a.read().send_pending.contains_key(&other.id));
        post_gate.open();
        settle(cx, &[&a], || a.read().draft(&other.id).is_empty()).await;
        let confirmed = alice.client.history_page(other.id.clone(), None).await.unwrap().items.into_iter().find(|m| m.text == "pending across reconnect").unwrap();
        assert_eq!(ids(&a, &other.id), std::slice::from_ref(&confirmed.id));
        assert!(matches!(a.read().history.get(&other.id), Some(Load::Ready(messages)) if messages == std::slice::from_ref(&confirmed)));
        assert!(!ids(&a, &other.id).contains(&overlap.id));

        lose_post.store(true, Ordering::SeqCst);
        a.edit_draft("identical uncertain text".into());
        a.send();
        settle(cx, &[&a], || a.read().uncertain.contains(&other.id) && ids(&a, &other.id).len() == 2).await;
        let feedback = a.read().send_feedback[&other.id].clone();
        let before_reads = reads.load(Ordering::SeqCst);
        relay.disconnect();
        settle(cx, &[&a], || a.status() == "Live updates disconnected — reconnecting.").await;
        cx.background_executor.advance_clock(Duration::from_secs(3));
        settle(cx, &[&a], || a.status().is_empty()).await;
        // Same author and text, distinct entity ID: an event is not an originating
        // HTTP confirmation, even after reconnect and even if the write did commit.
        let matching = alice.client.send_message(other.id.clone(), "identical uncertain text".into()).await.unwrap();
        settle(cx, &[&a], || ids(&a, &other.id).contains(&matching.id)).await;
        assert_eq!(a.read().draft(&other.id), "identical uncertain text");
        assert!(a.read().uncertain.contains(&other.id));
        assert_eq!(a.read().send_feedback[&other.id], feedback);
        assert_eq!(reads.load(Ordering::SeqCst), before_reads);
        assert_eq!(writes.load(Ordering::SeqCst), 3, "two deliberate coordinator writes and one explicit API write, no automatic resend");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        a.close();
        cx.background_executor.advance_clock(Duration::from_secs(60));
        cx.executor().run_until_parked();
        assert_eq!(attempts.load(Ordering::SeqCst), 3, "session teardown cannot reconnect");
        assert!(a.read().channels.is_none());
        stop(handle, task).await;
    });
}

#[gpui_kit::test]
fn server_history_traverses_multiple_pages_with_timestamp_ties(cx: &mut TestAppContext) {
    actix_web::rt::System::new().block_on(async {
        use actix_web::{App, HttpServer, web};
        use hamlet::{connect_to_database, routes};
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        let dir = tempfile::tempdir().unwrap();
        let db = connect_to_database(&format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("pages.db").display()
        ))
        .await
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new({
            let db = db.clone();
            move || {
                App::new()
                    .app_data(web::Data::new(db.clone()))
                    .configure(routes)
            }
        })
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);
        let auth = HttpTransport::new().server(&url).unwrap();
        let login = auth
            .signup("Alice".into(), "long password".into())
            .await
            .unwrap();
        let channel = login.client.channels().await.unwrap()[0].id.clone();
        for ix in 0..53 {
            let text = format!("line {ix}");
            assert_eq!(
                login
                    .client
                    .send_message(channel.clone(), text.clone())
                    .await
                    .unwrap()
                    .text,
                text
            );
        }
        db.db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "UPDATE messages SET created_at = '2026-01-01T00:00:00.000000000Z'",
            ))
            .await
            .unwrap();
        let activity = ConversationHandle::new(
            1,
            login.expires_at,
            login.client.clone(),
            Execution::controlled(cx.background_executor.clone(), login.expires_at - 3600),
        );
        activity.start();
        await_conversation(cx, &activity, |activity| {
            activity.status().is_empty() && matches!(
                activity.read().history.get(&channel),
                Some(Load::Ready(messages)) if messages.len() == 50
            )
        })
        .await;
        assert_history(&activity, &channel, 50, 0);
        assert_eq!(activity.read().older.get(&channel), Some(&Older::Available));

        // A burst larger than two newest pages arrives as creations, without traversal.
        // The original older cursor must still reach the three oldest messages.
        for ix in 53..158 {
            let text = format!("line {ix}");
            assert_eq!(
                login
                    .client
                    .send_message(channel.clone(), text.clone())
                    .await
                    .unwrap()
                    .text,
                text
            );
        }
        await_conversation(cx, &activity, |activity| {
            matches!(activity.read().history.get(&channel), Some(Load::Ready(items)) if items.len() == 155)
        })
        .await;
        // 105 new plus the 50 initially loaded; the oldest three remain paged.
        assert_history(&activity, &channel, 50, 105);
        assert_eq!(activity.read().older.get(&channel), Some(&Older::Available));
        activity.request_older();
        await_conversation(cx, &activity, |activity| {
            activity.read().older.get(&channel) == Some(&Older::Exhausted)
        })
        .await;
        assert_history(&activity, &channel, 53, 105);
        activity.close();
        cx.executor().run_until_parked();
        handle.stop(true).await;
    });
}

fn assert_history(activity: &ConversationHandle, channel: &str, old: usize, new: usize) {
    let state = activity.read();
    let Some(Load::Ready(messages)) = state.history.get(channel) else {
        panic!("history missing");
    };
    assert_eq!(messages.len(), old + new);
    let lines = messages
        .iter()
        .map(|message| {
            message
                .text
                .strip_prefix("line ")
                .unwrap()
                .parse::<usize>()
                .unwrap()
        })
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        lines.len(),
        old + new,
        "every published message appears once"
    );
    assert_eq!(lines.iter().filter(|&&ix| ix < 53).count(), old);
    assert_eq!(
        lines.iter().filter(|&&ix| (53..158).contains(&ix)).count(),
        new
    );
    assert_eq!(
        messages
            .iter()
            .map(|message| &message.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        messages.len(),
        "live creations and older pages must not duplicate identities"
    );
    assert!(
        messages
            .iter()
            .filter(|message| message.text[5..].parse::<usize>().unwrap() < 53)
            .all(|message| message.created_at.starts_with("2026-01-01"))
    );
    assert!(
        messages
            .windows(2)
            .all(|pair| pair[0].created_at > pair[1].created_at
                || (pair[0].created_at == pair[1].created_at
                    && pair[0].id.parse::<i64>().unwrap() > pair[1].id.parse::<i64>().unwrap())),
        "server timestamp and ID tie order must survive decoding and live merges"
    );
}

#[gpui_kit::test]
fn real_route_http_event_orders_reconcile_once_without_healthy_reads(cx: &mut TestAppContext) {
    actix_web::rt::System::new().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let database = format!("sqlite://{}?mode=rwc", dir.path().join("orders.db").display());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = serve(listener, hamlet::connect_to_database(&database).await.unwrap());
        let handle = server.handle();
        let task = actix_web::rt::spawn(server);
        let post_gate = Gate::new();
        let stream_gate = Gate::new();
        let relay = RelayTasks::default();
        let attempts = Arc::new(AtomicUsize::new(0));
        let a_reads = Arc::new(AtomicUsize::new(0));
        let a_transport = HttpTransport::with_adapters(Arc::new(HeldPost {
            inner: CountReads { reads: a_reads.clone(), client: reqwest::Client::new() }, gate: post_gate.clone(),
        }), Arc::new(relay.stream(stream_gate.clone(), attempts.clone())));
        let (b_transport, b_reads) = transport();
        let alice = a_transport.server(&url).unwrap().signup("Alice".into(), "long password".into()).await.unwrap();
        let bob = b_transport.server(&url).unwrap().signup("Bob".into(), "long password".into()).await.unwrap();
        assert_ne!(alice.user.id, bob.user.id);
        let a = ConversationHandle::new(1, alice.expires_at, alice.client, Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        let b = ConversationHandle::new(2, bob.expires_at, bob.client.clone(), Execution::controlled(cx.background_executor.clone(), bob.expires_at - 3600));
        let clients = [&a, &b];
        a.start(); b.start();
        settle(cx, &clients, || clients.iter().all(|c| c.status().is_empty() && c.read().selected.as_ref().is_some_and(|id| matches!(c.read().history.get(id), Some(Load::Ready(_)))))).await;
        let channel = a.read().selected.clone().unwrap();
        let reads = [a_reads.load(Ordering::SeqCst), b_reads.load(Ordering::SeqCst)];
        let Some(Load::Ready(initial_channels)) = a.read().channels.clone() else { panic!("initial channels") };
        let initial_channel_count = initial_channels.len();
        // Subsequent successful frames are finite barriers behind these failures.
        // A phantom creation would change the integrated histories/channel lists.
        assert_eq!(bob.client.send_message(channel.clone(), " ".into()).await, Err(ApiError::InvalidInput));
        assert_eq!(bob.client.send_message("999999999999999".into(), "missing channel".into()).await, Err(ApiError::NotFound));
        assert_eq!(bob.client.create_channel("bad!".into()).await, Err(ApiError::InvalidInput));
        assert_eq!(bob.client.create_channel(initial_channels[0].name.clone()).await, Err(ApiError::Conflict));
        post_gate.arm();
        a.edit_draft("event before response".into()); a.send();
        settle(cx, &clients, || post_gate.reached.load(Ordering::SeqCst) && clients.iter().all(|c| ids(c, &channel).len() == 1)).await;
        assert_eq!(a.read().draft(&channel), "event before response", "real event cannot confirm the originating operation");
        post_gate.open();
        settle(cx, &clients, || a.read().draft(&channel).is_empty()).await;
        assert_eq!(ids(&a, &channel), ids(&b, &channel));

        stream_gate.arm();
        a.edit_draft("response before event".into()); a.send();
        settle(cx, &clients, || stream_gate.reached.load(Ordering::SeqCst) && a.read().draft(&channel).is_empty() && ids(&b, &channel).len() == 2).await;
        assert_eq!(ids(&a, &channel), ids(&b, &channel), "HTTP confirmation renders before Alice's real event is released");
        stream_gate.open();
        let remote = bob.client.create_channel("after duplicate barrier".into()).await.unwrap();
        settle(cx, &clients, || clients.iter().all(|c| matches!(&c.read().channels, Some(Load::Ready(channels)) if channels.contains(&remote)))).await;
        assert_eq!(ids(&a, &channel).len(), 2, "released duplicate does not allocate a second row");
        assert_eq!(ids(&a, &channel), ids(&b, &channel));
        assert_eq!([a_reads.load(Ordering::SeqCst), b_reads.load(Ordering::SeqCst)], reads);
        for client in clients {
            assert_eq!(client.read().selected.as_deref(), Some(channel.as_str()));
            assert!(!client.read().history.contains_key(&remote.id));
        }
        // Channel confirmations have the same two arrival orders. Local creation
        // deliberately selects its new uncached channel (one navigation read);
        // remote observers must do no read and must not change their selection.
        for (index, event_first) in [true, false].into_iter().enumerate() {
            let name = if event_first { "event first room" } else { "response first room" };
            let previous_selection = a.read().selected.clone();
            let previous_confirmation = a.created();
            if event_first { post_gate.arm(); } else { stream_gate.arm(); }
            a.create_channel(name);
            let has_channel = |client: &ConversationHandle| matches!(&client.read().channels, Some(Load::Ready(channels)) if channels.iter().any(|c| c.name == name));
            if event_first {
                settle(cx, &clients, || post_gate.reached.load(Ordering::SeqCst) && clients.iter().all(|c| has_channel(c))).await;
                assert_eq!(a.read().selected, previous_selection, "event alone cannot select or confirm local creation");
                assert_eq!(a.created(), previous_confirmation);
                post_gate.open();
            }
            settle(cx, &clients, || a.created() != previous_confirmation && a.read().selected.as_ref().is_some_and(|id| matches!(a.read().history.get(id), Some(Load::Ready(_)))) && has_channel(&b)).await;
            if !event_first {
                settle(cx, &clients, || stream_gate.reached.load(Ordering::SeqCst)).await;
                stream_gate.open();
            }
            // A later selected-history event is a finite barrier behind the released
            // duplicate channel event, without changing either current selection.
            let barrier = bob.client.send_message(channel.clone(), format!("channel order barrier {index}")).await.unwrap();
            settle(cx, &clients, || clients.iter().all(|c| ids(c, &channel).contains(&barrier.id))).await;
            let selected = a.read().selected.clone().unwrap();
            for client in clients {
                let state = client.read();
                let Some(Load::Ready(channels)) = &state.channels else { panic!("channels") };
                assert_eq!(channels.iter().filter(|c| c.name == name).count(), 1);
            }
            assert_eq!(b.read().selected.as_deref(), Some(channel.as_str()));
            assert!(!b.read().history.contains_key(&selected));
            assert_eq!([a_reads.load(Ordering::SeqCst), b_reads.load(Ordering::SeqCst)], [reads[0] + index + 1, reads[1]], "only local selection loads history; no confirmation/event catch-up reads");
            for client in clients {
                assert!(matches!(&client.read().channels, Some(Load::Ready(channels)) if channels.len() == initial_channel_count + index + 2), "failed writes emit no phantom channels");
                assert_eq!(ids(client, &channel).len(), index + 3, "only confirmed sends and explicit barriers appear");
                assert!(matches!(client.read().history.get(&channel), Some(Load::Ready(messages)) if messages.contains(&barrier)), "complete HTTP and event entities agree");
            }
        }
        for client in clients { client.close(); }
        cx.executor().run_until_parked();
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        relay.cancel_latest().await;
        stop(handle, task).await;
    });
}

#[gpui_kit::test]
fn two_authenticated_coordinators_reconnect_without_catchup_after_actual_server_restart(
    cx: &mut TestAppContext,
) {
    actix_web::rt::System::new().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let database = format!("sqlite://{}?mode=rwc", dir.path().join("restart.db").display());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let url = format!("http://{address}");
        let server = serve(listener, hamlet::connect_to_database(&database).await.unwrap());
        let handle = server.handle();
        let task = actix_web::rt::spawn(server);
        let (a_transport, a_reads, a_attempts) = restart_transport();
        let (b_transport, b_reads, b_attempts) = restart_transport();
        let alice = a_transport.server(&url).unwrap().signup("Alice".into(), "long password".into()).await.unwrap();
        let bob = b_transport.server(&url).unwrap().signup("Bob".into(), "long password".into()).await.unwrap();
        assert_ne!(alice.user.id, bob.user.id);
        let channel = alice.client.channels().await.unwrap()[0].id.clone();
        let inactive = alice.client.create_channel("Other room".into()).await.unwrap();
        for index in 0..103 {
            alice.client.send_message(channel.clone(), format!("before restart {index}")).await.unwrap();
        }
        let a = ConversationHandle::new(1, alice.expires_at, alice.client.clone(), Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        let b = ConversationHandle::new(2, bob.expires_at, bob.client.clone(), Execution::controlled(cx.background_executor.clone(), bob.expires_at - 3600));
        let clients = [&a, &b];
        a.start(); b.start();
        settle(cx, &clients, || clients.iter().all(|c| c.status().is_empty())).await;
        // Load an older page and an inactive cache. Both, including the still-usable
        // cursor to the oldest three messages, must survive stream failure.
        for client in clients {
            client.select_channel(&channel);
        }
        settle(cx, &clients, || clients.iter().all(|c| ids(c, &channel).len() == 50)).await;
        for client in clients { client.request_older(); }
        settle(cx, &clients, || clients.iter().all(|c| ids(c, &channel).len() == 100)).await;
        for client in clients { client.select_channel(&inactive.id); }
        settle(cx, &clients, || clients.iter().all(|c| matches!(c.read().history.get(&inactive.id), Some(Load::Ready(_))))).await;
        for client in clients {
            client.edit_draft("inactive draft".into());
            client.select_channel(&channel);
            client.edit_draft("selected draft".into());
        }
        let reads = [a_reads.load(Ordering::SeqCst), b_reads.load(Ordering::SeqCst)];
        // Independently authenticated concurrent writes, real HTTP confirmations plus events.
        a.edit_draft("Alice concurrent".into()); a.send();
        b.edit_draft("Bob concurrent".into()); b.send();
        settle(cx, &clients, || clients.iter().all(|c| ids(c, &channel).len() == 102 && c.read().draft(&channel).is_empty())).await;
        assert_eq!(ids(&a, &channel), ids(&b, &channel), "same sorted identities, no duplicate HTTP/event entities");
        let remote = bob.client.create_channel("Remote room".into()).await.unwrap();
        let ignored = bob.client.send_message(remote.id.clone(), "unloaded history".into()).await.unwrap();
        settle(cx, &clients, || clients.iter().all(|c| matches!(&c.read().channels, Some(Load::Ready(channels)) if channels.contains(&remote)))).await;
        // The next selected event is a barrier behind the ignored message on both streams.
        let barrier = bob.client.send_message(channel.clone(), "healthy barrier".into()).await.unwrap();
        settle(cx, &clients, || clients.iter().all(|c| ids(c, &channel).contains(&barrier.id))).await;
        for (index, client) in clients.iter().enumerate() {
            assert_eq!(client.read().selected.as_deref(), Some(channel.as_str()));
            assert!(!client.read().history.contains_key(&ignored.channel_id));
            client.edit_draft(format!("unsent draft {index}"));
        }
        assert_eq!([a_reads.load(Ordering::SeqCst), b_reads.load(Ordering::SeqCst)], reads, "healthy message/channel creations cause zero follow-up reads");
        let retained = ids(&a, &channel);
        let retained_channels = a.read().channels.clone();
        assert_eq!(retained.len(), 103);
        cx.background_executor.advance_clock(Duration::from_secs(10));
        settle(cx, &clients, || clients.iter().all(|c| c.status().is_empty())).await;
        assert_eq!([a_reads.load(Ordering::SeqCst), b_reads.load(Ordering::SeqCst)], reads, "healthy timer ticks perform no periodic polling");
        assert_eq!([a_attempts.load(Ordering::SeqCst), b_attempts.load(Ordering::SeqCst)], [1, 1]);
        // Stop the actual Actix server/workers and await completion, closing both TCP bodies.
        stop(handle, task).await;
        let disconnected = "Live updates disconnected — reconnecting.";
        settle(cx, &clients, || clients.iter().all(|c| c.status() == disconnected)).await;
        for client in clients {
            assert_eq!(ids(client, &channel), retained);
            assert_eq!(client.read().history.len(), 2);
            assert!(matches!(client.read().history.get(&inactive.id), Some(Load::Ready(items)) if items.is_empty()));
            assert_eq!(client.read().older.get(&channel), Some(&Older::Available));
        }
        // A fresh AppState/pool/hub and listener, same persisted database/credentials/origin.
        let server = serve(TcpListener::bind(address).unwrap(), hamlet::connect_to_database(&database).await.unwrap());
        let handle = server.handle();
        let task = actix_web::rt::spawn(server);
        // While client policy clocks remain stopped, create more than a newest page.
        let mut missed = Vec::new();
        for index in 0..55 {
            missed.push(bob.client.send_message(channel.clone(), format!("during disconnect {index}")).await.unwrap());
        }
        let offline = bob.client.create_channel("Offline room".into()).await.unwrap();
        cx.background_executor.advance_clock(Duration::from_millis(2999));
        cx.executor().run_until_parked();
        assert!(clients.iter().all(|c| c.status() == disconnected));
        assert_eq!([a_attempts.load(Ordering::SeqCst), b_attempts.load(Ordering::SeqCst)], [1, 1], "no reconnect before three seconds");
        cx.background_executor.advance_clock(Duration::from_millis(1));
        settle(cx, &clients, || clients.iter().all(|c| c.status().is_empty())).await;
        assert_eq!([a_attempts.load(Ordering::SeqCst), b_attempts.load(Ordering::SeqCst)], [2, 2]);
        for (index, client) in clients.iter().enumerate() {
            let state = client.read();
            let Some(Load::Ready(messages)) = state.history.get(&channel) else { panic!("retained history") };
            assert_eq!(ids(client, &channel), retained, "offline creations are accepted losses, not replayed");
            assert!(messages.iter().all(|m| !m.text.starts_with("during disconnect")));
            assert_eq!(state.history.len(), 2, "inactive histories retained");
            assert_eq!(state.older.get(&channel), Some(&Older::Available));
            assert_eq!(state.draft(&channel), format!("unsent draft {index}"));
            assert_eq!(state.draft(&inactive.id), "inactive draft");
            assert_eq!(state.selected.as_deref(), Some(channel.as_str()));
            assert_eq!(state.channels, retained_channels);
            assert!(matches!(&state.channels, Some(Load::Ready(channels)) if !channels.contains(&offline)));
        }
        assert_eq!([a_reads.load(Ordering::SeqCst), b_reads.load(Ordering::SeqCst)], reads, "reconnect performs no catch-up GETs");
        // Delivery resumes for future creations without backfilling the offline gap.
        let resumed = bob.client.send_message(channel.clone(), "after reconnect".into()).await.unwrap();
        let resumed_channel = bob.client.create_channel("After reconnect room".into()).await.unwrap();
        let inactive_message = bob.client.send_message(inactive.id.clone(), "inactive cache still live".into()).await.unwrap();
        settle(cx, &clients, || clients.iter().all(|c| ids(c, &channel).contains(&resumed.id) && ids(c, &inactive.id).contains(&inactive_message.id) && matches!(&c.read().channels, Some(Load::Ready(channels)) if channels.contains(&resumed_channel)))).await;
        assert_eq!(ids(&a, &channel), ids(&b, &channel));
        assert_eq!([a_reads.load(Ordering::SeqCst), b_reads.load(Ordering::SeqCst)], reads);
        // Explicit pagination uses the retained pre-restart cursor, reaching only
        // the oldest three messages, not the newer offline gap.
        for client in clients { client.request_older(); }
        settle(cx, &clients, || clients.iter().all(|c| c.read().older.get(&channel) == Some(&Older::Exhausted))).await;
        for client in clients {
            assert_eq!(ids(client, &channel).len(), 107);
            assert!(matches!(client.read().history.get(&channel), Some(Load::Ready(messages)) if messages.iter().all(|m| !m.text.starts_with("during disconnect"))));
            assert_eq!(client.read().selected.as_deref(), Some(channel.as_str()));
        }
        assert_eq!(ids(&a, &channel), ids(&b, &channel));
        assert_eq!([a_reads.load(Ordering::SeqCst), b_reads.load(Ordering::SeqCst)], [reads[0] + 1, reads[1] + 1], "only explicit older-page requests read");
        for client in clients { client.close(); }
        cx.executor().run_until_parked();
        // Reopening the conversation is an ordinary initial channel/history load,
        // not a Refresh control or a special catch-up operation. The newest page
        // can retrieve missed creations; no promise is made about the whole gap.
        let reopened = ConversationHandle::new(3, alice.expires_at, alice.client.clone(), Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        reopened.start();
        settle(cx, &[&reopened], || matches!(&reopened.read().channels, Some(Load::Ready(channels)) if channels.contains(&offline))).await;
        reopened.select_channel(&channel);
        settle(cx, &[&reopened], || ids(&reopened, &channel).contains(&missed.last().unwrap().id)).await;
        assert!(matches!(reopened.read().history.get(&channel), Some(Load::Ready(messages)) if messages.contains(missed.last().unwrap())));
        assert_eq!(b_reads.load(Ordering::SeqCst), reads[1] + 1, "the other closed client performs no reads");
        assert_eq!(a_reads.load(Ordering::SeqCst), reads[0] + 3, "one ordinary channel list and one newest history read after reopening");
        reopened.close();
        cx.executor().run_until_parked();
        stop(handle, task).await;
    });
}
