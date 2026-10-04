use actix_web::{App, body::MessageBody, http::StatusCode, test, web};
use hamlet::{connect_to_database, routes};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use serde_json::{Value, json};
use std::{pin::Pin, time::Duration};

async fn frame(body: &mut actix_web::body::BoxBody) -> Option<web::Bytes> {
    tokio::time::timeout(
        Duration::from_secs(2),
        std::future::poll_fn(|cx| Pin::new(&mut *body).poll_next(cx)),
    )
    .await
    .expect("bounded frame read")
    .map(Result::unwrap)
}

fn change() -> hamlet_protocol::Event {
    serde_json::from_value(json!({"type":"message_created", "message": {
        "id":"100000000000001", "channel_id":"100000000000002", "text":"雪\nnext\rline",
        "author":{"id":"100000000000003", "display_name":"Alice"}, "created_at":"2026-01-01T00:00:00Z"
    }})).unwrap()
}

#[actix_web::test]
async fn subscription_precedes_ready_and_fanout_is_fresh_and_escaped() {
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
    state.events.notify(change()); // No subscribers is normal; this must not be replayed.
    let mut bodies = Vec::new();
    for _ in 0..2 {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/events")
                .insert_header(("Authorization", bearer.clone()))
                .insert_header(("Last-Event-ID", "old-id"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        bodies.push(response.into_body());
    }
    state.events.notify(change()); // Registered even though neither ready was polled yet.
    for body in &mut bodies {
        assert_eq!(frame(body).await.unwrap(), "event: ready\ndata: {}\n\n");
        let bytes = frame(body).await.unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.starts_with("event: change\ndata: {"));
        assert!(text.ends_with("\n\n"));
        assert_eq!(text.lines().count(), 3);
        assert!(!text.contains("\nid:"));
        let payload: Value =
            serde_json::from_str(text.strip_prefix("event: change\ndata: ").unwrap().trim())
                .unwrap();
        assert_eq!(payload["type"], "message_created");
        assert_eq!(payload["message"]["text"], "雪\nnext\rline");
        assert!(
            tokio::time::timeout(
                Duration::from_millis(20),
                std::future::poll_fn(|cx| Pin::new(&mut *body).poll_next(cx))
            )
            .await
            .is_err(),
            "no replay or duplicate"
        );
    }
}

async fn advance(duration: Duration) {
    tokio::time::pause();
    tokio::time::advance(duration).await;
    tokio::time::resume();
}

#[actix_web::test]
async fn idle_stream_sends_comment_heartbeat() {
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
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header((
                "Authorization",
                format!("Bearer {}", auth["access_token"].as_str().unwrap()),
            ))
            .to_request(),
    )
    .await;
    let mut body = response.into_body();
    assert_eq!(
        frame(&mut body).await.unwrap(),
        "event: ready\ndata: {}\n\n"
    );
    advance(Duration::from_secs(15)).await;
    assert_eq!(frame(&mut body).await.unwrap(), ": heartbeat\n\n");
}

#[actix_web::test]
async fn revoked_session_closes_idle_and_busy_streams_before_more_delivery() {
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
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/logout")
            .insert_header(("Authorization", bearer.clone()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    advance(Duration::from_secs(15)).await;
    assert_eq!(frame(&mut bodies[0]).await, None, "idle revocation");
    for _ in 0..128 {
        state.events.notify(change());
    }
    assert_eq!(
        frame(&mut bodies[1]).await,
        None,
        "due validation wins over queued traffic"
    );
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header(("Authorization", bearer))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn known_expiry_preempts_idle_queued_and_not_yet_ready_streams() {
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
    let expiry = (chrono::Utc::now() + chrono::Duration::seconds(5)).to_rfc3339();
    state
        .db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE sessions SET expires_at = ?",
            [expiry.into()],
        ))
        .await
        .unwrap();
    let mut bodies = Vec::new();
    for i in 0..3 {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/events")
                .insert_header((
                    "Authorization",
                    format!("Bearer {}", auth["access_token"].as_str().unwrap()),
                ))
                .to_request(),
        )
        .await;
        let mut body = response.into_body();
        if i < 2 {
            frame(&mut body).await.unwrap();
        }
        bodies.push(body);
    }
    advance(Duration::from_secs(6)).await;
    assert_eq!(frame(&mut bodies[0]).await, None, "idle expiry");
    state.events.notify(change());
    assert_eq!(
        frame(&mut bodies[1]).await,
        None,
        "expiry beats queued traffic"
    );
    assert_eq!(frame(&mut bodies[2]).await, None, "expiry beats readiness");
}

#[actix_web::test]
async fn lag_terminates_before_ready_and_after_ready_even_when_heartbeat_is_due() {
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
    let mut bodies = Vec::new();
    for i in 0..2 {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/events")
                .insert_header((
                    "Authorization",
                    format!("Bearer {}", auth["access_token"].as_str().unwrap()),
                ))
                .to_request(),
        )
        .await;
        let mut body = response.into_body();
        if i == 1 {
            frame(&mut body).await.unwrap();
        }
        bodies.push(body);
    }
    for _ in 0..257 {
        state.events.notify(change());
    }
    advance(Duration::from_secs(15)).await;
    for body in &mut bodies {
        assert_eq!(
            frame(body).await,
            None,
            "lag must end, never skip then resume"
        );
    }
}

#[actix_web::test]
async fn pending_validation_suppresses_all_frames_then_resumes_only_on_success() {
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
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header((
                "Authorization",
                format!("Bearer {}", auth["access_token"].as_str().unwrap()),
            ))
            .to_request(),
    )
    .await;
    let mut body = response.into_body();
    frame(&mut body).await.unwrap();
    // Memory databases have one connection. Holding it is a deterministic validation barrier.
    let barrier = state.db.begin().await.unwrap();
    advance(Duration::from_secs(15)).await;
    state.events.notify(change());
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            std::future::poll_fn(|cx| Pin::new(&mut body).poll_next(cx))
        )
        .await
        .is_err(),
        "neither heartbeat nor queued change during validation"
    );
    barrier.rollback().await.unwrap();
    assert_eq!(frame(&mut body).await.unwrap(), ": heartbeat\n\n");
    assert!(
        frame(&mut body)
            .await
            .unwrap()
            .starts_with(b"event: change\n")
    );
}

#[actix_web::test]
async fn expiry_cancels_stalled_validation_without_delivering_queued_changes() {
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
    let expiry = (chrono::Utc::now() + chrono::Duration::seconds(20)).to_rfc3339();
    state
        .db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE sessions SET expires_at = ?",
            [expiry.into()],
        ))
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header((
                "Authorization",
                format!("Bearer {}", auth["access_token"].as_str().unwrap()),
            ))
            .to_request(),
    )
    .await;
    let mut body = response.into_body();
    frame(&mut body).await.unwrap();
    let barrier = state.db.begin().await.unwrap();
    advance(Duration::from_secs(15)).await;
    state.events.notify(change());
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            std::future::poll_fn(|cx| Pin::new(&mut body).poll_next(cx))
        )
        .await
        .is_err()
    );
    advance(Duration::from_secs(6)).await;
    assert_eq!(frame(&mut body).await, None);
    barrier.rollback().await.unwrap();
}

#[actix_web::test]
async fn failed_validation_ends_stream_but_next_handshake_is_server_error_not_rejection() {
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
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    frame(&mut body).await.unwrap();
    state
        .db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TABLE sessions",
        ))
        .await
        .unwrap();
    state.events.notify(change());
    advance(Duration::from_secs(15)).await;
    assert_eq!(
        frame(&mut body).await,
        None,
        "no replacement status or error frame after headers"
    );
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header(("Authorization", bearer))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["error"]["code"], "internal_error");
}

#[actix_web::test]
async fn continuous_changes_cannot_postpone_revalidation() {
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
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/logout")
            .insert_header(("Authorization", bearer))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    for _ in 0..14 {
        advance(Duration::from_secs(1)).await;
        state.events.notify(change());
        assert!(
            frame(&mut body)
                .await
                .unwrap()
                .starts_with(b"event: change\n")
        );
    }
    advance(Duration::from_secs(1)).await;
    state.events.notify(change());
    assert_eq!(
        frame(&mut body).await,
        None,
        "traffic must not restart the validation deadline"
    );
}

#[actix_web::test]
async fn protected_stream_has_uniform_methods_and_finite_ready_frame() {
    let state = connect_to_database("sqlite::memory:").await.unwrap();
    let app =
        test::init_service(App::new().app_data(web::Data::new(state)).configure(routes)).await;
    for credential in [None, Some("Bearer invalid"), Some("Basic invalid")] {
        let mut request = test::TestRequest::get().uri("/api/v1/events");
        if let Some(value) = credential {
            request = request.insert_header(("Authorization", value));
        }
        let response = test::call_service(&app, request.to_request()).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["error"]["code"], "unauthorized");
    }
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
    for method in ["POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
        let response = test::call_service(
            &app,
            test::TestRequest::default()
                .method(method.parse().unwrap())
                .uri("/api/v1/events")
                .insert_header(("Authorization", bearer.clone()))
                .to_request(),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{method}"
        );
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["error"]["code"], "method_not_allowed");
    }
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header(("Authorization", bearer))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
    assert_eq!(
        response
            .headers()
            .get("content-encoding")
            .and_then(|v| v.to_str().ok()),
        Some("identity")
    );
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "no-cache, no-transform"
    );
    assert_eq!(response.headers().get("x-accel-buffering").unwrap(), "no");
    let mut body = response.into_body();
    assert_eq!(
        frame(&mut body).await.unwrap(),
        "event: ready\ndata: {}\n\n"
    );
}
