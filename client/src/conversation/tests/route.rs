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
    task: Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
}
impl crate::api::test_support::StreamAdapter for HeldStream {
    fn open(
        &self,
        request: reqwest::Request,
    ) -> ApiFuture<Result<crate::api::test_support::StreamResponse, ApiError>> {
        let gate = self.gate.clone();
        let task = self.task.clone();
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
            let mut owned = task.lock().unwrap();
            assert!(
                owned.is_none(),
                "exactly one stream in this healthy scenario"
            );
            *owned = Some(tokio::spawn(async move {
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
struct RelayTask(Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>);
impl Drop for RelayTask {
    fn drop(&mut self) {
        if let Some(task) = self.0.lock().unwrap().take() {
            task.abort();
        }
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
        let relay = RelayTask(Arc::new(std::sync::Mutex::new(None)));
        let a_reads = Arc::new(AtomicUsize::new(0));
        let a_transport = HttpTransport::with_adapters(Arc::new(HeldPost {
            inner: CountReads { reads: a_reads.clone(), client: reqwest::Client::new() }, gate: post_gate.clone(),
        }), Arc::new(HeldStream { gate: stream_gate.clone(), task: relay.0.clone() }));
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
        }
        for client in clients { client.close(); }
        cx.executor().run_until_parked();
        let forwarder = relay.0.lock().unwrap().take().unwrap();
        forwarder.abort();
        assert!(forwarder.await.unwrap_err().is_cancelled());
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
        for index in 0..55 {
            bob.client.send_message(channel.clone(), format!("during disconnect {index}")).await.unwrap();
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
        stop(handle, task).await;
    });
}
