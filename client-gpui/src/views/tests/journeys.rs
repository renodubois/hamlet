use super::*;

#[gpui_kit::test]
fn expiry_wipes_hidden_composer_before_a_new_login(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let stored = std::rc::Rc::new(std::cell::RefCell::new(None));
    let saved = stored.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
        *saved.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = stored.borrow().as_ref().unwrap().clone();
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
        view.update(cx, |view, _| {
            view.session.expire(i64::MAX);
            view.conversation.clear();
        });
        window.render_frame(cx);
        assert!(view.read(cx).session.active.is_none());
        assert!(view.read(cx).conversation.drafts.is_empty());
        assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "");
    });
}

#[gpui_kit::test]
fn real_controls_read_selected_conversation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
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

struct RaceAuth {
    pages: std::sync::mpsc::Sender<(Option<String>, PageReply)>,
    sends: Arc<Mutex<Vec<Sent>>>,
}
impl RequestAdapter for RaceAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, AuthError>> {
        if request.method() == reqwest::Method::POST {
            SendAuth(self.sends.clone()).execute(request)
        } else {
            PagedAuth(self.pages.clone()).execute(request)
        }
    }
}

#[gpui_kit::test]
fn polling_and_send_confirmation_share_one_headless_history_without_duplicate(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (pages_tx, pages) = std::sync::mpsc::channel();
    let sends = Arc::new(Mutex::new(Vec::<Sent>::new()));
    let captured = sends.clone();
    let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored = saved.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            Hamlet::new(
                window,
                cx,
                Arc::new(RaceAuth {
                    pages: pages_tx,
                    sends: captured,
                }),
            )
        });
        *stored.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
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
    let message = |id: &str| crate::conversation::Message {
        id: id.into(),
        channel_id: "000000000000001".into(),
        author_id: "42".into(),
        author_name: "Ada".into(),
        text: "same".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    let page = |ids: &[&str]| crate::conversation::Page {
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
    cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(4), cx)));
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
        assert_eq!(view.read(cx).composer.read(cx).text().to_string(), "");
        assert!(
            matches!(view.read(cx).conversation.history.get("000000000000001"),
            Some(crate::conversation::Load::Ready(items)) if items.len() == 3)
        );
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
fn bob_activity_arrives_through_hamlet_poll_at_and_real_rewrite_routes(cx: &mut TestAppContext) {
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
            crate::persistence::Config {
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
    // The actual lifecycle timer drives poll_at; no private state or alternate HTTP wrapper.
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
    let (_, cx) = cx.add_window_view(|window, cx| Hamlet::new(window, cx, Arc::new(TestAuth)));
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
