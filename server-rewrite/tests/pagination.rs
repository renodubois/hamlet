use actix_web::{App, http::StatusCode, test, web};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hamlet::{connect_to_database, routes};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};
use std::collections::HashSet;

#[actix_web::test]
async fn cursor_pages_are_bounded_and_stable_on_ties_and_cross_channel_positions() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("pagination.db").display()
    );
    let db = connect_to_database(&url).await.unwrap();
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
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&path)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    let empty: Value = test::read_body_json(response).await;
    assert_eq!(empty, json!({"items":[], "next_cursor":null}));
    for i in 0..5 {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&path)
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(json!({"text":format!("message {i}")}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    db.db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE messages SET created_at = '2026-01-01T00:00:00.000000000Z'",
        ))
        .await
        .unwrap();
    let mut seen = HashSet::new();
    let mut before: Option<String> = None;
    let mut ordered_ids = Vec::new();
    for expected_count in [2, 2, 1] {
        let uri = before.as_ref().map_or_else(
            || format!("{path}?limit=2"),
            |cursor| format!("{path}?limit=2&before={cursor}"),
        );
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&uri)
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let page: Value = test::read_body_json(response).await;
        assert_eq!(page["items"].as_array().unwrap().len(), expected_count);
        for item in page["items"].as_array().unwrap() {
            let message_id: i64 = item["id"].as_str().unwrap().parse().unwrap();
            assert!(seen.insert(message_id));
            ordered_ids.push(message_id);
        }
        before = page["next_cursor"].as_str().map(str::to_owned);
        assert_eq!(before.is_some(), expected_count == 2);
    }
    assert_eq!(seen.len(), 5);
    assert!(ordered_ids.windows(2).all(|pair| pair[0] > pair[1]));
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("{path}?limit=1"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    let page: Value = test::read_body_json(response).await;
    let cursor = page["next_cursor"].as_str().unwrap();
    let position: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(cursor).unwrap()).unwrap();
    assert_eq!(
        position,
        json!({
            "created_at":"2026-01-01T00:00:00.000000000Z",
            "id":page["items"][0]["id"].as_str().unwrap().parse::<i64>().unwrap()
        })
    );
    // A position need not reference an existing message.
    let arbitrary = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&json!({
            "created_at":"2026-01-01T00:00:00.000000000Z", "id":999999999999999_i64
        }))
        .unwrap(),
    );
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("{path}?limit=5&before={arbitrary}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let arbitrary_page: Value = test::read_body_json(response).await;
    assert_eq!(arbitrary_page["items"].as_array().unwrap().len(), 5);
    // Returned cursors survive a restart/reconnect without a persisted key.
    let restarted = connect_to_database(&url).await.unwrap();
    let app_after_restart = test::init_service(
        App::new()
            .app_data(web::Data::new(restarted))
            .configure(routes),
    )
    .await;
    let response = test::call_service(
        &app_after_restart,
        test::TestRequest::get()
            .uri(&format!("{path}?before={cursor}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"name":"other","type":"text"}))
            .to_request(),
    )
    .await;
    let other: Value = test::read_body_json(response).await;
    let other_id = other["id"].as_str().unwrap();
    // A cursor from another channel is just a position in this channel's history.
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/channels/{other_id}/messages?before={cursor}"
            ))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let other_page: Value = test::read_body_json(response).await;
    assert_eq!(other_page, json!({"items":[], "next_cursor":null}));
    let invalid_timestamp = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&json!({
            "created_at":"not-a-date", "id":ordered_ids[0]
        }))
        .unwrap(),
    );
    let invalid_id = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&json!({
            "created_at":"2026-01-01T00:00:00.000000000Z", "id":0
        }))
        .unwrap(),
    );
    for uri in [
        format!("{path}?limit=0"),
        format!("{path}?limit=101"),
        format!("{path}?limit=abc"),
        format!("{path}?before=garbage"),
        format!("{path}?before="),
        format!("{path}?before={invalid_timestamp}"),
        format!("{path}?before={invalid_id}"),
        format!("{path}?before={}.signature", cursor),
        format!("{path}?before={}", "a".repeat(513)),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&uri)
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["error"]["code"], "bad_request");
    }
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels/999999999999999/messages?before=garbage")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("{path}?limit=5"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    let page: Value = test::read_body_json(response).await;
    assert_eq!(page["items"].as_array().unwrap().len(), 5);
    assert!(page["next_cursor"].is_null());
}
