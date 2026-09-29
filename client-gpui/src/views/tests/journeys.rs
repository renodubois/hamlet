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
impl AuthApi for RaceAuth {
    fn signup(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
        TestAuth.signup(s, u, p)
    }
    fn login(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
        TestAuth.login(s, u, p)
    }
    fn logout(&self, s: String, t: String) -> ApiFuture<Result<(), AuthError>> {
        TestAuth.logout(s, t)
    }
    fn channels(
        &self,
        s: String,
        t: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Channel>, AuthError>> {
        TestAuth.channels(s, t)
    }
    fn create_channel(
        &self,
        s: String,
        t: String,
        n: String,
    ) -> ApiFuture<Result<crate::conversation::Channel, AuthError>> {
        TestAuth.create_channel(s, t, n)
    }
    fn history(
        &self,
        _: String,
        _: String,
        _: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Message>, AuthError>> {
        Box::pin(async { unreachable!() })
    }
    fn history_page(
        &self,
        _: String,
        _: String,
        _: String,
        before: Option<String>,
    ) -> ApiFuture<Result<crate::conversation::Page, AuthError>> {
        let tx = self.pages.clone();
        Box::pin(async move {
            let (reply, rx) = async_channel::bounded(1);
            tx.send((before, reply)).unwrap();
            rx.recv().await.unwrap()
        })
    }
    fn send_message(
        &self,
        _: String,
        _: String,
        id: String,
        text: String,
    ) -> ApiFuture<Result<crate::conversation::Message, AuthError>> {
        let (tx, rx) = async_channel::bounded(1);
        self.sends.lock().unwrap().push((id, text, tx));
        Box::pin(async move { rx.recv().await.unwrap() })
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

// Bridge only the test fixture's HTTP futures onto Tokio; Hamlet still dispatches
// channels and history via its production poll_at / completion paths.
struct ServerPollingAuth {
    api: crate::http::HttpAuth,
    completed: std::sync::mpsc::Sender<&'static str>,
}
impl AuthApi for ServerPollingAuth {
    fn signup(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
        self.api.signup(s, u, p)
    }
    fn login(&self, s: String, u: String, p: String) -> ApiFuture<Result<Login, AuthError>> {
        self.api.login(s, u, p)
    }
    fn current_user(&self, s: String, t: String) -> ApiFuture<Result<User, AuthError>> {
        self.api.current_user(s, t)
    }
    fn logout(&self, s: String, t: String) -> ApiFuture<Result<(), AuthError>> {
        self.api.logout(s, t)
    }
    fn channels(
        &self,
        s: String,
        t: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Channel>, AuthError>> {
        let future = self.api.channels(s, t);
        let done = self.completed.clone();
        Box::pin(async move {
            let result = future.await;
            done.send("channels").unwrap();
            result
        })
    }
    fn create_channel(
        &self,
        s: String,
        t: String,
        n: String,
    ) -> ApiFuture<Result<crate::conversation::Channel, AuthError>> {
        self.api.create_channel(s, t, n)
    }
    fn send_message(
        &self,
        s: String,
        t: String,
        id: String,
        text: String,
    ) -> ApiFuture<Result<crate::conversation::Message, AuthError>> {
        self.api.send_message(s, t, id, text)
    }
    fn history(
        &self,
        s: String,
        t: String,
        id: String,
    ) -> ApiFuture<Result<Vec<crate::conversation::Message>, AuthError>> {
        self.api.history(s, t, id)
    }
    fn history_page(
        &self,
        s: String,
        t: String,
        id: String,
        before: Option<String>,
    ) -> ApiFuture<Result<crate::conversation::Page, AuthError>> {
        let future = self.api.history_page(s, t, id, before);
        let done = self.completed.clone();
        Box::pin(async move {
            let result = future.await;
            done.send("history").unwrap();
            result
        })
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
    let api = crate::http::HttpAuth::new();
    let alice = super::runtime()
        .block_on(api.signup(url.clone(), "Alice".into(), "long password".into()))
        .unwrap();
    let bob = super::runtime()
        .block_on(api.signup(url.clone(), "Bob".into(), "long password".into()))
        .unwrap();
    let (done, completions) = std::sync::mpsc::channel();
    cx.update(gpui_kit::init);
    let saved = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored = saved.clone();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let execution = crate::runtime::Execution::production(cx.background_executor().clone());
            Hamlet::with_dependencies(
                window,
                cx,
                Arc::new(ServerPollingAuth {
                    api: crate::http::HttpAuth::new(),
                    completed: done,
                }),
                crate::persistence::Config::default(),
                None,
                execution,
            )
        });
        *stored.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view: gpui_kit::Entity<Hamlet> = saved.borrow().as_ref().unwrap().clone();
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.session.change_server(url.clone());
            v.session.username = "Alice".into();
            v.session.password = "long password".into();
            let login = v.session.submit().unwrap();
            v.session
                .complete_login(login, Ok(alice), chrono::Utc::now().timestamp());
            v.polling.focus(true, Duration::ZERO);
            v.load_channels(cx);
        })
    });
    assert_eq!(
        completions.recv_timeout(Duration::from_secs(5)).unwrap(),
        "channels"
    );
    cx.run_until_parked();
    assert_eq!(
        completions.recv_timeout(Duration::from_secs(5)).unwrap(),
        "history"
    );
    cx.run_until_parked();
    let selected = cx.update(|_, cx| view.read(cx).conversation.selected.clone().unwrap());
    let posted = super::runtime()
        .block_on(api.send_message(
            url.clone(),
            bob.token.clone(),
            selected.clone(),
            "hello from Bob".into(),
        ))
        .unwrap();
    let channel = super::runtime()
        .block_on(api.create_channel(url.clone(), bob.token, "Bob room".into()))
        .unwrap();
    cx.update(|_, cx| view.update(cx, |v, cx| v.poll_at(Duration::from_secs(100), cx)));
    assert_eq!(
        completions.recv_timeout(Duration::from_secs(5)).unwrap(),
        "channels"
    );
    cx.run_until_parked();
    assert_eq!(
        completions.recv_timeout(Duration::from_secs(5)).unwrap(),
        "history"
    );
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert!(matches!(&v.conversation.channels, Some(crate::conversation::Load::Ready(items)) if items.contains(&channel)));
        assert!(matches!(v.conversation.history.get(&selected), Some(crate::conversation::Load::Ready(items)) if items.iter().any(|m| m.id == posted.id && m.author_name == "Bob")));
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
