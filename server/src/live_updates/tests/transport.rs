use super::tests::change;
use crate::{connect_to_database, routes};
use actix_web::{
    App, HttpServer,
    middleware::{Compress, DefaultHeaders},
    test, web,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};

struct StreamClient {
    reader: BufReader<TcpStream>,
    pending: Vec<u8>,
}

impl StreamClient {
    async fn connect(address: std::net::SocketAddr, token: &str) -> (Self, String) {
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut socket = TcpStream::connect(address).await.unwrap();
            socket.write_all(format!("GET /api/v1/events HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nAccept-Encoding: gzip\r\n\r\n").as_bytes()).await.unwrap();
            let mut reader = BufReader::new(socket);
            let mut status = String::new();
            reader.read_line(&mut status).await.unwrap();
            assert!(status.starts_with("HTTP/1.1 200"));
            let mut headers = String::new();
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).await.unwrap() > 0);
                if line == "\r\n" { break; }
                headers.push_str(&line.to_lowercase());
                assert!(headers.len() < 8192);
            }
            assert!(headers.contains("content-type: text/event-stream\r\n"));
            assert!(headers.contains("content-encoding: identity\r\n"), "compression middleware must not batch SSE");
            assert!(headers.contains("transfer-encoding: chunked\r\n"));
            let worker = headers.lines().find_map(|line| line.strip_prefix("x-test-worker: ")).unwrap().to_owned();
            (Self { reader, pending: Vec::new() }, worker)
        }).await.expect("bounded HTTP handshake")
    }

    async fn frame(&mut self) -> Vec<u8> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(end) = self.pending.windows(2).position(|bytes| bytes == b"\n\n") {
                    return self.pending.drain(..end + 2).collect();
                }
                let mut length = String::new();
                assert!(self.reader.read_line(&mut length).await.unwrap() > 0);
                let length = usize::from_str_radix(length.trim(), 16).unwrap();
                assert!((1..=65536).contains(&length));
                let mut chunk = vec![0; length + 2];
                self.reader.read_exact(&mut chunk).await.unwrap();
                assert_eq!(&chunk[length..], b"\r\n");
                self.pending.extend_from_slice(&chunk[..length]);
                assert!(self.pending.len() <= 65536);
            }
        })
        .await
        .expect("bounded SSE frame")
    }
}

/// Bounded owner-local measurement, not a production benchmark or capacity claim.
/// Eight actual HTTP readers plus one deterministically unpolled HTTP body. Writes
/// use registered routes and SQLite; latency starts at route submission (not TCP POST).
#[actix_web::test]
#[ignore = "optional fanout measurement, not a release prerequisite"]
async fn measured_bounded_fanout_at_ten_changes_per_second() {
    use actix_web::body::MessageBody;
    use std::{pin::Pin, time::Instant};

    const CLIENTS: usize = 8;
    const CHANGES: usize = 270;
    const PERIOD: Duration = Duration::from_millis(100);
    let dir = tempfile::tempdir().unwrap();
    let state = connect_to_database(&format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("fanout.db").display()
    ))
    .await
    .unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(routes),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(json!({"username":"Fanout", "password":"long password"}))
            .to_request(),
    )
    .await;
    let auth: Value = test::read_body_json(response).await;
    let token = auth["access_token"].as_str().unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    let channels: Value = test::read_body_json(response).await;
    let channel = channels["items"][0]["id"].as_str().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker_state = state.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(worker_state.clone()))
            .wrap(DefaultHeaders::new().add(("x-test-worker", "fanout")))
            .configure(routes)
    })
    .workers(2)
    .disable_signals()
    .listen(listener)
    .unwrap()
    .run();
    let handle = server.handle();
    let task = actix_web::rt::spawn(server);
    let mut clients = Vec::with_capacity(CLIENTS);
    let mut readiness_ms = Vec::with_capacity(CLIENTS);
    for _ in 0..CLIENTS {
        let start = Instant::now();
        let (mut client, _) = StreamClient::connect(address, token).await;
        assert_eq!(client.frame().await, b"event: ready\ndata: {}\n\n");
        readiness_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        clients.push(client);
    }
    // Unlike an unread TCP peer (which may still fit in kernel/Actix buffers),
    // this registered HTTP body's pause deterministically backpressures the hub.
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    let mut slow = response.into_body();
    let ready = tokio::time::timeout(
        Duration::from_secs(5),
        std::future::poll_fn(|cx| Pin::new(&mut slow).poll_next(cx)),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    assert_eq!(ready, "event: ready\ndata: {}\n\n");
    assert_eq!(state.events.sender.receiver_count(), CLIENTS + 1);

    let texts = ["x".repeat(128), "\u{1}\n🦀\"".repeat(1000)];
    let mut frame_sizes = Vec::with_capacity(CHANGES);
    let mut latencies = Vec::with_capacity(CHANGES * CLIENTS);
    let mut write_ms = Vec::with_capacity(CHANGES);
    let mut retained_peak = 0;
    let mut retained_bytes_peak = 0;
    let mut schedule_lateness_ms = 0.0_f64;
    let started = Instant::now();
    for index in 0..CHANGES {
        let due = started + PERIOD * index as u32;
        tokio::time::sleep_until(tokio::time::Instant::from_std(due)).await;
        schedule_lateness_ms = schedule_lateness_ms.max(due.elapsed().as_secs_f64() * 1000.0);
        let submitted = Instant::now();
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            test::call_service(
                &app,
                test::TestRequest::post()
                    .uri(&format!("/api/v1/channels/{channel}/messages"))
                    .insert_header(("Authorization", format!("Bearer {token}")))
                    .set_json(json!({"text":texts[index % 2]}))
                    .to_request(),
            ),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), actix_web::http::StatusCode::CREATED);
        let message: Value = test::read_body_json(response).await;
        write_ms.push(submitted.elapsed().as_secs_f64() * 1000.0);
        for (client_index, client) in clients.iter_mut().enumerate() {
            // A real 15-second heartbeat may precede the change; finite frames only.
            let mut frame = client.frame().await;
            if frame.starts_with(b":") {
                frame = client.frame().await;
            }
            latencies.push(submitted.elapsed().as_secs_f64() * 1000.0);
            assert!(frame.starts_with(b"event: change\ndata: "));
            let event: Value =
                serde_json::from_slice(&frame[b"event: change\ndata: ".len()..frame.len() - 2])
                    .unwrap();
            assert_eq!(event["type"], "message_created");
            assert_eq!(event["message"], message);
            if client_index == 0 {
                frame_sizes.push(frame.len());
            }
        }
        let retained = state.events.sender.len();
        retained_peak = retained_peak.max(retained);
        // All healthy readers consumed this frame; only the paused body retains
        // the trailing ring. Each Bytes allocation is shared, never x subscribers.
        let retained_bytes: usize = frame_sizes.iter().rev().take(retained).sum();
        retained_bytes_peak = retained_bytes_peak.max(retained_bytes);
        assert!(retained <= 256);
        if index == 256 {
            let ended = tokio::time::timeout(
                Duration::from_secs(5),
                std::future::poll_fn(|cx| Pin::new(&mut slow).poll_next(cx)),
            )
            .await
            .unwrap();
            assert!(
                ended.is_none(),
                "slow body ends on change 257; never resumes after skipped changes"
            );
            assert_eq!(state.events.sender.receiver_count(), CLIENTS);
            assert_eq!(state.events.sender.len(), 0);
        }
    }
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        started + PERIOD * CHANGES as u32,
    ))
    .await;
    let duration = started.elapsed().as_secs_f64();
    latencies.sort_by(f64::total_cmp);
    write_ms.sort_by(f64::total_cmp);
    assert_eq!(retained_peak, 256);
    assert_eq!(latencies.len(), CHANGES * CLIENTS);
    assert_eq!(state.events.sender.len(), 0);
    println!(
        "FANOUT_METRICS {}",
        json!({
            "healthy_tcp_clients": CLIENTS, "paused_http_bodies": 1, "changes": CHANGES,
            "duration_seconds": duration, "changes_per_second": CHANGES as f64 / duration,
            "message_characters": [128,4000], "message_utf8_bytes": [texts[0].len(), texts[1].len()],
            "frame_bytes_min": frame_sizes.iter().min().unwrap(), "frame_bytes_max": frame_sizes.iter().max().unwrap(),
            "ready_ms": readiness_ms, "route_to_last_byte_ms_p50": latencies[latencies.len()/2],
            "route_to_last_byte_ms_p95": latencies[latencies.len()*95/100], "route_to_last_byte_ms_max": latencies.last().unwrap(),
            "write_ms_p95": write_ms[write_ms.len()*95/100], "write_ms_max": write_ms.last().unwrap(),
            "schedule_lateness_ms_max": schedule_lateness_ms, "shared_retained_events_peak": retained_peak,
            "shared_retained_frame_bytes_peak": retained_bytes_peak, "retained_events_after_slow_close": state.events.sender.len(),
            "slow_terminal_at_change": 257, "healthy_deliveries": latencies.len(),
            "healthy_total_frame_bytes": frame_sizes.iter().sum::<usize>() * CLIENTS
        })
    );
    drop(clients);
    drop(slow);
    tokio::time::timeout(Duration::from_secs(5), handle.stop(false))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    // Server future completion can precede worker-thread body destruction.
    tokio::time::timeout(Duration::from_secs(5), async {
        while state.events.sender.receiver_count() != 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("worker shutdown releases all subscriptions");
    assert_eq!(state.events.sender.len(), 0);
}

#[actix_web::test]
async fn real_workers_share_fanout_and_disconnect_releases_receivers() {
    let dir = tempfile::tempdir().unwrap();
    let state = connect_to_database(&format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("transport.db").display()
    ))
    .await
    .unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(routes),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(json!({"username":"Alice", "password":"long password"}))
            .to_request(),
    )
    .await;
    let auth: Value = test::read_body_json(response).await;
    let token = auth["access_token"].as_str().unwrap();
    let workers = Arc::new(AtomicUsize::new(0));
    let worker_state = state.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = HttpServer::new(move || {
        let worker = workers.fetch_add(1, Ordering::SeqCst);
        App::new()
            .app_data(web::Data::new(worker_state.clone()))
            .wrap(DefaultHeaders::new().add(("x-test-worker", worker.to_string())))
            .wrap(Compress::default())
            .configure(routes)
    })
    .workers(2)
    .disable_signals()
    .listen(listener)
    .unwrap()
    .run();
    let handle = server.handle();
    let server_task = actix_web::rt::spawn(server);
    let mut clients = Vec::new();
    let mut seen = BTreeSet::new();
    for _ in 0..8 {
        let (mut client, worker) = StreamClient::connect(address, token).await;
        assert_eq!(client.frame().await, b"event: ready\ndata: {}\n\n");
        seen.insert(worker);
        clients.push(client);
        if seen.len() == 2 {
            break;
        }
    }
    assert_eq!(
        seen.len(),
        2,
        "subscriptions actually served on distinct workers"
    );
    assert_eq!(state.events.sender.receiver_count(), clients.len());
    state.events.notify(change());
    for client in &mut clients {
        assert_eq!(client.frame().await, b"event: change\ndata: {\"type\":\"channel_created\",\"channel\":{\"id\":\"100000000000001\",\"name\":\"general\",\"type\":\"text\"}}\n\n");
    }
    for client in &mut clients {
        client.reader.get_mut().shutdown().await.unwrap();
    }
    drop(clients);
    tokio::time::timeout(Duration::from_secs(5), async {
        while state.events.sender.receiver_count() != 0 {
            // HTTP/1 permits a read-half close while a response is still running.
            // A subsequent write detects the vanished reader (heartbeats do this when idle).
            state.events.notify(change());
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("disconnect drops the streaming body and subscriptions");
    assert_eq!(state.events.sender.len(), 0);
    tokio::time::timeout(Duration::from_secs(5), handle.stop(false))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), server_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
