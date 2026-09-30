//! Session/storage lifecycle through production execution and loopback server routes.
use super::*;
use crate::runtime::runtime;
use crate::storage;
use crate::test_support::storage::Controlled;
use gpui_kit::TestAppContext;
use std::sync::{Arc, Condvar, Mutex};

fn await_session(
    cx: &mut TestAppContext,
    session: &mut SessionCoordinator,
    ready: impl Fn(&SessionCoordinator) -> bool,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.executor().run_until_parked();
        while let Ok(update) = session.updates().try_recv() {
            session.apply(update);
        }
        if ready(session) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "session workflow did not complete"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[gpui_kit::test]
fn signup_uses_shared_persistent_session_and_verified_restore_against_server_routes(
    cx: &mut TestAppContext,
) {
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let database = dir.path().join("signup-restore.db");
    let (ready, server) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || {
        actix_web::rt::System::new().block_on(async move {
            use actix_web::{App, HttpServer, web};
            let db =
                hamlet::connect_to_database(&format!("sqlite://{}?mode=rwc", database.display()))
                    .await
                    .unwrap();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let server = HttpServer::new(move || {
                App::new()
                    .app_data(web::Data::new(db.clone()))
                    .configure(hamlet::routes)
            })
            .listen(listener)
            .unwrap()
            .run();
            ready.send((url, server.handle())).unwrap();
            server.await.unwrap();
        });
    });
    let (url, server) = server.recv_timeout(Duration::from_secs(5)).unwrap();
    let shared = Arc::new((
        Mutex::new((vec![], false, false, false, false, false)),
        Condvar::new(),
    ));
    let store = Persistence::start(Controlled(shared.clone()), Some(path.clone()));
    let execution = Execution::production(cx.background_executor.clone());
    let mut session = SessionCoordinator::new(
        HttpTransport::new(),
        execution.clone(),
        Config {
            server: Some(url.clone()),
            ..Default::default()
        },
        Some(store),
    );
    session.submit("Alice".into(), "long password".into(), true);
    await_session(cx, &mut session, |session| {
        session.active().is_some()
            && session.storage_feedback().as_deref()
                == Some("Login saved in Secret Service for this server and user.")
    });
    let selected = storage::load_at(Some(&path)).saved.unwrap();
    assert_eq!(session.active().unwrap().user, selected.user);
    let old_client = session.active().unwrap().client();
    let metadata = std::fs::read_to_string(&path).unwrap();
    assert!(!metadata.contains(old_client.credential_for_session()));
    assert!(!metadata.contains("long password"));
    drop(session);

    // Startup owns reading and verification: no manual finish_restore/save/delete dispatch.
    let mut restarted = SessionCoordinator::new(
        HttpTransport::new(),
        execution,
        storage::load_at(Some(&path)),
        Some(Persistence::start(
            Controlled(shared.clone()),
            Some(path.clone()),
        )),
    );
    assert!(restarted.active().is_none());
    assert!(restarted.conversation().is_none());
    await_session(cx, &mut restarted, |session| session.active().is_some());
    assert_eq!(restarted.active().unwrap().user, selected.user);
    assert_eq!(
        restarted.storage_feedback().as_deref(),
        Some("Login restored from Secret Service.")
    );
    assert!(restarted.conversation().is_some());
    restarted.logout();
    assert!(restarted.active().is_none());
    assert!(restarted.conversation().is_none());
    await_session(cx, &mut restarted, |session| {
        session.storage_feedback().as_deref() == Some("Saved login removed from Secret Service.")
    });
    assert!(storage::load_at(Some(&path)).saved.is_none());
    assert!(shared.0.lock().unwrap().0.is_empty());
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while runtime().block_on(old_client.current_user()) != Err(ApiError::AlreadyInvalid) {
        assert!(
            std::time::Instant::now() < deadline,
            "revocation did not complete"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    await_session(cx, &mut restarted, |session| session.feedback().is_none());
    runtime().block_on(server.stop(true));
    thread.join().unwrap();
}
