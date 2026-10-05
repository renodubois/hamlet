//! Real-route two-client verification with transport barriers and actual server restart.
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
        settle(cx, &clients, || clients.iter().all(|c| c.status().is_empty())).await;
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
fn two_authenticated_coordinators_recover_after_actual_server_restart(cx: &mut TestAppContext) {
    actix_web::rt::System::new().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let database = format!("sqlite://{}?mode=rwc", dir.path().join("restart.db").display());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let url = format!("http://{address}");
        let server = serve(listener, hamlet::connect_to_database(&database).await.unwrap());
        let handle = server.handle();
        let task = actix_web::rt::spawn(server);
        let (a_transport, a_reads) = transport();
        let (b_transport, b_reads) = transport();
        let alice = a_transport.server(&url).unwrap().signup("Alice".into(), "long password".into()).await.unwrap();
        let bob = b_transport.server(&url).unwrap().signup("Bob".into(), "long password".into()).await.unwrap();
        assert_ne!(alice.user.id, bob.user.id);
        let channel = alice.client.channels().await.unwrap()[0].id.clone();
        let inactive = alice.client.create_channel("Other room".into()).await.unwrap();
        for index in 0..53 {
            alice.client.send_message(channel.clone(), format!("before restart {index}")).await.unwrap();
        }
        let a = ConversationHandle::new(1, alice.expires_at, alice.client.clone(), Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        let b = ConversationHandle::new(2, bob.expires_at, bob.client.clone(), Execution::controlled(cx.background_executor.clone(), bob.expires_at - 3600));
        let clients = [&a, &b];
        a.start(); b.start();
        settle(cx, &clients, || clients.iter().all(|c| c.status().is_empty())).await;
        // Populate an inactive cache and an old page: recovery must not retain either.
        for client in clients {
            client.select_channel(&channel);
        }
        settle(cx, &clients, || clients.iter().all(|c| ids(c, &channel).len() == 50)).await;
        for client in clients { client.request_older(); }
        settle(cx, &clients, || clients.iter().all(|c| ids(c, &channel).len() == 53)).await;
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
        settle(cx, &clients, || clients.iter().all(|c| ids(c, &channel).len() == 55 && c.read().draft(&channel).is_empty())).await;
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
        let revisions = [a.read().recovery_reset_revision(), b.read().recovery_reset_revision()];
        // Stop the actual Actix server/workers and await completion, closing both TCP bodies.
        stop(handle, task).await;
        settle(cx, &clients, || clients.iter().all(|c| c.status() == "Reconnecting… messages may be out of date")).await;
        for client in clients {
            assert!(client.read().history.is_empty());
            assert!(matches!(client.read().history_for_display(&channel), Some(Load::Ready(items)) if items.len() == 56));
            assert!(client.read().older.is_empty());
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
        cx.background_executor.advance_clock(Duration::from_secs(1));
        settle(cx, &clients, || clients.iter().all(|c| c.status().is_empty())).await;
        assert_eq!(ids(&a, &channel), ids(&b, &channel));
        for (index, client) in clients.iter().enumerate() {
            let state = client.read();
            let Some(Load::Ready(messages)) = state.history.get(&channel) else { panic!("newest history") };
            assert_eq!(messages.len(), 50);
            assert!(messages.iter().all(|m| m.text.starts_with("during disconnect")));
            assert_eq!(state.history.len(), 1, "inactive histories discarded");
            assert_eq!(state.older.get(&channel), Some(&Older::Available));
            assert_eq!(state.draft(&channel), format!("unsent draft {index}"));
            assert_eq!(state.draft(&inactive.id), "inactive draft");
            assert_eq!(state.selected.as_deref(), Some(channel.as_str()));
            assert!(state.recovery_reset_revision() > revisions[index]);
            assert!(matches!(&state.channels, Some(Load::Ready(channels)) if channels.contains(&offline)));
        }
        assert_eq!([a_reads.load(Ordering::SeqCst), b_reads.load(Ordering::SeqCst)], [reads[0] + 2, reads[1] + 2], "one fresh channels/newest baseline per reconnected client");
        // New cursors traverse the current dataset, not the pre-restart exhausted cursor.
        for client in clients { client.request_older(); }
        settle(cx, &clients, || clients.iter().all(|c| ids(c, &channel).len() == 100)).await;
        assert_eq!(ids(&a, &channel), ids(&b, &channel));
        for client in clients { client.close(); }
        cx.executor().run_until_parked();
        stop(handle, task).await;
    });
}
