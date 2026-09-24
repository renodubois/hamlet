use actix_web::{App, http::StatusCode, test, web};
use hamlet_rewrite::{connect, routes};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};

#[actix_web::test]
async fn messages_keep_author_identity_and_preserve_text() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("messages.db").display()
    );
    let db = connect(&url).await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.clone()))
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
    let token = auth["access_token"].as_str().unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    let channels: Value = test::read_body_json(response).await;
    let id = channels["items"][0]["id"].as_str().unwrap();
    let path = format!("/api/v1/channels/{id}/messages");
    let text = "  Unicode 🎉  ";
    let mut created = Vec::new();
    for text in [text, "second"] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&path)
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(json!({"text":text}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let item: Value = test::read_body_json(response).await;
        assert_eq!(item["text"], text);
        assert_eq!(item["author"]["id"], auth["user"]["id"]);
        assert_eq!(item["author"]["display_name"], "Alice");
        assert_eq!(item["channel_id"], id);
        assert_eq!(item["id"].as_str().unwrap().len(), 15);
        chrono::DateTime::parse_from_rfc3339(item["created_at"].as_str().unwrap()).unwrap();
        created.push(item);
    }
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&path)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    let recent: Value = test::read_body_json(response).await;
    assert_eq!(recent["items"][0]["text"], "second");
    // Tie timestamps across two messages; ID is the deterministic second key.
    db.db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE messages SET created_at = '2026-01-01T00:00:00.000000000Z'",
        ))
        .await
        .unwrap();
    db.db.execute_raw(Statement::from_string(DbBackend::Sqlite,
        "UPDATE users SET username = 'NewAlice', username_key = 'newalice' WHERE username_key = 'alice'")).await.unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&path)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let history: Value = test::read_body_json(response).await;
    assert!(history["next_cursor"].is_null());
    assert_eq!(history["items"].as_array().unwrap().len(), 2);
    let first_id: i64 = history["items"][0]["id"].as_str().unwrap().parse().unwrap();
    let second_id: i64 = history["items"][1]["id"].as_str().unwrap().parse().unwrap();
    assert!(first_id > second_id);
    for item in history["items"].as_array().unwrap() {
        assert_eq!(item["author"]["id"], auth["user"]["id"]);
        assert_eq!(item["author"]["display_name"], "NewAlice");
    }
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&path)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"text":"🎉".repeat(4000)}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    for invalid in [
        json!({"text":"  "}),
        json!({"text":"a".repeat(4001)}),
        json!({"text":"ok", "extra":1}),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&path)
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(invalid)
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    for i in 0..51 {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&path)
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(json!({"text":format!("entry {i}")}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&path)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    let first_page: Value = test::read_body_json(response).await;
    assert_eq!(first_page["items"].as_array().unwrap().len(), 50);
    let cursor = first_page["next_cursor"].as_str().unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("{path}?before={cursor}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    let last_page: Value = test::read_body_json(response).await;
    assert_eq!(last_page["items"].as_array().unwrap().len(), 4);
    assert!(last_page["next_cursor"].is_null());
    assert!(
        first_page["items"]
            .as_array()
            .unwrap()
            .iter()
            .chain(last_page["items"].as_array().unwrap().iter())
            .all(|item| item["author"]["display_name"] == "NewAlice")
    );
    let missing = "/api/v1/channels/999999999999999/messages";
    for (method, uri, authorization, expected) in [
        ("GET", missing, Some(token), StatusCode::NOT_FOUND),
        ("POST", missing, Some(token), StatusCode::NOT_FOUND),
        ("GET", path.as_str(), None, StatusCode::UNAUTHORIZED),
        ("POST", path.as_str(), None, StatusCode::UNAUTHORIZED),
        (
            "GET",
            path.as_str(),
            Some("invalid"),
            StatusCode::UNAUTHORIZED,
        ),
    ] {
        let mut req = if method == "GET" {
            test::TestRequest::get()
        } else {
            test::TestRequest::post().set_json(json!({"text":"ok"}))
        }
        .uri(uri);
        if let Some(value) = authorization {
            req = req.insert_header(("Authorization", format!("Bearer {value}")));
        }
        let response = test::call_service(&app, req.to_request()).await;
        assert_eq!(response.status(), expected);
        let body: Value = test::read_body_json(response).await;
        assert!(body["error"]["code"].is_string());
    }
}
