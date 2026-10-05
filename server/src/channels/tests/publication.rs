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

// Owner-local fault/randomness control, scoped to one creation request.
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
async fn missed_publication_does_not_disable_existing_or_fresh_subscriptions() {
    use futures_util::FutureExt;
    use std::panic::AssertUnwindSafe;

    let state = connect_to_database("sqlite::memory:").await.unwrap();
    let mut existing = state.events.subscribe();
    existing.next_frame().await.unwrap();
    let input = CreateChannel {
        name: "Committed without notification".into(),
        kind: super::super::types::ChannelType::Text,
    };
    let outcome = AssertUnwindSafe(CONTROL.scope(
        Control {
            panic_after_insert: true,
            ..Default::default()
        },
        create(&state.db, &state.events, &input),
    ))
    .catch_unwind()
    .await;
    assert!(!matches!(outcome, Ok(Ok(_))));
    assert!(
        list(&state.db)
            .await
            .unwrap()
            .items
            .iter()
            .any(|channel| channel.name == input.name)
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(30), existing.next_frame())
            .await
            .is_err(),
        "a missed notification neither publishes nor closes delivery"
    );

    let mut fresh = state.events.subscribe();
    assert_eq!(
        fresh.next_frame().await.unwrap(),
        "event: ready\ndata: {}\n\n"
    );
    let channel = create(
        &state.db,
        &state.events,
        &CreateChannel {
            name: "Next ordinary creation".into(),
            kind: input.kind,
        },
    )
    .await
    .ok()
    .unwrap();
    for subscription in [&mut existing, &mut fresh] {
        let bytes = tokio::time::timeout(Duration::from_secs(2), subscription.next_frame())
            .await
            .unwrap()
            .unwrap();
        let event: Value = serde_json::from_str(
            std::str::from_utf8(&bytes)
                .unwrap()
                .strip_prefix("event: change\ndata: ")
                .unwrap()
                .trim(),
        )
        .unwrap();
        assert_eq!(event, json!({"type":"channel_created", "channel":channel}));
    }
}
