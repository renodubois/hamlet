use super::*;
use crate::{connect_to_database, routes};
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

// Owner-local fault/randomness control, scoped to one request and its owned task.
// Never compiled into the production server or exposed over HTTP.
#[derive(Clone, Default)]
pub(super) struct Control {
    ids: Arc<Mutex<VecDeque<i64>>>,
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
pub(super) fn after_insert() {
    if CONTROL
        .try_with(|control| control.panic_after_insert)
        .unwrap_or(false)
    {
        panic!("injected unexpected post-commit failure");
    }
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
async fn id_collisions_retry_without_phantoms_and_exhaustion_publishes_nothing() {
    let state = connect_to_database("sqlite::memory:").await.unwrap();
    let app =
        test::init_service(App::new().app_data(web::Data::new(state)).configure(routes)).await;
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
    for (name, ids, expected_id) in [
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
    ] {
        let control = Control {
            ids: Arc::new(Mutex::new(ids.into())),
            ..Default::default()
        };
        let response = CONTROL
            .scope(
                control,
                test::call_service(
                    &app,
                    test::TestRequest::post()
                        .uri("/api/v1/channels")
                        .insert_header(("Authorization", bearer.clone()))
                        .set_json(json!({"name":name, "type":"text"}))
                        .to_request(),
                ),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let channel: Value = test::read_body_json(response).await;
        assert_eq!(channel["id"], expected_id);
        let bytes = frame(&mut body).await.unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        let event: Value =
            serde_json::from_str(text.strip_prefix("event: change\ndata: ").unwrap().trim())
                .unwrap();
        assert_eq!(event, json!({"type":"channel_created", "channel":channel}));
    }
    let control = Control {
        ids: Arc::new(Mutex::new(vec![100_000_000_000_222; 5].into())),
        ..Default::default()
    };
    let response = CONTROL
        .scope(
            control,
            test::call_service(
                &app,
                test::TestRequest::post()
                    .uri("/api/v1/channels")
                    .insert_header(("Authorization", bearer.clone()))
                    .set_json(json!({"name":"Exhausted", "type":"text"}))
                    .to_request(),
            ),
        )
        .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            std::future::poll_fn(|cx| Pin::new(&mut body).poll_next(cx))
        )
        .await
        .is_err(),
        "neither retry collisions nor exhaustion emit extra changes or terminate delivery"
    );
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearer))
            .to_request(),
    )
    .await;
    let list: Value = test::read_body_json(response).await;
    assert_eq!(
        list["items"].as_array().unwrap().len(),
        3,
        "bootstrap and two successful channels only"
    );
}

#[actix_web::test]
async fn aborted_owned_write_closes_delivery_even_when_sqlite_commits_after_abort() {
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
        create_owned(
            &owned_state.db,
            &owned_state.events,
            &CreateChannel {
                name: "Late commit after abort".into(),
                kind: super::super::types::ChannelType::Text,
            },
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(2), entered_rx)
        .await
        .unwrap()
        .unwrap();
    task.abort(); // Only this owner-lifecycle test can obtain the inner task's abort handle.
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
        "reconnecting before the late commit is unsafe too"
    );
    release_tx.send(()).unwrap();
    let response = tokio::time::timeout(
        Duration::from_secs(2),
        test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/channels")
                .insert_header(("Authorization", bearer))
                .to_request(),
        ),
    )
    .await
    .unwrap();
    let list: Value = test::read_body_json(response).await;
    assert!(
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "Late commit after abort")
    );
}

#[actix_web::test]
async fn unexpected_post_commit_failure_halts_delivery_but_authoritative_reads_recover() {
    let state = connect_to_database("sqlite::memory:").await.unwrap();
    let app =
        test::init_service(App::new().app_data(web::Data::new(state)).configure(routes)).await;
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
                    .uri("/api/v1/channels")
                    .insert_header(("Authorization", bearer.clone()))
                    .set_json(json!({"name":"Committed before panic", "type":"text"}))
                    .to_request(),
            ),
        )
        .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    for body in &mut bodies {
        assert_eq!(
            frame(body).await,
            None,
            "never leave healthy delivery after a lost change"
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
    let mut fresh = response.into_body();
    assert_eq!(
        frame(&mut fresh).await,
        None,
        "new delivery cannot claim safety until restart"
    );
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearer))
            .to_request(),
    )
    .await;
    let channels: Value = test::read_body_json(response).await;
    assert!(
        channels["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "Committed before panic")
    );
}
