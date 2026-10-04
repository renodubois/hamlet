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
            std::future::poll_fn(|cx| Pin::new(&mut *body).poll_next(cx))
        )
        .await
        .is_err(),
        "no phantom or duplicate change, and ordinary errors keep delivery live"
    );
}

async fn fixture(url: &str) -> (hamlet::AppState, String, String) {
    let state = connect_to_database(url).await.unwrap();
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
    (state, bearer, path)
}

#[actix_web::test]
async fn rejected_and_failed_writes_publish_nothing_and_no_subscriber_is_normal() {
    let (state, bearer, path) = fixture("sqlite::memory:").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(routes),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&path)
            .insert_header(("Authorization", bearer.clone()))
            .set_json(json!({"text":"Before subscription"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
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
    quiet(&mut body).await; // Pre-subscription creation is not replayed.
    for (uri, input, status) in [
        (
            path.as_str(),
            json!({"text":"  \n\t"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            &path,
            json!({"text":"雪".repeat(4001)}),
            StatusCode::BAD_REQUEST,
        ),
        (
            &path,
            json!({"text":"ok", "extra":true}),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/api/v1/channels/999999999999999/messages",
            json!({"text":"ok"}),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(uri)
                .insert_header(("Authorization", bearer.clone()))
                .set_json(input)
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), status);
        quiet(&mut body).await;
    }
    state.db.execute_raw(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER reject_message BEFORE INSERT ON messages BEGIN SELECT RAISE(ABORT, 'forced insert failure'); END"
    )).await.unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&path)
            .insert_header(("Authorization", bearer.clone()))
            .set_json(json!({"text":"Rejected"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    quiet(&mut body).await;
    state
        .db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER reject_message",
        ))
        .await
        .unwrap();
    // Maximum legal text, including characters requiring JSON escaping.
    let text = "雪\n\r\t".repeat(1000);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&path)
            .insert_header(("Authorization", bearer.clone()))
            .set_json(json!({"text":text}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let recovered: Value = test::read_body_json(response).await;
    assert_eq!(recovered["text"], text);
    assert_eq!(
        change(&mut body).await,
        json!({"type":"message_created", "message":recovered})
    );
    quiet(&mut body).await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&path)
            .insert_header(("Authorization", bearer))
            .to_request(),
    )
    .await;
    let history: Value = test::read_body_json(response).await;
    assert_eq!(history["items"], json!([recovered, first]));
}

#[actix_web::test]
async fn slow_and_dropped_subscribers_never_hold_writes_or_healthy_delivery() {
    let (state, bearer, path) = fixture("sqlite::memory:").await;
    let app =
        test::init_service(App::new().app_data(web::Data::new(state)).configure(routes)).await;
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
                    .uri(&path)
                    .insert_header(("Authorization", bearer.clone()))
                    .set_json(json!({"text":format!("Message {index}")}))
                    .to_request(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::CREATED);
            let message: Value = test::read_body_json(response).await;
            assert_eq!(
                change(&mut healthy).await,
                json!({"type":"message_created", "message":message})
            );
        }
    })
    .await
    .expect("writes never wait for slow/dropped subscribers");
    assert_eq!(
        frame(&mut slow).await,
        None,
        "overflow ends rather than skipping updates"
    );
    quiet(&mut healthy).await;
}

#[actix_web::test]
async fn concurrent_creations_across_channels_deliver_each_identity_once_without_order_assumptions()
{
    let dir = tempfile::tempdir().unwrap();
    let (state, bearer, path) = fixture(&format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("concurrent.db").display()
    ))
    .await;
    let app =
        test::init_service(App::new().app_data(web::Data::new(state)).configure(routes)).await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearer.clone()))
            .set_json(json!({"name":"Other", "type":"text"}))
            .to_request(),
    )
    .await;
    let other: Value = test::read_body_json(response).await;
    let other_path = format!(
        "/api/v1/channels/{}/messages",
        other["id"].as_str().unwrap()
    );
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
                .uri(if index % 2 == 0 { &path } else { &other_path })
                .insert_header(("Authorization", bearer.clone()))
                .set_json(json!({"text":"Independent identical text"}))
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
    for response in responses {
        assert_eq!(response.status(), StatusCode::CREATED);
        let message: Value = test::read_body_json(response).await;
        assert!(
            confirmed
                .insert(message["id"].as_str().unwrap().to_owned(), message)
                .is_none()
        );
    }
    assert_eq!(confirmed.len(), 16);
    let mut delivered = std::collections::BTreeMap::new();
    for _ in 0..16 {
        let event = change(&mut body).await;
        assert_eq!(event["type"], "message_created");
        let message = event["message"].clone();
        assert!(
            delivered
                .insert(message["id"].as_str().unwrap().to_owned(), message)
                .is_none()
        );
    }
    assert_eq!(delivered, confirmed);
    quiet(&mut body).await;
    let mut authoritative = std::collections::BTreeMap::new();
    for path in [&path, &other_path] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(path)
                .insert_header(("Authorization", bearer.clone()))
                .to_request(),
        )
        .await;
        let history: Value = test::read_body_json(response).await;
        assert_eq!(history["items"].as_array().unwrap().len(), 8);
        for message in history["items"].as_array().unwrap() {
            authoritative.insert(message["id"].as_str().unwrap().to_owned(), message.clone());
        }
    }
    assert_eq!(authoritative, delivered);
}

#[actix_web::test]
async fn author_lookup_failure_after_insert_cannot_suppress_response_or_matching_fanout() {
    let state = connect_to_database("sqlite::memory:").await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(routes),
    )
    .await;
    let mut users = Vec::new();
    for username in ["Alice", "Bob"] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/signup")
                .set_json(json!({"username":username, "password":"long password"}))
                .to_request(),
        )
        .await;
        users.push(test::read_body_json::<Value, _>(response).await);
    }
    let bearer = format!("Bearer {}", users[0]["access_token"].as_str().unwrap());
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearer.clone()))
            .to_request(),
    )
    .await;
    let channels: Value = test::read_body_json(response).await;
    let channel_id = channels["items"][0]["id"].as_str().unwrap();
    let path = format!("/api/v1/channels/{channel_id}/messages");
    let mut bodies = Vec::new();
    for user in &users {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/events")
                .insert_header((
                    "Authorization",
                    format!("Bearer {}", user["access_token"].as_str().unwrap()),
                ))
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
    // Narrow real-DB fault: the insert commits, but any subsequent author decode
    // fails on invalid UTF-8. Authentication already supplied the valid author.
    state.db.execute_raw(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER corrupt_author AFTER INSERT ON messages BEGIN UPDATE users SET username = x'80' WHERE id = NEW.author_id; END"
    )).await.unwrap();
    let text = "  Hello 雪\nsecond line\r\nevent: forged  ";
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&path)
            .insert_header(("Authorization", bearer.clone()))
            .set_json(json!({"text":text}))
            .to_request(),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "a committed message must not depend on a fallible post-write author read"
    );
    let message: Value = test::read_body_json(response).await;
    assert_eq!(
        message["author"],
        json!({"id":users[0]["user"]["id"], "display_name":"Alice"})
    );
    assert_eq!(message["channel_id"], channel_id);
    assert_eq!(message["text"], text);
    assert_eq!(message["id"].as_str().unwrap().len(), 15);
    chrono::DateTime::parse_from_rfc3339(message["created_at"].as_str().unwrap()).unwrap();
    for body in &mut bodies {
        assert_eq!(
            change(body).await,
            json!({"type":"message_created", "message":message})
        );
        quiet(body).await;
    }
    // Restore the deliberately damaged author so ordinary authenticated history
    // can independently confirm the committed entity, not just the write response.
    state
        .db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE users SET username = 'Alice' WHERE username_key = 'alice'",
        ))
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&path)
            .insert_header(("Authorization", bearer))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let history: Value = test::read_body_json(response).await;
    assert_eq!(history["items"], json!([message]));
}
