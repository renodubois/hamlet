#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use hamlet_client_iced::avatar::{AvatarImageError, fetch_avatar_image};

#[test]
fn fetch_avatar_image_downloads_successful_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(ResponseSpec::bytes(200, b"fake-webp".to_vec()))?;
    let url = format!("{}/uploads/avatars/42.webp?v=1", server.base_url());

    let (returned_url, result) = fetch_avatar_image(url.clone());

    assert_eq!(returned_url, url);
    assert_eq!(result?, b"fake-webp".to_vec());
    assert_eq!(server.path(), "/uploads/avatars/42.webp?v=1");

    Ok(())
}

#[test]
fn fetch_avatar_image_reports_http_failures() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(ResponseSpec::bytes(404, Vec::new()))?;
    let url = format!("{}/uploads/avatars/missing.webp", server.base_url());

    let (_, result) = fetch_avatar_image(url);

    assert_eq!(result, Err(AvatarImageError::Server { status: 404 }));

    Ok(())
}

#[derive(Debug, Clone)]
struct ResponseSpec {
    status: u16,
    body: Vec<u8>,
}

impl ResponseSpec {
    fn bytes(status: u16, body: Vec<u8>) -> Self {
        Self { status, body }
    }
}

struct TestServer {
    base_url: String,
    request_path: Arc<Mutex<Option<String>>>,
    handle: Option<JoinHandle<()>>,
}

impl TestServer {
    fn start(response: ResponseSpec) -> Result<Self, Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let base_url = format!("http://{}", listener.local_addr()?);
        let request_path = Arc::new(Mutex::new(None));
        let thread_request_path = Arc::clone(&request_path);
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("test server accepts request");
            let path = read_path(&mut stream);
            *thread_request_path.lock().unwrap() = Some(path);
            write_response(&mut stream, &response);
        });

        Ok(Self {
            base_url,
            request_path,
            handle: Some(handle),
        })
    }

    fn base_url(&self) -> String {
        self.base_url.clone()
    }

    fn path(&self) -> String {
        if let Some(handle) = &self.handle {
            while !handle.is_finished() {
                thread::yield_now();
            }
        }

        self.request_path
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_default()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().expect("test server thread should finish");
        }
    }
}

fn read_path(stream: &mut TcpStream) -> String {
    let mut reader = BufReader::new(stream.try_clone().expect("clone test stream"));
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .expect("read request line");
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();

    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read header line");
        if line.trim_end_matches(['\r', '\n']).is_empty() {
            break;
        }
    }

    path
}

fn write_response(stream: &mut TcpStream, response: &ResponseSpec) {
    let reason = match response.status {
        200 => "OK",
        404 => "Not Found",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        reason,
        response.body.len()
    );

    stream.write_all(head.as_bytes()).expect("write head");
    stream.write_all(&response.body).expect("write body");
}
