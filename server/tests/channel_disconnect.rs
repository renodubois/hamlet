use actix_web::{App, HttpServer, dev::Service, test, web};
use hamlet::{connect_to_database, routes};
use serde_json::{Value, json};
use std::{
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    sync::oneshot,
};

struct RequestLifetime {
    ended: Option<oneshot::Sender<bool>>,
    completed: bool,
}
impl Drop for RequestLifetime {
    fn drop(&mut self) {
        if let Some(ended) = self.ended.take() {
            let _ = ended.send(self.completed);
        }
    }
}

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("bounded transport/barrier")
}

async fn read_frame(reader: &mut BufReader<TcpStream>) -> Vec<u8> {
    bounded(async {
        let mut frame = Vec::new();
        while !frame.ends_with(b"\n\n") {
            let mut length = String::new();
            assert!(
                reader.read_line(&mut length).await.unwrap() > 0,
                "stream ended"
            );
            let length = usize::from_str_radix(length.trim(), 16).unwrap();
            assert!((1..=65536).contains(&length));
            let mut chunk = vec![0; length + 2];
            reader.read_exact(&mut chunk).await.unwrap();
            assert_eq!(&chunk[length..], b"\r\n");
            frame.extend_from_slice(&chunk[..length]);
            assert!(frame.len() <= 65536);
        }
        frame
    })
    .await
}

#[actix_web::test]
async fn reset_origin_during_sqlite_commit_still_publishes_to_another_http_subscriber() {
    let dir = tempfile::tempdir().unwrap();
    let state = connect_to_database(&format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("disconnect.db").display()
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

    // SQLite calls this inside sqlite3_step, before the autocommit finishes.
    // All pool connections are held while hooks are installed, so whichever
    // connection SeaORM acquires must enter this deterministic barrier.
    let (entered_tx, entered_rx) = oneshot::channel();
    let entered = Arc::new(Mutex::new(Some(entered_tx)));
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release = Arc::new(Mutex::new(release_rx));
    let armed = Arc::new(AtomicBool::new(true));
    let mut connections = Vec::new();
    for _ in 0..5 {
        let mut connection = state
            .db
            .get_sqlite_connection_pool()
            .acquire()
            .await
            .unwrap();
        let entered = entered.clone();
        let release = release.clone();
        let armed = armed.clone();
        connection
            .lock_handle()
            .await
            .unwrap()
            .set_commit_hook(move || {
                if armed.swap(false, Ordering::SeqCst) {
                    let _ = entered.lock().unwrap().take().unwrap().send(());
                    let _ = release
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(10));
                }
                true // SQLx's bool means permit commit (opposite SQLite's raw C hook).
            });
        connections.push(connection);
    }
    drop(connections);

    let (ended_tx, ended_rx) = oneshot::channel();
    let ended = Arc::new(Mutex::new(Some(ended_tx)));
    let worker_state = state.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = HttpServer::new(move || {
        let ended = ended.clone();
        App::new()
            .app_data(web::Data::new(worker_state.clone()))
            .wrap_fn(move |request, service| {
                let mut lifetime = RequestLifetime {
                    ended: if request.path() == "/api/v1/channels" && request.method() == "POST" {
                        ended.lock().unwrap().take()
                    } else {
                        None
                    },
                    completed: false,
                };
                let future = service.call(request);
                async move {
                    let response = future.await;
                    lifetime.completed = true;
                    drop(lifetime);
                    response
                }
            })
            .configure(routes)
    })
    .workers(2)
    .disable_signals()
    .listen(listener)
    .unwrap()
    .run();
    let handle = server.handle();
    let server_task = actix_web::rt::spawn(server);

    let mut subscriber = bounded(TcpStream::connect(address)).await.unwrap();
    subscriber.write_all(format!("GET /api/v1/events HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\n\r\n").as_bytes()).await.unwrap();
    let mut subscriber = BufReader::new(subscriber);
    bounded(async {
        let mut status = String::new();
        subscriber.read_line(&mut status).await.unwrap();
        assert!(status.starts_with("HTTP/1.1 200"));
        loop {
            let mut line = String::new();
            assert!(subscriber.read_line(&mut line).await.unwrap() > 0);
            if line == "\r\n" {
                break;
            }
        }
    })
    .await;
    assert_eq!(
        read_frame(&mut subscriber).await,
        b"event: ready\ndata: {}\n\n"
    );

    let mut origin = bounded(TcpStream::connect(address)).await.unwrap();
    let body = r#"{"name":"Survives disconnect","type":"text"}"#;
    origin.write_all(format!("POST /api/v1/channels HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    bounded(entered_rx).await.unwrap();
    origin.set_zero_linger().unwrap(); // Real TCP RST, not just allowed HTTP/1 read-half FIN.
    drop(origin);
    let completed = bounded(ended_rx).await.unwrap();
    release_tx.send(()).unwrap();
    assert!(
        !completed,
        "Actix dropped the request future while SQLite was inside commit"
    );

    // Observe the committed entity through the ordinary API, not a private DB assertion.
    let channel = bounded(async {
        loop {
            let response = test::call_service(
                &app,
                test::TestRequest::get()
                    .uri("/api/v1/channels")
                    .insert_header(("Authorization", format!("Bearer {token}")))
                    .to_request(),
            )
            .await;
            let channels: Value = test::read_body_json(response).await;
            if let Some(channel) = channels["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|channel| channel["name"] == "Survives disconnect")
            {
                break channel.clone();
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    eprintln!(
        "origin request canceled; committed channel confirmed by GET; awaiting subscriber change"
    );
    let bytes = read_frame(&mut subscriber).await;
    let text = std::str::from_utf8(&bytes).unwrap();
    let event: Value =
        serde_json::from_str(text.strip_prefix("event: change\ndata: ").unwrap().trim()).unwrap();
    assert_eq!(event["type"], "channel_created");
    assert_eq!(event["channel"]["name"], "Survives disconnect");
    assert_eq!(event["channel"], channel);
    drop(subscriber);
    bounded(handle.stop(false)).await;
    bounded(server_task).await.unwrap().unwrap();
}
