use actix_web::{App, body::BoxBody, body::MessageBody, http::StatusCode, test, web};
use hamlet::{connect_to_database, routes};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};
use std::{pin::Pin, time::Duration};

async fn frame(body: &mut BoxBody) -> Option<web::Bytes> {
    tokio::time::timeout(
        Duration::from_secs(2),
        std::future::poll_fn(|cx| Pin::new(&mut *body).poll_next(cx)),
    )
    .await
    .expect("bounded frame read")
    .map(Result::unwrap)
}

async fn change(body: &mut BoxBody) -> Value {
    let bytes = frame(body).await.expect("live subscription");
    let text = std::str::from_utf8(&bytes).unwrap();
    serde_json::from_str(text.strip_prefix("event: change\ndata: ").unwrap().trim()).unwrap()
}

async fn quiet(body: &mut BoxBody) {
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            std::future::poll_fn(|cx| { Pin::new(&mut *body).poll_next(cx) })
        )
        .await
        .is_err(),
        "no phantom or duplicate change"
    );
}

#[actix_web::test]
async fn rejected_and_failed_creations_publish_nothing_and_no_subscriber_is_normal() {
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
        test::TestRequest::post()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearer.clone()))
            .set_json(json!({"name":"First", "type":"text"}))
            .to_request(),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "no subscribers must not fail the write"
    );
    let first: Value = test::read_body_json(response).await;
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
    quiet(&mut body).await; // Neither bootstrap nor the pre-subscription creation is replayed.
    for (input, expected) in [
        (json!({"name":" ", "type":"text"}), StatusCode::BAD_REQUEST),
        (
            json!({"name":"bad!", "type":"text"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"name":"X".repeat(65), "type":"text"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"name":"New", "type":"unknown"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"name":" fIRSt ", "type":"text"}),
            StatusCode::CONFLICT,
        ),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/channels")
                .insert_header(("Authorization", bearer.clone()))
                .set_json(input)
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), expected);
        quiet(&mut body).await;
    }
    state.db.execute_raw(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER reject_channel BEFORE INSERT ON channels BEGIN SELECT RAISE(ABORT, 'forced insert failure'); END"))
        .await.unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearer.clone()))
            .set_json(json!({"name":"Rejected", "type":"text"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    quiet(&mut body).await;
    state
        .db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER reject_channel",
        ))
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearer.clone()))
            .set_json(json!({"name":"Recovered", "type":"text"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let recovered: Value = test::read_body_json(response).await;
    assert_eq!(
        change(&mut body).await,
        json!({"type":"channel_created", "channel":recovered})
    );
    quiet(&mut body).await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearer))
            .to_request(),
    )
    .await;
    let list: Value = test::read_body_json(response).await;
    let items = list["items"].as_array().unwrap();
    assert!(items.contains(&first));
    assert!(items.contains(&recovered));
    assert!(!items.iter().any(|channel| channel["name"] == "Rejected"));
}

#[actix_web::test]
async fn slow_and_dropped_subscribers_do_not_hold_writes_or_healthy_delivery() {
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
    for _ in 0..3 {
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
    drop(bodies.pop().unwrap());
    let mut healthy = bodies.pop().unwrap();
    let mut slow = bodies.pop().unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        for index in 0..257 {
            let response = test::call_service(
                &app,
                test::TestRequest::post()
                    .uri("/api/v1/channels")
                    .insert_header(("Authorization", bearer.clone()))
                    .set_json(json!({"name":format!("Channel {index}"), "type":"text"}))
                    .to_request(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::CREATED);
            let channel: Value = test::read_body_json(response).await;
            assert_eq!(
                change(&mut healthy).await,
                json!({"type":"channel_created", "channel":channel})
            );
        }
    })
    .await
    .expect("mutation completion never waits for slow subscriber");
    assert_eq!(
        frame(&mut slow).await,
        None,
        "overflow terminates, never silently skips"
    );
    quiet(&mut healthy).await;
}

#[actix_web::test]
async fn concurrent_creations_publish_each_confirmed_identity_once_without_commit_order_assumptions()
 {
    let dir = tempfile::tempdir().unwrap();
    let state = connect_to_database(&format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("concurrent.db").display()
    ))
    .await
    .unwrap();
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
    let requests = (0..16).map(|index| {
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/channels")
                .insert_header(("Authorization", bearer.clone()))
                .set_json(json!({"name":format!("Concurrent {}", index / 2), "type":"text"}))
                .to_request(),
        )
    });
    let responses = tokio::time::timeout(
        Duration::from_secs(5),
        futures_util::future::join_all(requests),
    )
    .await
    .unwrap();
    let mut confirmed = std::collections::BTreeMap::new();
    let mut conflicts = 0;
    for response in responses {
        match response.status() {
            StatusCode::CREATED => {
                let channel: Value = test::read_body_json(response).await;
                assert!(
                    confirmed
                        .insert(channel["id"].as_str().unwrap().to_owned(), channel)
                        .is_none()
                );
            }
            StatusCode::CONFLICT => conflicts += 1,
            status => panic!("unexpected status {status}"),
        }
    }
    assert_eq!(confirmed.len(), 8);
    assert_eq!(conflicts, 8);
    let mut delivered = std::collections::BTreeMap::new();
    for _ in 0..8 {
        let event = change(&mut body).await;
        assert_eq!(event["type"], "channel_created");
        let channel = event["channel"].clone();
        assert!(
            delivered
                .insert(channel["id"].as_str().unwrap().to_owned(), channel)
                .is_none()
        );
    }
    assert_eq!(delivered, confirmed);
    quiet(&mut body).await;
}

#[actix_web::test]
async fn channel_creation_reaches_multiple_subscribers_with_the_http_payload() {
    let dir = tempfile::tempdir().unwrap();
    let state = connect_to_database(&format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("channels.db").display()
    ))
    .await
    .unwrap();
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
        assert_eq!(
            frame(&mut body).await.unwrap(),
            "event: ready\ndata: {}\n\n"
        );
        bodies.push(body);
    }
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearer.clone()))
            .set_json(json!({"name":"  New Channel  ", "type":"text"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let channel: Value = test::read_body_json(response).await;
    assert_eq!(channel["name"], "New Channel");
    assert_eq!(channel["type"], "text");
    for body in &mut bodies {
        assert_eq!(
            change(body).await,
            json!({"type":"channel_created", "channel":channel})
        );
        quiet(body).await;
    }
}
