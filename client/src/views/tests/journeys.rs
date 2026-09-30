use super::*;
use crate::{runtime::Execution, storage, views::app_shell::open};

struct ShortLogin(crate::runtime::Execution);
impl RequestAdapter for ShortLogin {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.url().path() == "/api/v1/auth/login" {
            let expiry = chrono::DateTime::from_timestamp(self.0.unix_seconds() + 3, 0)
                .unwrap()
                .to_rfc3339();
            let body = serde_json::json!({
                "user": {"id":"42", "username":"Ada"}, "access_token":"synthetic",
                "expires_at": expiry
            });
            return Box::pin(
                async move { Ok(Response::controlled(StatusCode::OK, body.to_string())) },
            );
        }
        bound_auth::BoundAuth.execute(request)
    }
}

#[gpui_kit::test]
fn expiry_wipes_hidden_composer_before_a_new_login(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let execution = Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open(
            window,
            cx,
            bound_auth::bound_api(ShortLogin(execution.clone())),
            storage::Config::default(),
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
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("secret draft", cx);
        window.click("channel-000000000000002", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("another secret draft", cx);
    });
    cx.background_executor.advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("composer").is_none());
        assert!(window.find("password").value().is_none_or(str::is_empty));
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    for channel in ["channel-000000000000001", "channel-000000000000002"] {
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click(channel, cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.find("composer").value().is_none_or(str::is_empty));
            window.click("composer", cx);
            window.input("fresh", cx);
            window.press("ctrl-a", cx);
            window.press("ctrl-c", cx);
            assert_eq!(
                cx.read_from_clipboard().and_then(|c| c.text()),
                Some("fresh".into())
            );
        });
    }
}

#[gpui_kit::test]
fn real_controls_read_selected_conversation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(window, cx, Arc::new(BoundAuth));
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
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("channel-000000000000001")
                .label()
                .unwrap()
                .contains("alpha")
        );
        assert_eq!(
            window.find("message-000000000000001").label(),
            Some("first line\nsecond line")
        );
    });
    cx.update(|window, cx| {
        window.click("channel-000000000000002", cx);
        window.render_frame(cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("message-000000000000002").bounds().size.height > px(0.));
        window.click("logout", cx);
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Log in"));
    });
}

#[gpui_kit::test]
fn polling_and_send_confirmation_share_one_headless_history_without_duplicate(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (pages_tx, pages) = std::sync::mpsc::channel();
    let sends = Arc::new(Mutex::new(Vec::<Sent>::new()));
    let captured = sends.clone();

    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(
            window,
            cx,
            Arc::new(RaceAuth {
                pages: pages_tx,
                sends: captured,
            }),
        );

        Root::new(view, window, cx)
    });

    cx.deactivate_window();
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
    });
    cx.run_until_parked();
    let (_, initial) = pages.recv_timeout(Duration::from_secs(2)).unwrap();
    let message = |id: &str| crate::api::Message {
        id: id.into(),
        channel_id: "000000000000001".into(),
        author_id: "42".into(),
        author_name: "Ada".into(),
        text: "same".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    let page = |ids: &[&str]| crate::api::Page {
        items: ids.iter().map(|id| message(id)).collect(),
        next_cursor: None,
    };
    initial.send_blocking(Ok(page(&["8"]))).unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("composer", cx);
        window.input("same", cx);
        window.click("send-message", cx);
    });
    cx.run_until_parked();
    assert_eq!(sends.lock().unwrap().len(), 1);
    advance(cx, 1);
    cx.run_until_parked();
    let (_, poll) = pages.recv_timeout(Duration::from_secs(2)).unwrap();
    poll.send_blocking(Ok(page(&["10", "9", "8"]))).unwrap();
    cx.run_until_parked();
    let (_, text, confirmation) = sends.lock().unwrap().remove(0);
    assert_eq!(text, "same");
    confirmation.try_send(Ok(message("10"))).unwrap();
    cx.run_until_parked();
    let (_, read) = pages.recv_timeout(Duration::from_secs(2)).unwrap();
    read.send_blocking(Ok(page(&["10", "9", "8"]))).unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("message-10").label(), Some("same"));
        assert_eq!(composer_text(window, cx), "");
        assert_eq!(window.find("message-9").label(), Some("same"));
        assert_eq!(window.find("message-8").label(), Some("same"));
        assert!(sends.lock().unwrap().is_empty());
        assert!(pages.try_recv().is_err());
    });
}

// Wait for real HTTP/foreign-thread progress, observing only the stable control surface.
fn await_control(cx: &mut gpui_kit::VisualTestContext, id: String, label: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if cx.update(|window, cx| {
            window.render_frame(cx);
            window
                .try_find(id.clone())
                .is_some_and(|control| control.label() == Some(label))
        }) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "loopback activity did not reach {id}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[gpui_kit::test]
fn bob_activity_arrives_through_scheduled_polling_and_real_server_routes(cx: &mut TestAppContext) {
    cx.background_executor.allow_parking(); // real loopback I/O on the production executor
    use actix_web::{App, HttpServer, web};
    use std::net::TcpListener;
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let server_thread = std::thread::spawn(move || {
        actix_web::rt::System::new().block_on(async move {
            let dir = tempfile::tempdir().unwrap();
            let db = hamlet::connect_to_database(&format!(
                "sqlite://{}?mode=rwc",
                dir.path().join("poll-at.db").display()
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
            ready_tx.send((url, server.handle())).unwrap();
            server.await.unwrap();
        });
    });
    let (url, server_handle) = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let api = HttpTransport::new().server(&url).unwrap();
    let bob = super::runtime()
        .block_on(api.signup("Bob".into(), "long password".into()))
        .unwrap();
    super::runtime()
        .block_on(api.signup("Alice".into(), "long password".into()))
        .unwrap();
    let selected = super::runtime().block_on(bob.client.channels()).unwrap()[0]
        .id
        .clone();
    let initial = super::runtime()
        .block_on(
            bob.client
                .send_message(selected.clone(), "already here".into()),
        )
        .unwrap();
    cx.update(gpui_kit::init);
    let execution = crate::runtime::Execution::production(cx.background_executor.clone());
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = crate::views::app_shell::open(
            window,
            cx,
            HttpTransport::new(),
            crate::storage::Config {
                server: Some(url),
                ..Default::default()
            },
            None,
            execution,
        );
        Root::new(view, window, cx)
    });
    cx.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click("username", cx);
        window.input("Alice", cx);
        window.click("password", cx);
        window.input("long password", cx);
        window.click("login", cx);
    });
    await_control(cx, format!("message-{}", initial.id), "already here");
    let posted = super::runtime()
        .block_on(
            bob.client
                .send_message(selected.clone(), "hello from Bob".into()),
        )
        .unwrap();
    let channel = super::runtime()
        .block_on(bob.client.create_channel("Bob room".into()))
        .unwrap();
    // The actual lifecycle timer drives polling; no private state or alternate HTTP wrapper.
    cx.background_executor
        .advance_clock(Duration::from_secs(15));
    await_control(cx, format!("channel-{}", channel.id), "# Bob room");
    await_control(cx, format!("message-{}", posted.id), "hello from Bob");
    cx.update(|window, cx| {
        window.click("logout", cx);
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Log in"));
    });
    super::runtime().block_on(server_handle.stop(true));
    server_thread.join().unwrap();
}

#[gpui_kit::test]
fn real_controls_login_and_logout(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = open_controlled(window, cx, Arc::new(BoundAuth));
        Root::new(view, window, cx)
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("server-url").value(),
            Some("http://127.0.0.1:8081")
        );
        window.click("username", cx);
        window.input("Ada", cx);
        window.click("password", cx);
        window.input("pass", cx);
        window.click("login", cx);
        window.render_frame(cx);
        assert_eq!(window.find("login").label(), Some("Signing in…"));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("session-status")
                .label()
                .unwrap()
                .contains("Ada")
        );
        window.click("logout", cx);
        window.render_frame(cx);
        assert!(window.find("password").value().is_none_or(str::is_empty));
        assert_eq!(window.find("login").label(), Some("Log in"));
    });
}
