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
        "no phantom, replayed or duplicate deletion"
    );
}

#[actix_web::test]
async fn two_users_receive_deletions_and_failed_deletes_publish_nothing() {
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
    let mut ids = Vec::new();
    for name in ["Before subscription", "Shared deletion", "Failed deletion"] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/channels")
                .insert_header(("Authorization", bearers[0].clone()))
                .set_json(json!({"name":name, "type":"text"}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let channel: Value = test::read_body_json(response).await;
        ids.push(channel["id"].as_str().unwrap().to_owned());
    }
    // A different user may delete, even without subscribers.
    let response = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&format!("/api/v1/channels/{}", ids[0]))
            .insert_header(("Authorization", bearers[1].clone()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
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
        quiet(&mut body).await; // No replay of earlier deletions.
        bodies.push(body);
    }
    for (id, bearer, expected) in [
        (ids[1].as_str(), None, StatusCode::UNAUTHORIZED),
        (
            ids[1].as_str(),
            Some("Bearer invalid"),
            StatusCode::UNAUTHORIZED,
        ),
        ("bad-id", Some(bearers[0].as_str()), StatusCode::BAD_REQUEST),
        (
            "000000000000000",
            Some(bearers[1].as_str()),
            StatusCode::NOT_FOUND,
        ),
        (
            ids[0].as_str(),
            Some(bearers[1].as_str()),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let mut request = test::TestRequest::delete().uri(&format!("/api/v1/channels/{id}"));
        if let Some(bearer) = bearer {
            request = request.insert_header(("Authorization", bearer));
        }
        assert_eq!(
            test::call_service(&app, request.to_request())
                .await
                .status(),
            expected
        );
        for body in &mut bodies {
            quiet(body).await;
        }
    }
    // An actual database mutation failure emits no success event.
    state.db.execute_raw(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER reject_delete BEFORE UPDATE ON channels BEGIN SELECT RAISE(ABORT, 'forced update failure'); END"))
        .await.unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&format!("/api/v1/channels/{}", ids[2]))
            .insert_header(("Authorization", bearers[1].clone()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    for body in &mut bodies {
        quiet(body).await;
    }
    state
        .db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER reject_delete",
        ))
        .await
        .unwrap();
    for id in &ids[1..] {
        let response = test::call_service(
            &app,
            test::TestRequest::delete()
                .uri(&format!("/api/v1/channels/{id}"))
                .insert_header(("Authorization", bearers[1].clone()))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(test::read_body(response).await.is_empty());
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
            assert_eq!(event, json!({"type":"channel_deleted", "channel_id":id}));
            let hamlet_protocol::Event::ChannelDeleted { channel_id } =
                serde_json::from_value(event).unwrap()
            else {
                panic!("expected deletion");
            };
            assert_eq!(&channel_id, id);
            quiet(body).await;
        }
        // Repeating a successful deletion is missing, not another success.
        let response = test::call_service(
            &app,
            test::TestRequest::delete()
                .uri(&format!("/api/v1/channels/{id}"))
                .insert_header(("Authorization", bearers[0].clone()))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        for body in &mut bodies {
            quiet(body).await;
        }
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
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    let last = list["items"][0]["id"].as_str().unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&format!("/api/v1/channels/{last}"))
            .insert_header(("Authorization", bearers[1].clone()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    for body in &mut bodies {
        quiet(body).await;
    }
    // A fresh subscription also has no deletion backlog.
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header(("Authorization", bearers[0].clone()))
            .to_request(),
    )
    .await;
    let mut fresh = response.into_body();
    assert_eq!(frame(&mut fresh).await, "event: ready\ndata: {}\n\n");
    quiet(&mut fresh).await;
}
