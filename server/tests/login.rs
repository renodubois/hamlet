use actix_web::{App, http::StatusCode, test, web};
use hamlet::{connect_to_database, routes};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};

#[actix_web::test]
async fn independent_sessions_and_logout() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("login.db").display()
    );
    let db = connect_to_database(&url).await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.clone()))
            .configure(routes),
    )
    .await;
    let signup = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(json!({"username":"Alice", "password":"long password"}))
            .to_request(),
    )
    .await;
    assert_eq!(signup.status(), StatusCode::CREATED);
    let first: Value = test::read_body_json(signup).await;
    let token = first["access_token"].as_str().unwrap();
    let mut errors = Vec::new();
    for username in ["Alice", "unknown"] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/login")
                .set_json(json!({"username": username, "password": "incorrect password"}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let error: Value = test::read_body_json(response).await;
        assert_eq!(error["error"]["code"], "unauthorized");
        errors.push(error);
    }
    assert_eq!(errors[0], errors[1]);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(json!({"username":"aLiCe", "password":"long password"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let second: Value = test::read_body_json(response).await;
    assert_eq!(second["user"], first["user"]);
    assert_ne!(second["access_token"], first["access_token"]);
    let second_token = second["access_token"].as_str().unwrap();
    let expired =
        chrono::DateTime::parse_from_rfc3339(second["expires_at"].as_str().unwrap()).unwrap();
    assert!(expired.signed_duration_since(chrono::Utc::now()) > chrono::Duration::days(29));
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/logout")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(test::read_body(response).await.is_empty());
    for (token, expected) in [
        (token, StatusCode::UNAUTHORIZED),
        (second_token, StatusCode::OK),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/me")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), expected);
    }
    let rows = db
        .db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT token_digest, expires_at FROM sessions",
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let stored: String = rows[0].try_get("", "token_digest").unwrap();
    assert_ne!(stored, second_token);
    let persisted_expiry: String = rows[0].try_get("", "expires_at").unwrap();
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(&persisted_expiry).unwrap(),
        chrono::DateTime::parse_from_rfc3339(second["expires_at"].as_str().unwrap()).unwrap()
    );
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/logout")
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/logout")
            .insert_header(("Authorization", "Bearer invalid"))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/logout")
            .insert_header(("Authorization", format!("Bearer {second_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    let error: Value = test::read_body_json(response).await;
    assert_eq!(error["error"]["code"], "method_not_allowed");
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
            .insert_header(("Authorization", format!("Bearer {second_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/logout")
            .insert_header(("Authorization", format!("Bearer {second_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
