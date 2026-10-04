//! Actual reqwest adapter and actual server routes; finite reads only.
use super::*;
use crate::api::HttpTransport;
use gpui_kit::TestAppContext;
use std::{
    io::{Read, Write},
    net::TcpListener,
};

async fn delivery(
    cx: &mut TestAppContext,
    stream: &mut EventStream,
) -> Result<LiveEvent, StreamError> {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.executor().run_until_parked();
        if let Poll::Ready(result) = std::pin::pin!(stream.next())
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return result;
        }
        assert!(Instant::now() < deadline, "finite delivery");
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

#[gpui_kit::test]
fn real_server_delivers_both_creations_to_two_authenticated_consumers_without_reads(
    cx: &mut TestAppContext,
) {
    actix_web::rt::System::new().block_on(async {
        use actix_web::{App, HttpServer, web};
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        let dir = tempfile::tempdir().unwrap();
        let state = hamlet::connect_to_database(&format!("sqlite://{}?mode=rwc", dir.path().join("events.db").display())).await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new({ let state = state.clone(); move || {
            App::new().app_data(web::Data::new(state.clone())).configure(hamlet::routes)
        }}).workers(2).listen(listener).unwrap().run();
        let handle = server.handle();
        actix_web::rt::spawn(server);
        let api = HttpTransport::new().server(&url).unwrap();
        let alice = api.signup("Alice".into(), "long password".into()).await.unwrap();
        let bob = api.signup("Bob".into(), "long password".into()).await.unwrap();
        let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
        let mut a = alice.client.events(&execution);
        let mut b = bob.client.events(&execution);
        assert_eq!(delivery(cx, &mut a).await, Ok(LiveEvent::Ready));
        assert_eq!(delivery(cx, &mut b).await, Ok(LiveEvent::Ready));
        assert_eq!(alice.client.create_channel("bad!".into()).await, Err(ApiError::InvalidInput));
        let channel = bob.client.create_channel("A room".into()).await.unwrap();
        for stream in [&mut a, &mut b] {
            assert_eq!(delivery(cx, stream).await, Ok(LiveEvent::ChannelCreated(channel.clone())));
        }
        assert_eq!(bob.client.create_channel("a ROOM".into()).await, Err(ApiError::Conflict));
        assert_eq!(alice.client.send_message(channel.id.clone(), " ".into()).await, Err(ApiError::InvalidInput));
        state.db.execute_raw(Statement::from_string(DbBackend::Sqlite,
            "CREATE TRIGGER reject_message BEFORE INSERT ON messages BEGIN SELECT RAISE(ABORT, 'fixture'); END".to_owned()
        )).await.unwrap();
        assert_eq!(bob.client.send_message(channel.id.clone(), "not committed".into()).await, Err(ApiError::ServerFailure));
        state.db.execute_raw(Statement::from_string(DbBackend::Sqlite, "DROP TRIGGER reject_message".to_owned())).await.unwrap();
        let text = format!("雪\n\"\\{}", "\u{0001}".repeat(3996));
        assert_eq!(text.chars().count(), 4000);
        let message = bob.client.send_message(channel.id.clone(), text.clone()).await.unwrap();
        assert_eq!(message.text, text);
        assert_eq!(message.author_id, bob.user.id);
        for stream in [&mut a, &mut b] {
            assert_eq!(delivery(cx, stream).await, Ok(LiveEvent::MessageCreated(message.clone())));
            for _ in 0..10 {
                cx.executor().run_until_parked();
                assert!(tokio::time::timeout(Duration::from_millis(10), stream.next()).await.is_err(), "failed writes emitted a phantom event");
            }
        }
        // A revoked credential is authoritative at a new handshake, not inferred from EOF.
        bob.client.logout().await.unwrap();
        let mut rejected = bob.client.events(&execution);
        assert_eq!(delivery(cx, &mut rejected).await, Err(StreamError::Api(ApiError::AlreadyInvalid)));
        drop((a, b, rejected));
        cx.executor().run_until_parked();
        handle.stop(false).await;
    });
}

#[gpui_kit::test]
fn real_stream_redirect_never_forwards_the_bound_bearer(cx: &mut TestAppContext) {
    actix_web::rt::System::new().block_on(async {
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        destination.set_nonblocking(true).unwrap();
        let (origin, captured) = super::super::http_support::server(format!(
            "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", destination.local_addr().unwrap()
        ));
        let client = HttpTransport::new().server(&origin).unwrap().restore_candidate("synthetic-bound".into()).unwrap();
        let mut stream = client.events(&Execution::controlled(cx.background_executor.clone(), 1_800_000_000));
        assert_eq!(delivery(cx, &mut stream).await, Err(StreamError::Api(ApiError::Unavailable)));
        let request = captured.join().unwrap();
        assert!(request.starts_with("GET /api/v1/events "));
        assert!(request.to_ascii_lowercase().contains("authorization: bearer synthetic-bound"));
        assert!(destination.accept().is_err());
    });
}

#[tokio::test]
async fn ordinary_http_still_has_an_eight_second_total_body_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        assert!(read_headers(&mut socket).starts_with("GET /api/v1/me "));
        socket
            .set_read_timeout(Some(Duration::from_secs(12)))
            .unwrap();
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{").unwrap();
        let mut byte = [0];
        assert_eq!(socket.read(&mut byte).unwrap(), 0);
    });
    let client = HttpTransport::new()
        .server(&origin)
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    let start = Instant::now();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(11), client.current_user())
            .await
            .unwrap(),
        Err(ApiError::InvalidResponse)
    );
    assert!(start.elapsed() >= Duration::from_millis(7500));
    tokio::task::spawn_blocking(move || server.join().unwrap())
        .await
        .unwrap();
}

fn read_headers(stream: &mut std::net::TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") {
        assert!(bytes.len() < 8192);
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
    }
    String::from_utf8(bytes).unwrap()
}
fn send_frame(stream: &mut std::net::TcpStream, frame: &[u8]) {
    write!(stream, "{:x}\r\n", frame.len()).unwrap();
    stream.write_all(frame).unwrap();
    stream.write_all(b"\r\n").unwrap();
    stream.flush().unwrap();
}

#[gpui_kit::test]
fn production_stream_has_no_eight_second_total_timeout_and_drop_closes_tcp_body(
    cx: &mut TestAppContext,
) {
    actix_web::rt::System::new().block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (release, receive) = std::sync::mpsc::channel();
        let (closed, closure) = async_channel::bounded(1);
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            assert!(read_headers(&mut socket).starts_with("GET /api/v1/events "));
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
            send_frame(&mut socket, b"event: ready\ndata: {}\n\n");
            receive.recv_timeout(Duration::from_secs(12)).unwrap();
            send_frame(&mut socket, b"event: change\ndata: {\"type\":\"channel_created\",\"channel\":{\"id\":\"123\",\"name\":\"late\",\"type\":\"text\"}}\n\n");
            let mut byte = [0];
            // Actual reqwest body cancellation must close, not retain the live socket.
            let ended = matches!(socket.read(&mut byte), Ok(0)) ;
            closed.try_send(ended).unwrap();
        });
        let client = HttpTransport::new().server(&origin).unwrap().restore_candidate("synthetic".into()).unwrap();
        let mut stream = client.events(&Execution::controlled(cx.background_executor.clone(), 1_800_000_000));
        assert_eq!(delivery(cx, &mut stream).await, Ok(LiveEvent::Ready));
        tokio::time::sleep(Duration::from_millis(8200)).await;
        release.send(()).unwrap();
        assert!(matches!(delivery(cx, &mut stream).await, Ok(LiveEvent::ChannelCreated(channel)) if channel.name == "late"));
        drop(stream);
        cx.executor().run_until_parked();
        assert!(tokio::time::timeout(Duration::from_secs(5), closure.recv()).await.unwrap().unwrap());
        server.join().unwrap();
    });
}
