//! Full real-control/authenticated-route journey, retained storage and live removal.
use super::*;
use crate::{
    chat::{ChatHandle, Load},
    runtime::Execution,
};
use sea_orm::{ConnectionTrait, DbBackend, Statement};

struct Server {
    handle: actix_web::dev::ServerHandle,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Server {
    fn drop(&mut self) {
        runtime().block_on(self.handle.stop(false));
        self.thread.take().unwrap().join().unwrap();
    }
}
struct DeleteTransport {
    client: reqwest::Client,
    calls: Arc<Mutex<Vec<String>>>,
    reads: Arc<AtomicUsize>,
    release: async_channel::Receiver<()>,
}
impl RequestAdapter for DeleteTransport {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        if request.method() == reqwest::Method::GET {
            self.reads.fetch_add(1, Ordering::SeqCst);
        }
        let deleting = request.method() == reqwest::Method::DELETE;
        if deleting {
            assert!(request.headers().contains_key("authorization"));
            assert!(request.body().is_none());
            self.calls
                .lock()
                .unwrap()
                .push(request.url().path().to_owned());
        }
        let result = RequestAdapter::execute(&self.client, request);
        let release = self.release.clone();
        Box::pin(async move {
            let response = result.await?;
            if deleting {
                release.recv().await.unwrap();
            }
            Ok(response)
        })
    }
}
fn converge(
    cx: &mut gpui_kit::VisualTestContext,
    observer: &ChatHandle,
    ready: impl Fn(&mut gpui_kit::Window, &mut gpui_kit::App) -> bool,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        while let Ok(update) = observer.updates().try_recv() {
            assert!(observer.apply(update).is_none());
        }
        if cx.update(|window, cx| {
            window.render_frame(cx);
            ready(window, cx)
        }) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "bounded authenticated route convergence"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn row(db: &sea_orm::DatabaseConnection, id: &str) -> (String, i64, i64, String, Option<String>) {
    let row = runtime()
        .block_on(db.query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT text, author_id, channel_id, created_at, deleted_at FROM messages WHERE id = ?",
            [id.parse::<i64>().unwrap().into()],
        )))
        .unwrap()
        .unwrap();
    (
        row.try_get("", "text").unwrap(),
        row.try_get("", "author_id").unwrap(),
        row.try_get("", "channel_id").unwrap(),
        row.try_get("", "created_at").unwrap(),
        row.try_get("", "deleted_at").unwrap(),
    )
}

#[gpui_kit::test]
fn authenticated_context_menu_confirmation_delete_retains_row_and_removes_both_live_chats(
    cx: &mut TestAppContext,
) {
    authenticated_message_delete_journey(cx, false);
}

#[gpui_kit::test]
fn authenticated_trash_tray_confirmation_removes_both_live_chats_without_moving_reader(
    cx: &mut TestAppContext,
) {
    authenticated_message_delete_journey(cx, true);
}

fn authenticated_message_delete_journey(cx: &mut TestAppContext, tray: bool) {
    cx.background_executor.allow_parking();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || {
        actix_web::rt::System::new().block_on(async move {
            let dir = tempfile::tempdir().unwrap();
            let state = hamlet::connect_to_database(&format!(
                "sqlite://{}?mode=rwc",
                dir.path().join("message-control.db").display()
            ))
            .await
            .unwrap();
            let db = state.db.clone();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let server = actix_web::HttpServer::new(move || {
                actix_web::App::new()
                    .app_data(actix_web::web::Data::new(state.clone()))
                    .configure(hamlet::routes)
            })
            .workers(2)
            .disable_signals()
            .listen(listener)
            .unwrap()
            .run();
            send.send((url, db, server.handle())).unwrap();
            server.await.unwrap();
        })
    });
    let (url, db, handle) = receive.recv_timeout(Duration::from_secs(5)).unwrap();
    let _server = Server {
        handle,
        thread: Some(thread),
    };
    let api = HttpTransport::new().server(&url).unwrap();
    let alice = runtime()
        .block_on(api.signup("Alice".into(), "long password".into()))
        .unwrap();
    let bob = runtime()
        .block_on(api.signup("Bob".into(), "long password".into()))
        .unwrap();
    let channel = runtime().block_on(alice.client.channels()).unwrap()[0]
        .id
        .clone();
    let mut surrounding = Vec::new();
    if tray {
        for ix in 0..10 {
            surrounding.push(
                runtime()
                    .block_on(
                        alice
                            .client
                            .send_message(channel.clone(), format!("older {ix}\nsecond line")),
                    )
                    .unwrap()
                    .id,
            );
        }
    }
    let message = runtime()
        .block_on(
            alice
                .client
                .send_message(channel.clone(), "retained original\nmessage".into()),
        )
        .unwrap();
    if tray {
        for ix in 0..30 {
            surrounding.push(
                runtime()
                    .block_on(
                        alice
                            .client
                            .send_message(channel.clone(), format!("newer {ix}\nsecond line")),
                    )
                    .unwrap()
                    .id,
            );
        }
    }
    let before = row(&db, &message.id);
    assert_eq!(before.0, "retained original\nmessage");
    assert!(before.4.is_none());
    let execution = Execution::production(cx.background_executor.clone());
    let observer = ChatHandle::new(
        90,
        bob.user.id,
        bob.expires_at,
        bob.client,
        execution.clone(),
    );
    observer.start();
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let reads = Arc::new(AtomicUsize::new(0));
    let (release, held) = async_channel::bounded(1);
    let config_dir = tempfile::tempdir().unwrap();
    let credentials = crate::storage::Persistence::start(
        crate::test_support::storage::Controlled(Default::default()),
        Some(config_dir.path().join("session.json")),
    );
    let (_, cx) = cx.add_window_view(|window, cx| {
        let transport = HttpTransport::with_adapter(Arc::new(DeleteTransport {
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
            calls: calls.clone(),
            reads: reads.clone(),
            release: held,
        }));
        let view = crate::views::app_shell::open(
            window,
            cx,
            transport,
            crate::storage::Config {
                server: Some(url),
                ..Default::default()
            },
            Some(credentials),
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
    let target = format!("message-{}", message.id);
    let initially_visible = if tray {
        format!("message-{}", surrounding.last().unwrap())
    } else {
        target.clone()
    };
    converge(cx, &observer, |window, _| {
        window.try_find(initially_visible.clone()).is_some()
            && matches!(observer.read().history.get(&channel), Some(Load::Ready(rows)) if rows.iter().any(|m| m.id == message.id))
    });
    let reader = tray.then(|| {
        cx.update(|window, cx| {
            for _ in 0..35 {
                window.render_frame(cx);
                let pane = window.find("history").bounds();
                if window.try_find(target.clone()).is_some_and(|row| {
                    row.bounds().top() >= pane.top() + px(64.)
                        && row.bounds().bottom() <= pane.bottom()
                }) {
                    break;
                }
                window.scroll(
                    "history",
                    gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(83.))),
                    cx,
                );
            }
            window.render_frame(cx);
            let pane = window.find("history").bounds();
            let row = window.find(target.clone()).bounds();
            assert!(row.top() >= pane.top() + px(64.) && row.bottom() <= pane.bottom());
            assert!(window.try_find("jump-latest").is_some());
            let anchor = surrounding
                .iter()
                .find_map(|id| {
                    let name = format!("message-{id}");
                    let row = window.try_find(name.clone())?.bounds();
                    (row.top() <= pane.top() && row.bottom() > pane.top())
                        .then_some((name, row.top()))
                })
                .expect("surviving reader anchor before the target");
            (anchor, pane)
        })
    });
    let baseline_reads = reads.load(Ordering::SeqCst);
    let open = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|window, cx| {
            window.render_frame(cx);
            if tray {
                window.hover(target.clone(), cx);
                window.click(format!("trash-message-{}", message.id), cx);
            } else {
                window.right_click(target.clone(), cx);
            }
        });
        cx.run_until_parked();
        if !tray {
            cx.update(|window, cx| {
                window.render_frame(cx);
                window.click("delete-message-label", cx);
            });
            cx.run_until_parked();
        }
    };
    open(cx);
    assert!(calls.lock().unwrap().is_empty());
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("cancel-message-action", cx);
    });
    cx.run_until_parked();
    assert!(calls.lock().unwrap().is_empty());
    open(cx);
    assert!(calls.lock().unwrap().is_empty());
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("cancel-message-action").focused(), Some(true));
        assert_eq!(
            window.find("delete-message-preview").label(),
            Some("retained original\nmessage")
        );
        window.press("tab", cx);
        window.press("enter", cx);
    });
    // The actual 204 is held at the transport seam: ordered SSE must close the
    // pending confirmation and remove both Chats before HTTP is delivered.
    converge(cx, &observer, |window, _| {
        window.try_find(target.clone()).is_none()
            && window.try_find("delete-message-preview").is_none()
            && matches!(observer.read().history.get(&channel), Some(Load::Ready(rows)) if rows.iter().all(|m| m.id != message.id))
    });
    assert_eq!(
        *calls.lock().unwrap(),
        [format!(
            "/api/v1/channels/{channel}/messages/{}",
            message.id
        )]
    );
    assert_eq!(
        reads.load(Ordering::SeqCst),
        baseline_reads,
        "deletion is not a history read"
    );
    if let Some(((anchor, top), pane)) = &reader {
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find(anchor.clone()).bounds().top(), *top);
            assert_eq!(window.find("history").bounds(), *pane);
            assert!(window.try_find("jump-latest").is_some());
        });
    }
    let after = row(&db, &message.id);
    assert_eq!(
        (&after.0, after.1, after.2, &after.3),
        (&before.0, before.1, before.2, &before.3)
    );
    assert!(after.4.is_some());
    release.try_send(()).unwrap();
    cx.run_until_parked();
    assert!(
        runtime()
            .block_on(alice.client.history_page(channel, None))
            .unwrap()
            .items
            .iter()
            .all(|m| m.id != message.id)
    );
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(target.clone()).is_none());
        assert!(window.try_find("delete-message-preview").is_none());
        if let Some(((anchor, top), pane)) = &reader {
            assert_eq!(window.find(anchor.clone()).bounds().top(), *top);
            assert_eq!(window.find("history").bounds(), *pane);
            assert!(window.try_find("jump-latest").is_some());
        }
        window.click("logout", cx);
    });
    observer.close();
}
