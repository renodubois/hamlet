use actix_web::{App, http::StatusCode, test, web};
use hamlet::{connect_to_database, routes};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};

#[actix_web::test]
async fn authenticated_shared_rename_contract_and_conversation_retention() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("rename.db").display()
    );
    let state = connect_to_database(&url).await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(routes),
    )
    .await;
    let mut tokens = Vec::new();
    for username in ["Alice", "Bob"] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/signup")
                .set_json(json!({"username":username,"password":"long password"}))
                .to_request(),
        )
        .await;
        let auth: Value = test::read_body_json(response).await;
        tokens.push(auth["access_token"].as_str().unwrap().to_owned());
    }
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", format!("Bearer {}", tokens[0])))
            .to_request(),
    )
    .await;
    let list: Value = test::read_body_json(response).await;
    let original = list["items"][0].clone();
    let id = original["id"].as_str().unwrap();
    let path = format!("/api/v1/channels/{id}");
    let messages = format!("{path}/messages");
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&messages)
            .insert_header(("Authorization", format!("Bearer {}", tokens[0])))
            .set_json(json!({"text":"preserved conversation"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let message: Value = test::read_body_json(response).await;
    for header in [None, Some("Bearer invalid")] {
        let mut request = test::TestRequest::patch()
            .uri(&path)
            .set_json(json!({"name":"unauthorized"}));
        if let Some(header) = header {
            request = request.insert_header(("Authorization", header));
        }
        let response = test::call_service(&app, request.to_request()).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    for input in [
        json!({}),
        json!({"name":42}),
        json!({"name":"ok","type":"text"}),
        json!({"name":"ok","extra":true}),
        json!({"name":" "}),
        json!({"name":"bad!"}),
        json!({"name":"雪"}),
        json!({"name":"line\nbreak"}),
        json!({"name":"x".repeat(65)}),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&path)
                .insert_header(("Authorization", format!("Bearer {}", tokens[1])))
                .set_json(input)
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let error: Value = test::read_body_json(response).await;
        assert_eq!(error["error"]["code"], "bad_request");
    }
    // Bob may rename the bootstrap channel, including case-only and no-op writes.
    for name in [
        "  Alpha-1_test  ".to_owned(),
        "ALPHA-1_TEST".to_owned(),
        "ALPHA-1_TEST".to_owned(),
        "x".repeat(64),
        "a".to_owned(),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&path)
                .insert_header(("Authorization", format!("Bearer {}", tokens[1])))
                .set_json(json!({"name":name}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let channel: Value = test::read_body_json(response).await;
        assert_eq!(channel, json!({"id":id,"name":name.trim(),"type":"text"}));
    }
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", format!("Bearer {}", tokens[0])))
            .set_json(json!({"name":"Other","type":"text"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let other: Value = test::read_body_json(response).await;
    // Both users can rename a channel created by the other user.
    let response = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!(
                "/api/v1/channels/{}",
                other["id"].as_str().unwrap()
            ))
            .insert_header(("Authorization", format!("Bearer {}", tokens[1])))
            .set_json(json!({"name":"Zulu"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    for (target, input, status, code) in [
        (
            path.clone(),
            json!({"name":"zULU"}),
            StatusCode::CONFLICT,
            "conflict",
        ),
        (
            "/api/v1/channels/999999999999999".into(),
            json!({"name":"missing"}),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        (
            "/api/v1/channels/not-an-id".into(),
            json!({"name":"missing"}),
            StatusCode::BAD_REQUEST,
            "bad_request",
        ),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&target)
                .insert_header(("Authorization", format!("Bearer {}", tokens[0])))
                .set_json(input)
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), status);
        let error: Value = test::read_body_json(response).await;
        assert_eq!(error["error"]["code"], code);
    }
    // Successive writes have no precondition: the last successful write wins.
    for name in ["First", "Last"] {
        let response = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&path)
                .insert_header(("Authorization", format!("Bearer {}", tokens[0])))
                .set_json(json!({"name":name}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", format!("Bearer {}", tokens[1])))
            .to_request(),
    )
    .await;
    let list: Value = test::read_body_json(response).await;
    assert_eq!(
        list["items"][0],
        json!({"id":id,"name":"Last","type":"text"})
    );
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&messages)
            .insert_header(("Authorization", format!("Bearer {}", tokens[1])))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let history: Value = test::read_body_json(response).await;
    assert_eq!(history["items"], json!([message]));
    // Simultaneous requests both succeed; the final persisted name is one of
    // their returned values, never a mixed result or a new channel identity.
    let first = test::TestRequest::patch()
        .uri(&path)
        .insert_header(("Authorization", format!("Bearer {}", tokens[0])))
        .set_json(json!({"name":"Concurrent A"}))
        .to_request();
    let second = test::TestRequest::patch()
        .uri(&path)
        .insert_header(("Authorization", format!("Bearer {}", tokens[1])))
        .set_json(json!({"name":"Concurrent B"}))
        .to_request();
    let (first, second) = tokio::join!(
        test::call_service(&app, first),
        test::call_service(&app, second)
    );
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::OK);
    let first: Value = test::read_body_json(first).await;
    let second: Value = test::read_body_json(second).await;
    assert_eq!(first["name"], "Concurrent A");
    assert_eq!(second["name"], "Concurrent B");
    let restarted = connect_to_database(&url).await.unwrap();
    let restarted_app = test::init_service(
        App::new()
            .app_data(web::Data::new(restarted))
            .configure(routes),
    )
    .await;
    let response = test::call_service(
        &restarted_app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", format!("Bearer {}", tokens[0])))
            .to_request(),
    )
    .await;
    let list: Value = test::read_body_json(response).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 2);
    assert!(list["items"][0] == first || list["items"][0] == second);
    // Inject a database failure only in this disposable fixture.
    state
        .db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "CREATE TRIGGER fail_rename BEFORE UPDATE ON channels BEGIN SELECT RAISE(ABORT, 'injected failure'); END",
        ))
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&path)
            .insert_header(("Authorization", format!("Bearer {}", tokens[0])))
            .set_json(json!({"name":"failure"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let error: Value = test::read_body_json(response).await;
    assert_eq!(error["error"]["code"], "internal_error");
}
