use actix_web::{App, http::StatusCode, test, web};
use hamlet::{connect_to_database, routes};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};

#[actix_web::test]
async fn bootstrap_create_shared_listing_validation_and_order() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("channels.db").display()
    );
    let db = connect_to_database(&url).await.unwrap();
    let users = db
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT count(*) AS n FROM users",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(users.try_get::<i64>("", "n").unwrap(), 0);
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
    let auth: Value = test::read_body_json(signup).await;
    let token = auth["access_token"].as_str().unwrap();
    let list = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list: Value = test::read_body_json(list).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["name"], "general");
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"name":" Zebra ","type":"text"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created: Value = test::read_body_json(response).await;
    assert_eq!(created["name"], "Zebra");
    assert_eq!(created["id"].as_str().unwrap().len(), 15);
    assert_eq!(created.as_object().unwrap().len(), 3);
    for invalid in [
        json!({"name":"x"}),
        json!({"name":"x","type":"voice"}),
        json!({"name":"x!","type":"text"}),
        json!({"name":"   ","type":"text"}),
        json!({"name":"x","type":"text","extra":1}),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/channels")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(invalid)
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let error: Value = test::read_body_json(response).await;
        assert_eq!(error["error"]["code"], "bad_request");
    }
    let duplicate = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"name":"zEBRA","type":"text"}))
            .to_request(),
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    let duplicate: Value = test::read_body_json(duplicate).await;
    assert_eq!(duplicate["error"]["code"], "conflict");
    let second = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(json!({"username":"Bob", "password":"another password"}))
            .to_request(),
    )
    .await;
    let second: Value = test::read_body_json(second).await;
    let second_token = second["access_token"].as_str().unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", format!("Bearer {second_token}")))
            .to_request(),
    )
    .await;
    let list: Value = test::read_body_json(response).await;
    assert_eq!(list["items"][0]["name"], "general");
    assert_eq!(list["items"][1]["name"], "Zebra");
    db.db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "DELETE FROM channels WHERE name_key = 'general'",
        ))
        .await
        .unwrap();
    connect_to_database(&url).await.unwrap();
    let count = db
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT count(*) AS n FROM channels",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(count.try_get::<i64>("", "n").unwrap(), 1);
    let row = db
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name FROM channels",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "name").unwrap(), "Zebra");
    db.db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "DELETE FROM channels",
        ))
        .await
        .unwrap();
    connect_to_database(&url).await.unwrap();
    let row = db
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name FROM channels",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "name").unwrap(), "general");
    for header in [None, Some("Bearer invalid")] {
        let mut get = test::TestRequest::get().uri("/api/v1/channels");
        let mut post = test::TestRequest::post()
            .uri("/api/v1/channels")
            .set_json(json!({"name":"hidden", "type":"text"}));
        if let Some(header) = header {
            get = get.insert_header(("Authorization", header));
            post = post.insert_header(("Authorization", header));
        }
        for request in [get.to_request(), post.to_request()] {
            let response = test::call_service(&app, request).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }
}
