use actix_web::{App, http::StatusCode, test, web};
use hamlet::{connect_to_database, routes};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};

#[actix_web::test]
async fn deletion_retains_conversation_blocks_access_and_reuses_names() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("delete.db").display()
    );
    let state = connect_to_database(&url).await.unwrap();
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
                .set_json(json!({"username":username,"password":"long password"}))
                .to_request(),
        )
        .await;
        let auth: Value = test::read_body_json(response).await;
        bearers.push(format!("Bearer {}", auth["access_token"].as_str().unwrap()));
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
    let original = list["items"][0].clone();
    let id = original["id"].as_str().unwrap();
    let path = format!("/api/v1/channels/{id}");
    let messages = format!("{path}/messages");
    for header in [None, Some("Bearer invalid")] {
        let mut request = test::TestRequest::delete().uri(&path);
        if let Some(header) = header {
            request = request.insert_header(("Authorization", header));
        }
        assert_eq!(
            test::call_service(&app, request.to_request())
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    let response = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&path)
            .insert_header(("Authorization", bearers[1].clone()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let error: Value = test::read_body_json(response).await;
    assert_eq!(
        error,
        json!({"error":{"code":"conflict","message":"Cannot delete the last active channel"}})
    );
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearers[0].clone()))
            .set_json(json!({"name":"Other","type":"text"}))
            .to_request(),
    )
    .await;
    let other: Value = test::read_body_json(response).await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&messages)
            .insert_header(("Authorization", bearers[0].clone()))
            .set_json(json!({"text":"retain me"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let message: Value = test::read_body_json(response).await;
    let response = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&path)
            .insert_header(("Authorization", bearers[1].clone()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(test::read_body(response).await.is_empty());
    for (method, target, input) in [
        ("DELETE", path.clone(), json!(null)),
        ("PATCH", path.clone(), json!({"name":"new"})),
        ("POST", messages.clone(), json!({"text":"blocked"})),
        ("GET", messages.clone(), json!(null)),
        ("GET", format!("{messages}?before=bad"), json!(null)),
        (
            "DELETE",
            "/api/v1/channels/999999999999999".into(),
            json!(null),
        ),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::default()
                .method(method.parse().unwrap())
                .uri(&target)
                .insert_header(("Authorization", bearers[0].clone()))
                .set_json(input)
                .to_request(),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{method} {target}"
        );
    }
    for bad_id in ["bad", "123", "12345678901234x"] {
        assert_eq!(
            test::call_service(
                &app,
                test::TestRequest::delete()
                    .uri(&format!("/api/v1/channels/{bad_id}"))
                    .insert_header(("Authorization", bearers[0].clone()))
                    .to_request()
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let row = state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT id, name, type, deleted_at FROM channels WHERE id = ?",
            [id.parse::<i64>().unwrap().into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "name").unwrap(), original["name"]);
    assert_eq!(row.try_get::<String>("", "type").unwrap(), "text");
    chrono::DateTime::parse_from_rfc3339(&row.try_get::<String>("", "deleted_at").unwrap())
        .unwrap();
    let retained = state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT id, channel_id, author_id, text, created_at FROM messages WHERE id = ?",
            [message["id"]
                .as_str()
                .unwrap()
                .parse::<i64>()
                .unwrap()
                .into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        retained
            .try_get::<i64>("", "channel_id")
            .unwrap()
            .to_string(),
        id
    );
    assert_eq!(retained.try_get::<String>("", "text").unwrap(), "retain me");
    assert_eq!(
        retained.try_get::<i64>("", "id").unwrap().to_string(),
        message["id"]
    );
    assert_eq!(
        retained
            .try_get::<i64>("", "author_id")
            .unwrap()
            .to_string(),
        message["author"]["id"]
    );
    let timestamp = chrono::DateTime::parse_from_rfc3339(
        &retained.try_get::<String>("", "created_at").unwrap(),
    )
    .unwrap();
    assert_eq!(
        timestamp,
        chrono::DateTime::parse_from_rfc3339(message["created_at"].as_str().unwrap()).unwrap()
    );
    // Rename may reuse the deleted bootstrap name, including case folding.
    let other_path = format!("/api/v1/channels/{}", other["id"].as_str().unwrap());
    let response = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&other_path)
            .insert_header(("Authorization", bearers[0].clone()))
            .set_json(json!({"name":"GENERAL"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        test::call_service(
            &app,
            test::TestRequest::delete()
                .uri(&other_path)
                .insert_header(("Authorization", bearers[0].clone()))
                .to_request()
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    // Free the active name and create a replacement with an independent identity.
    assert_eq!(
        test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&other_path)
                .insert_header(("Authorization", bearers[0].clone()))
                .set_json(json!({"name":"Other"}))
                .to_request()
        )
        .await
        .status(),
        StatusCode::OK
    );
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearers[0].clone()))
            .set_json(json!({"name":"GENERAL","type":"text"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let replacement: Value = test::read_body_json(response).await;
    assert_ne!(replacement["id"], original["id"]);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/channels/{}/messages",
                replacement["id"].as_str().unwrap()
            ))
            .insert_header(("Authorization", bearers[0].clone()))
            .to_request(),
    )
    .await;
    let history: Value = test::read_body_json(response).await;
    assert_eq!(history["items"], json!([]));
    let restarted = connect_to_database(&url).await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(restarted))
            .configure(routes),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearers[0].clone()))
            .to_request(),
    )
    .await;
    let list: Value = test::read_body_json(response).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 2);
    assert!(
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["id"] != original["id"])
    );
}

#[actix_web::test]
async fn simultaneous_route_deletes_leave_one_active_channel() {
    let dir = tempfile::tempdir().unwrap();
    let state = connect_to_database(&format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("race.db").display()
    ))
    .await
    .unwrap();
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
            .set_json(json!({"username":"Alice","password":"long password"}))
            .to_request(),
    )
    .await;
    let auth: Value = test::read_body_json(response).await;
    let bearer = format!("Bearer {}", auth["access_token"].as_str().unwrap());
    test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", bearer.clone()))
            .set_json(json!({"name":"Other","type":"text"}))
            .to_request(),
    )
    .await;
    let rows = state
        .db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT id FROM channels",
        ))
        .await
        .unwrap();
    let requests: Vec<_> = rows
        .iter()
        .map(|row| {
            test::TestRequest::delete()
                .uri(&format!(
                    "/api/v1/channels/{}",
                    row.try_get::<i64>("", "id").unwrap()
                ))
                .insert_header(("Authorization", bearer.clone()))
                .to_request()
        })
        .collect();
    let mut requests = requests.into_iter();
    let (a, b) = tokio::join!(
        test::call_service(&app, requests.next().unwrap()),
        test::call_service(&app, requests.next().unwrap())
    );
    let mut statuses = [a.status().as_u16(), b.status().as_u16()];
    statuses.sort();
    assert_eq!(statuses, [204, 409]);
    let row = state
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS n FROM channels WHERE deleted_at IS NULL",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
}

#[actix_web::test]
async fn previous_schema_upgrade_preserves_values_and_foreign_keys() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("upgrade.db").display()
    );
    let db = Database::connect(&url).await.unwrap();
    hamlet_migration::Migrator::up(&db, Some(3)).await.unwrap();
    for sql in [
        "INSERT INTO users VALUES (100000000000001, 'Alice', 'alice', 'hash')",
        "INSERT INTO channels VALUES (100000000000002, 'Old', 'old', 'text')",
        "INSERT INTO messages VALUES (100000000000003, 100000000000002, 100000000000001, 'old conversation', '2026-01-01T00:00:00Z')",
    ] {
        db.execute_unprepared(sql).await.unwrap();
    }
    db.close().await.unwrap();
    let state = connect_to_database(&url).await.unwrap();
    let row = state.db.query_one_raw(Statement::from_string(DbBackend::Sqlite,
        "SELECT c.id, c.name, c.name_key, c.type, c.deleted_at, m.id AS message_id, m.author_id, m.text, m.created_at FROM channels c JOIN messages m ON m.channel_id = c.id")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "id").unwrap(), 100000000000002);
    assert_eq!(row.try_get::<String>("", "name").unwrap(), "Old");
    assert_eq!(row.try_get::<String>("", "name_key").unwrap(), "old");
    assert_eq!(row.try_get::<String>("", "type").unwrap(), "text");
    assert_eq!(
        row.try_get::<Option<String>>("", "deleted_at").unwrap(),
        None
    );
    assert_eq!(
        row.try_get::<i64>("", "message_id").unwrap(),
        100000000000003
    );
    assert_eq!(
        row.try_get::<i64>("", "author_id").unwrap(),
        100000000000001
    );
    assert_eq!(
        row.try_get::<String>("", "text").unwrap(),
        "old conversation"
    );
    assert_eq!(
        row.try_get::<String>("", "created_at").unwrap(),
        "2026-01-01T00:00:00Z"
    );
    assert!(
        state
            .db
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_key_check"
            ))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(state.db.execute_unprepared("INSERT INTO messages VALUES (100000000000004, 999, 100000000000001, 'invalid', '2026-01-01T00:00:00Z')").await.is_err());
}
