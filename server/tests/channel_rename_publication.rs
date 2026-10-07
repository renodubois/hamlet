use actix_web::{App, body::BoxBody, body::MessageBody, http::StatusCode, test, web};
use hamlet::{connect_to_database, routes};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};
use std::{pin::Pin, time::Duration};

async fn frame(body: &mut BoxBody) -> web::Bytes {
    tokio::time::timeout(
        Duration::from_secs(2),
        std::future::poll_fn(|cx| Pin::new(&mut *body).poll_next(cx)),
    )
    .await
    .expect("bounded frame read")
    .expect("live subscription")
    .unwrap()
}

async fn quiet(body: &mut BoxBody) {
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            std::future::poll_fn(|cx| Pin::new(&mut *body).poll_next(cx))
        )
        .await
        .is_err(),
        "no phantom or duplicate rename"
    );
}

#[actix_web::test]
async fn two_users_receive_matching_renames_and_failures_publish_nothing() {
    let state = connect_to_database("sqlite::memory:").await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(routes),
    )
    .await;
    let mut bearers = Vec::new();
    for username in ["Alice", "Bob"] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/signup")
                .set_json(json!({"username":username, "password":"long password"}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let auth: Value = test::read_body_json(response).await;
        bearers.push(format!("Bearer {}", auth["access_token"].as_str().unwrap()));
    }
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearers[0].clone()))
            .set_json(json!({"name":"Original", "type":"text"}))
            .to_request(),
    )
    .await;
    let original: Value = test::read_body_json(response).await;
    let uri = format!("/api/v1/channels/{}", original["id"].as_str().unwrap());
    // A different user can rename without any subscribers.
    let response = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&uri)
            .insert_header(("Authorization", bearers[1].clone()))
            .set_json(json!({"name":"Before subscription"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut bodies = Vec::new();
    for bearer in &bearers {
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
        assert_eq!(frame(&mut body).await, "event: ready\ndata: {}\n\n");
        quiet(&mut body).await; // Earlier renames are not replayed.
        bodies.push(body);
    }
    for (bearer, name, expected) in [
        (&bearers[0], "  Renamed  ", "Renamed"),
        (&bearers[1], "RENAMED", "RENAMED"),
        (&bearers[0], "RENAMED", "RENAMED"), // no-op publication is permitted
        (
            &bearers[1],
            "Last successful rename",
            "Last successful rename",
        ),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&uri)
                .insert_header(("Authorization", bearer.clone()))
                .set_json(json!({"name":name}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let channel: Value = test::read_body_json(response).await;
        assert_eq!(channel["id"], original["id"]);
        assert_eq!(channel["type"], original["type"]);
        assert_eq!(channel["name"], expected);
        for body in &mut bodies {
            let bytes = frame(body).await;
            let event: Value = serde_json::from_str(
                std::str::from_utf8(&bytes)
                    .unwrap()
                    .strip_prefix("event: change\ndata: ")
                    .unwrap()
                    .trim(),
            )
            .unwrap();
            assert_eq!(event, json!({"type":"channel_renamed", "channel":channel}));
            let decoded: hamlet_protocol::Event = serde_json::from_value(event).unwrap();
            assert!(matches!(
                decoded,
                hamlet_protocol::Event::ChannelRenamed { .. }
            ));
            quiet(body).await;
        }
    }
    for (target, bearer, input, expected) in [
        (
            uri.as_str(),
            None,
            json!({"name":"Unauthorized"}),
            StatusCode::UNAUTHORIZED,
        ),
        (
            uri.as_str(),
            Some("Bearer invalid"),
            json!({"name":"Unauthorized"}),
            StatusCode::UNAUTHORIZED,
        ),
        (
            uri.as_str(),
            Some(bearers[0].as_str()),
            json!({"name":"bad!"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            uri.as_str(),
            Some(bearers[0].as_str()),
            json!({"name":"New", "type":"text"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            uri.as_str(),
            Some(bearers[1].as_str()),
            json!({"name":"GENERAL"}),
            StatusCode::CONFLICT,
        ),
        (
            "/api/v1/channels/000000000000000",
            Some(bearers[1].as_str()),
            json!({"name":"Missing"}),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let mut request = test::TestRequest::patch().uri(target).set_json(input);
        if let Some(bearer) = bearer {
            request = request.insert_header(("Authorization", bearer));
        }
        let response = test::call_service(&app, request.to_request()).await;
        assert_eq!(response.status(), expected);
        for body in &mut bodies {
            quiet(body).await;
        }
    }
    state.db.execute_raw(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER reject_rename BEFORE UPDATE ON channels BEGIN SELECT RAISE(ABORT, 'forced update failure'); END"))
        .await.unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&uri)
            .insert_header(("Authorization", bearers[0].clone()))
            .set_json(json!({"name":"Rejected"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    for body in &mut bodies {
        quiet(body).await;
    }
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearers[0].clone()))
            .to_request(),
    )
    .await;
    let list: Value = test::read_body_json(response).await;
    let channel = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|channel| channel["id"] == original["id"])
        .unwrap();
    assert_eq!(channel["name"], "Last successful rename");
}
