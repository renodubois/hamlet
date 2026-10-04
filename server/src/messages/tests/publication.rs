use super::*;
use crate::{AppState, connect_to_database, routes};
use actix_web::{
    App,
    body::{BoxBody, MessageBody},
    http::StatusCode,
    test, web,
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

// Owner-local clock/randomness/lifecycle control, scoped to a request and its owned task; never
// compiled into the production server or exposed as HTTP configuration.
#[derive(Clone, Default)]
pub(super) struct Control {
    ids: Arc<Mutex<VecDeque<i64>>>,
    created_at: Option<DateTime<Utc>>,
    panic_after_insert: bool,
}
tokio::task_local! { pub(super) static CONTROL: Control; }

pub(super) fn next_id() -> i64 {
    CONTROL
        .try_with(|control| control.ids.lock().unwrap().pop_front())
        .ok()
        .flatten()
        .unwrap_or_else(new_id)
}

pub(super) fn now() -> DateTime<Utc> {
    CONTROL
        .try_with(|control| control.created_at)
        .ok()
        .flatten()
        .unwrap_or_else(Utc::now)
}

pub(super) fn after_insert() {
    if CONTROL
        .try_with(|control| control.panic_after_insert)
        .unwrap_or(false)
    {
        panic!("injected unexpected post-commit failure");
    }
}

async fn fixture() -> (AppState, String, String, UserIdentity) {
    let state = connect_to_database("sqlite::memory:").await.unwrap();
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
    let bearer = format!("Bearer {}", auth["access_token"].as_str().unwrap());
    let author = UserIdentity {
        id: auth["user"]["id"].as_str().unwrap().parse().unwrap(),
        username: "Alice".into(),
    };
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearer.clone()))
            .to_request(),
    )
    .await;
    let channels: Value = test::read_body_json(response).await;
    let path = format!(
        "/api/v1/channels/{}/messages",
        channels["items"][0]["id"].as_str().unwrap()
    );
    (state, bearer, path, author)
}

async fn frame(body: &mut BoxBody) -> Option<web::Bytes> {
    tokio::time::timeout(
        Duration::from_secs(2),
        std::future::poll_fn(|cx| Pin::new(&mut *body).poll_next(cx)),
    )
    .await
    .expect("bounded subscription result")
    .map(Result::unwrap)
}

#[actix_web::test]
async fn id_collisions_retry_without_phantoms_and_exhaustion_keeps_delivery_usable() {
    let (state, bearer, path, _) = fixture().await;
    let app =
        test::init_service(App::new().app_data(web::Data::new(state)).configure(routes)).await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header(("Authorization", bearer.clone()))
            .to_request(),
    )
    .await;
    let mut body = response.into_body();
    frame(&mut body).await.unwrap();
    let mut confirmed = Vec::new();
    for (text, ids, expected_id) in [
        ("First", vec![100_000_000_000_111], "100000000000111"),
        (
            "Retried",
            vec![
                100_000_000_000_111,
                100_000_000_000_111,
                100_000_000_000_222,
            ],
            "100000000000222",
        ),
        ("Exhausted", vec![100_000_000_000_222; 5], ""),
        ("Recovered", vec![100_000_000_000_333], "100000000000333"),
    ] {
        let response = CONTROL
            .scope(
                Control {
                    ids: Arc::new(Mutex::new(ids.into())),
                    ..Default::default()
                },
                test::call_service(
                    &app,
                    test::TestRequest::post()
                        .uri(&path)
                        .insert_header(("Authorization", bearer.clone()))
                        .set_json(json!({"text":text}))
                        .to_request(),
                ),
            )
            .await;
        if expected_id.is_empty() {
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
            continue;
        }
        assert_eq!(response.status(), StatusCode::CREATED);
        let message: Value = test::read_body_json(response).await;
        assert_eq!(message["id"], expected_id);
        let bytes = frame(&mut body).await.unwrap();
        let event: Value = serde_json::from_str(
            std::str::from_utf8(&bytes)
                .unwrap()
                .strip_prefix("event: change\ndata: ")
                .unwrap()
                .trim(),
        )
        .unwrap();
        assert_eq!(event, json!({"type":"message_created", "message":message}));
        confirmed.push(message);
    }
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            std::future::poll_fn(|cx| Pin::new(&mut body).poll_next(cx))
        )
        .await
        .is_err(),
        "no retry/exhaustion duplicates or delivery termination"
    );
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&path)
            .insert_header(("Authorization", bearer))
            .to_request(),
    )
    .await;
    let history: Value = test::read_body_json(response).await;
    confirmed.reverse();
    assert_eq!(history["items"], json!(confirmed));
}

#[actix_web::test]
async fn aborted_owned_write_closes_delivery_before_sqlite_finishes_its_late_commit() {
    let (state, bearer, path, author) = fixture().await;
    let channel_id = path
        .strip_prefix("/api/v1/channels/")
        .unwrap()
        .strip_suffix("/messages")
        .unwrap()
        .parse()
        .unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(routes),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header(("Authorization", bearer.clone()))
            .to_request(),
    )
    .await;
    let mut body = response.into_body();
    frame(&mut body).await.unwrap();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let mut connection = state
        .db
        .get_sqlite_connection_pool()
        .acquire()
        .await
        .unwrap();
    let mut entered_tx = Some(entered_tx);
    connection
        .lock_handle()
        .await
        .unwrap()
        .set_commit_hook(move || {
            if let Some(entered_tx) = entered_tx.take() {
                let _ = entered_tx.send(());
                let _ = release_rx.recv_timeout(Duration::from_secs(10));
            }
            true
        });
    drop(connection);
    let owned_state = state.clone();
    let task = tokio::spawn(async move {
        post_owned(
            &owned_state.db,
            &owned_state.events,
            channel_id,
            &author,
            "Late commit after abort",
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(2), entered_rx)
        .await
        .unwrap()
        .unwrap();
    task.abort(); // Only this owner-lifecycle test has access to the inner task handle.
    assert!(
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .is_err()
    );
    assert_eq!(frame(&mut body).await, None);
    let mut fresh = state.events.subscribe();
    assert_eq!(
        fresh.next_frame().await,
        None,
        "even a new baseline before the late commit could miss the message"
    );
    release_tx.send(()).unwrap();
    let response = tokio::time::timeout(
        Duration::from_secs(2),
        test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&path)
                .insert_header(("Authorization", bearer.clone()))
                .to_request(),
        ),
    )
    .await
    .unwrap();
    let history: Value = test::read_body_json(response).await;
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
    assert_eq!(history["items"][0]["text"], "Late commit after abort");
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header(("Authorization", bearer))
            .to_request(),
    )
    .await;
    assert_eq!(frame(&mut response.into_body()).await, None);
}

#[actix_web::test]
async fn independent_creations_arrive_out_of_timestamp_order_but_match_sortable_history() {
    let (state, bearer, path, _) = fixture().await;
    let app =
        test::init_service(App::new().app_data(web::Data::new(state)).configure(routes)).await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header(("Authorization", bearer.clone()))
            .to_request(),
    )
    .await;
    let mut body = response.into_body();
    frame(&mut body).await.unwrap();
    let mut messages = Vec::new();
    // Deliberately nonmonotonic creation instants: neither publication nor history
    // may "repair" timestamps to match notification order. The final tie uses ID.
    for (id, timestamp) in [
        (100_000_000_000_111, "2026-01-01T00:00:00.900Z"),
        (100_000_000_000_222, "2026-01-01T00:00:00.100Z"),
        (100_000_000_000_333, "2026-01-01T00:00:00.900Z"),
    ] {
        let response = CONTROL
            .scope(
                Control {
                    ids: Arc::new(Mutex::new(vec![id].into())),
                    created_at: Some(
                        DateTime::parse_from_rfc3339(timestamp)
                            .unwrap()
                            .with_timezone(&Utc),
                    ),
                    ..Default::default()
                },
                test::call_service(
                    &app,
                    test::TestRequest::post()
                        .uri(&path)
                        .insert_header(("Authorization", bearer.clone()))
                        .set_json(json!({"text":"Independent creation"}))
                        .to_request(),
                ),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let message: Value = test::read_body_json(response).await;
        assert_eq!(message["created_at"], timestamp);
        let bytes = frame(&mut body).await.unwrap();
        let event: Value = serde_json::from_str(
            std::str::from_utf8(&bytes)
                .unwrap()
                .strip_prefix("event: change\ndata: ")
                .unwrap()
                .trim(),
        )
        .unwrap();
        assert_eq!(event, json!({"type":"message_created", "message":message}));
        messages.push(message);
    }
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&path)
            .insert_header(("Authorization", bearer))
            .to_request(),
    )
    .await;
    let history: Value = test::read_body_json(response).await;
    assert_eq!(
        history["items"],
        json!([messages[2], messages[0], messages[1]])
    );
}

#[actix_web::test]
async fn unexpected_post_commit_failure_halts_old_and_fresh_delivery_but_history_recovers() {
    let (state, bearer, path, _) = fixture().await;
    let app =
        test::init_service(App::new().app_data(web::Data::new(state)).configure(routes)).await;
    let mut bodies = Vec::new();
    for _ in 0..2 {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/events")
                .insert_header(("Authorization", bearer.clone()))
                .to_request(),
        )
        .await;
        let mut body = response.into_body();
        frame(&mut body).await.unwrap();
        bodies.push(body);
    }
    let response = CONTROL
        .scope(
            Control {
                panic_after_insert: true,
                ..Default::default()
            },
            test::call_service(
                &app,
                test::TestRequest::post()
                    .uri(&path)
                    .insert_header(("Authorization", bearer.clone()))
                    .set_json(json!({"text":"Committed before panic"}))
                    .to_request(),
            ),
        )
        .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    for body in &mut bodies {
        assert_eq!(
            frame(body).await,
            None,
            "never retain healthy delivery after a lost change"
        );
    }
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header(("Authorization", bearer.clone()))
            .to_request(),
    )
    .await;
    assert_eq!(
        frame(&mut response.into_body()).await,
        None,
        "fresh delivery cannot claim safety until restart"
    );
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&path)
            .insert_header(("Authorization", bearer))
            .to_request(),
    )
    .await;
    let history: Value = test::read_body_json(response).await;
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
    assert_eq!(history["items"][0]["text"], "Committed before panic");
}
