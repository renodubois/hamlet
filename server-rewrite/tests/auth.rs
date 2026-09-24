use actix_web::{App, http::StatusCode, test, web};
use hamlet_rewrite::{connect, routes};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};

#[actix_web::test]
async fn signup_and_identity_are_isolated_and_sessions_are_digest_only() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("rewrite.db").display()
    );
    let db = connect(&url).await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.clone()))
            .configure(routes),
    )
    .await;
    let request = test::TestRequest::post()
        .uri("/api/v1/auth/signup")
        .set_json(json!({"username":"Alice_1", "password":"correct horse battery staple"}))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["user"]["username"], "Alice_1");
    assert_eq!(body["user"]["id"].as_str().unwrap().len(), 15);
    let token = body["access_token"].as_str().unwrap();
    let expiry =
        chrono::DateTime::parse_from_rfc3339(body["expires_at"].as_str().unwrap()).unwrap();
    let remaining = expiry.signed_duration_since(chrono::Utc::now());
    assert!(remaining > chrono::Duration::days(29));
    assert!(remaining <= chrono::Duration::days(30));
    let rows = db
        .db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT token_digest FROM sessions",
        ))
        .await
        .unwrap();
    let stored: String = rows[0].try_get("", "token_digest").unwrap();
    assert_ne!(stored, token);
    assert_eq!(stored.len(), 64);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("x-request-id"));
    let me: Value = test::read_body_json(response).await;
    assert_eq!(me, body["user"]);
    let response = test::call_service(
        &app,
        test::TestRequest::get().uri("/api/v1/me").to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let error: Value = test::read_body_json(response).await;
    assert_eq!(error["error"]["code"], "unauthorized");
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(json!({"username":"aLiCe_1", "password":"other-password"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(json!({"username":"bad!", "password":"other-password"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    for invalid in [
        json!({"username":"ab", "password":"other-password"}),
        json!({"username":"Bad-Name", "password":"other-password"}),
        json!({"username":"Alice", "password":"short"}),
        json!({"username":"Alice", "password":"other-password", "extra":true}),
    ] {
        let result = test::try_call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/signup")
                .set_json(invalid)
                .to_request(),
        )
        .await;
        match result {
            Ok(response) => assert_eq!(response.status(), StatusCode::BAD_REQUEST),
            Err(error) => {
                assert_eq!(
                    error.as_response_error().status_code(),
                    StatusCode::BAD_REQUEST
                );
                let body = actix_web::body::to_bytes(error.error_response().into_body())
                    .await
                    .unwrap();
                let body: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(body["error"]["code"], "bad_request");
            }
        }
    }
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/signup")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["error"]["code"], "method_not_allowed");
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/me")
            .insert_header(("Authorization", "Bearer invalid"))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/unknown")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(response.headers().contains_key("x-request-id"));
    db.db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE sessions SET expires_at = ?",
            ["2020-01-01T00:00:00Z".into()],
        ))
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    connect(&url).await.unwrap();
}
