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
