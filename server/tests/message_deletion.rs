use actix_web::{
    App,
    body::{BoxBody, MessageBody},
    http::StatusCode,
    test, web,
};
use hamlet::{connect_to_database, routes};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};
use std::{pin::Pin, time::Duration};

macro_rules! request {
    ($app:expr, $method:ident, $path:expr, $bearer:expr) => {
        test::call_service(
            &$app,
            test::TestRequest::$method()
                .uri($path)
                .insert_header(("Authorization", ($bearer).to_string()))
                .to_request(),
        )
        .await
    };
    ($app:expr, $method:ident, $path:expr, $bearer:expr, $body:expr) => {
        test::call_service(
            &$app,
            test::TestRequest::$method()
                .uri($path)
                .insert_header(("Authorization", ($bearer).to_string()))
                .set_json($body)
                .to_request(),
        )
        .await
    };
}
macro_rules! setup {
    ($state:ident, $app:ident, $author:ident, $other:ident, $channel:ident, $message:ident) => {
        let fixture_dir = tempfile::tempdir().unwrap();
        let $state = connect_to_database(&format!("sqlite://{}?mode=rwc", fixture_dir.path().join("messages.db").display())).await.unwrap();
        let $app = test::init_service(App::new().app_data(web::Data::new($state.clone())).configure(routes)).await;
        let mut bearers = Vec::new();
        for username in ["Alice", "Bob"] {
            let response = test::call_service(&$app, test::TestRequest::post().uri("/api/v1/auth/signup")
                .set_json(json!({"username":username,"password":"long password"})).to_request()).await;
            assert_eq!(response.status(), StatusCode::CREATED);
            let auth: Value = test::read_body_json(response).await;
            bearers.push(format!("Bearer {}", auth["access_token"].as_str().unwrap()));
        }
        let $author = bearers.remove(0);
        let $other = bearers.remove(0);
        let response = request!($app, get, "/api/v1/channels", &$author);
        let list: Value = test::read_body_json(response).await;
        let $channel = list["items"][0]["id"].as_str().unwrap().to_owned();
        let response = request!($app, post, &format!("/api/v1/channels/{}/messages", $channel), &$author, json!({"text":"original message"}));
        assert_eq!(response.status(), StatusCode::CREATED);
        let $message: Value = test::read_body_json(response).await;
    };
}
async fn frame(body: &mut BoxBody) -> actix_web::web::Bytes {
    tokio::time::timeout(
        Duration::from_secs(2),
        std::future::poll_fn(|cx| Pin::new(&mut *body).poll_next(cx)),
    )
    .await
    .expect("bounded frame")
    .unwrap()
    .unwrap()
}
async fn quiet(body: &mut BoxBody) {
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            std::future::poll_fn(|cx| Pin::new(&mut *body).poll_next(cx))
        )
        .await
        .is_err()
    );
}

#[actix_web::test]
async fn rejected_requests_and_non_author_repeats_are_silent_and_use_error_envelopes() {
    setup!(state, app, author, other, channel, message);
    let id = message["id"].as_str().unwrap();
    let path = format!("/api/v1/channels/{channel}/messages/{id}");
    let second: Value = test::read_body_json(request!(
        app,
        post,
        "/api/v1/channels",
        &author,
        json!({"name":"Other","type":"text"})
    ))
    .await;
    let response = request!(app, get, "/api/v1/events", &author);
    let mut body = response.into_body();
    assert_eq!(frame(&mut body).await, "event: ready\ndata: {}\n\n");
    for (uri, bearer, status, code) in [
        (path.clone(), "", StatusCode::UNAUTHORIZED, "unauthorized"),
        (
            path.clone(),
            "Bearer invalid",
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        ),
        (
            path.clone(),
            other.as_str(),
            StatusCode::FORBIDDEN,
            "forbidden",
        ),
        (
            format!(
                "/api/v1/channels/{}/messages/{id}",
                second["id"].as_str().unwrap()
            ),
            author.as_str(),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        (
            format!("/api/v1/channels/000000000000000/messages/{id}"),
            author.as_str(),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        (
            format!("/api/v1/channels/{channel}/messages/000000000000000"),
            author.as_str(),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
    ] {
        let response = request!(app, delete, &uri, bearer);
        assert_eq!(response.status(), status);
        let error: Value = test::read_body_json(response).await;
        assert_eq!(error["error"]["code"], code);
        quiet(&mut body).await;
    }
    for invalid in [
        "bad-id",
        "12345678901234",
        "1234567890123456",
        "12345678901234x",
        "-23456789012345",
    ] {
        for (c, m) in [(invalid, id), (channel.as_str(), invalid)] {
            let response = request!(
                app,
                delete,
                &format!("/api/v1/channels/{c}/messages/{m}"),
                &author
            );
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let error: Value = test::read_body_json(response).await;
            assert_eq!(error["error"]["code"], "bad_request");
            quiet(&mut body).await;
        }
    }
    state.db.execute_raw(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER reject_delete BEFORE UPDATE ON messages BEGIN SELECT RAISE(ABORT, 'forced failure'); END")).await.unwrap();
    let response = request!(app, delete, &path, &author);
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let error: Value = test::read_body_json(response).await;
    assert_eq!(error["error"]["code"], "internal_error");
    quiet(&mut body).await;
    state
        .db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER reject_delete",
        ))
        .await
        .unwrap();
    // The identity captured by bearer validation, not the current displayed name,
    // owns the message. Swap display names while keeping persistent identities.
    state
        .db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE users SET username = CASE username WHEN 'Alice' THEN 'Bob' ELSE 'Alice' END",
        ))
        .await
        .unwrap();
    assert_eq!(
        request!(app, delete, &path, &other).status(),
        StatusCode::FORBIDDEN
    );
    quiet(&mut body).await;
    assert_eq!(
        request!(app, delete, &path, &author).status(),
        StatusCode::NO_CONTENT
    );
    frame(&mut body).await;
    let response = request!(app, delete, &path, &other);
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let error: Value = test::read_body_json(response).await;
    assert_eq!(error["error"]["code"], "forbidden");
    quiet(&mut body).await;
    // A deleted channel makes both owner and non-owner requests missing.
    assert_eq!(
        request!(app, delete, &format!("/api/v1/channels/{channel}"), &author).status(),
        StatusCode::NO_CONTENT
    );
    frame(&mut body).await;
    for bearer in [&author, &other] {
        let response = request!(app, delete, &path, bearer);
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let error: Value = test::read_body_json(response).await;
        assert_eq!(error["error"]["code"], "not_found");
        quiet(&mut body).await;
    }
    assert_eq!(
        request!(
            app,
            get,
            &format!("/api/v1/channels/{channel}/messages"),
            &author
        )
        .status(),
        StatusCode::NOT_FOUND
    );
    let row = state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT text, deleted_at FROM messages WHERE id = ?",
            [id.parse::<i64>().unwrap().into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<String>("", "text").unwrap(),
        "original message"
    );
    assert!(
        row.try_get::<Option<String>>("", "deleted_at")
            .unwrap()
            .is_some()
    );
    state
        .db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE sessions SET expires_at = '2000-01-01T00:00:00Z'",
        ))
        .await
        .unwrap();
    let response = request!(app, delete, &path, &author);
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let error: Value = test::read_body_json(response).await;
    assert_eq!(error["error"]["code"], "unauthorized");
    quiet(&mut body).await;
}

#[actix_web::test]
async fn concurrent_author_repeats_publish_once_and_keep_the_first_timestamp() {
    setup!(state, app, author, other, channel, message);
    let id = message["id"].as_str().unwrap();
    let path = format!("/api/v1/channels/{channel}/messages/{id}");
    let mut body = request!(app, get, "/api/v1/events", &other).into_body();
    frame(&mut body).await;
    // Any attempt to rewrite a deletion timestamp is a database failure, so
    // successful concurrent repeats also prove that the first value is retained.
    state.db.execute_unprepared("CREATE TRIGGER preserve_deletion BEFORE UPDATE OF deleted_at ON messages WHEN OLD.deleted_at IS NOT NULL BEGIN SELECT RAISE(ABORT, 'timestamp overwritten'); END").await.unwrap();
    let requests = (0..8).map(|_| {
        test::TestRequest::delete()
            .uri(&path)
            .insert_header(("Authorization", author.clone()))
            .to_request()
    });
    let responses = tokio::time::timeout(
        Duration::from_secs(5),
        futures_util::future::join_all(requests.map(|request| test::call_service(&app, request))),
    )
    .await
    .expect("bounded concurrent repeats");
    for response in responses {
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(test::read_body(response).await.is_empty());
    }
    let bytes = frame(&mut body).await;
    let event: Value = serde_json::from_str(
        std::str::from_utf8(&bytes)
            .unwrap()
            .strip_prefix("event: change\ndata: ")
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(
        event,
        json!({"type":"message_deleted","channel_id":channel,"message_id":id})
    );
    quiet(&mut body).await;
    let before = state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT deleted_at FROM messages WHERE id = ?",
            [id.parse::<i64>().unwrap().into()],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "deleted_at")
        .unwrap();
    assert_eq!(
        request!(app, delete, &path, &author).status(),
        StatusCode::NO_CONTENT
    );
    let after = state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT deleted_at FROM messages WHERE id = ?",
            [id.parse::<i64>().unwrap().into()],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "deleted_at")
        .unwrap();
    assert_eq!(before, after);
    quiet(&mut body).await;
}

#[actix_web::test]
async fn deleted_rows_are_filtered_before_pagination_and_deleted_cursors_still_work() {
    setup!(state, app, author, _other, channel, message);
    let base = format!("/api/v1/channels/{channel}/messages");
    assert_eq!(
        request!(
            app,
            delete,
            &format!("{base}/{}", message["id"].as_str().unwrap()),
            &author
        )
        .status(),
        StatusCode::NO_CONTENT
    );
    for n in 1..=8_i64 {
        state.db.execute_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
            "INSERT INTO messages (id, channel_id, author_id, text, created_at) VALUES (?, ?, ?, ?, ?)",
            [(100_000_000_000_000 + n).into(), channel.parse::<i64>().unwrap().into(), message["author"]["id"].as_str().unwrap().parse::<i64>().unwrap().into(), format!("message {n}").into(), "1999-01-01T00:00:00.000000000Z".into()])).await.unwrap();
    }
    let first: Value =
        test::read_body_json(request!(app, get, &format!("{base}?limit=2"), &author)).await;
    assert_eq!(first["items"][0]["id"], "100000000000008");
    assert_eq!(first["items"][1]["id"], "100000000000007");
    let cursor = first["next_cursor"].as_str().unwrap();
    for n in [2, 4, 6, 7, 8] {
        assert_eq!(
            request!(
                app,
                delete,
                &format!("{base}/{}", 100_000_000_000_000_i64 + n),
                &author
            )
            .status(),
            StatusCode::NO_CONTENT
        );
    }
    for query in ["limit=2".to_owned(), format!("limit=2&before={cursor}")] {
        let page: Value =
            test::read_body_json(request!(app, get, &format!("{base}?{query}"), &author)).await;
        assert_eq!(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["100000000000005", "100000000000003"]
        );
        assert!(page["items"][0].get("deleted_at").is_none());
        let older: Value = test::read_body_json(request!(
            app,
            get,
            &format!(
                "{base}?limit=2&before={}",
                page["next_cursor"].as_str().unwrap()
            ),
            &author
        ))
        .await;
        assert_eq!(older["items"].as_array().unwrap().len(), 1);
        assert_eq!(older["items"][0]["id"], "100000000000001");
        assert!(older["next_cursor"].is_null());
    }
    for n in [1, 3, 5] {
        assert_eq!(
            request!(
                app,
                delete,
                &format!("{base}/{}", 100_000_000_000_000_i64 + n),
                &author
            )
            .status(),
            StatusCode::NO_CONTENT
        );
    }
    for query in ["limit=2".to_owned(), format!("limit=2&before={cursor}")] {
        let empty: Value =
            test::read_body_json(request!(app, get, &format!("{base}?{query}"), &author)).await;
        assert_eq!(empty, json!({"items":[],"next_cursor":null}));
    }
}

#[actix_web::test]
async fn author_soft_deletes_with_one_identity_only_event_and_idempotent_repeats() {
    setup!(state, app, author, other, channel, message);
    let id = message["id"].as_str().unwrap();
    let path = format!("/api/v1/channels/{channel}/messages/{id}");
    let response = request!(app, get, "/api/v1/events", &other);
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    assert_eq!(frame(&mut body).await, "event: ready\ndata: {}\n\n");
    quiet(&mut body).await;
    let response = request!(app, delete, &path, &author);
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(test::read_body(response).await.is_empty());
    let bytes = frame(&mut body).await;
    let wire: Value = serde_json::from_str(
        std::str::from_utf8(&bytes)
            .unwrap()
            .strip_prefix("event: change\ndata: ")
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(
        wire,
        json!({"type":"message_deleted","channel_id":channel,"message_id":id})
    );
    assert!(matches!(
        serde_json::from_value::<hamlet_protocol::Event>(wire).unwrap(),
        hamlet_protocol::Event::MessageDeleted { .. }
    ));
    let retained = state.db.query_one_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
        "SELECT id, channel_id, author_id, text, created_at, deleted_at FROM messages WHERE id = ?", [id.parse::<i64>().unwrap().into()])).await.unwrap().unwrap();
    let deleted_at = retained.try_get::<String>("", "deleted_at").unwrap();
    assert!(chrono::DateTime::parse_from_rfc3339(&deleted_at).is_ok());
    assert_eq!(
        retained.try_get::<String>("", "text").unwrap(),
        "original message"
    );
    assert_eq!(
        retained
            .try_get::<i64>("", "author_id")
            .unwrap()
            .to_string(),
        message["author"]["id"]
    );
    assert_eq!(
        retained
            .try_get::<i64>("", "channel_id")
            .unwrap()
            .to_string(),
        channel
    );
    let created = chrono::DateTime::parse_from_rfc3339(
        &retained.try_get::<String>("", "created_at").unwrap(),
    )
    .unwrap();
    assert_eq!(
        created,
        chrono::DateTime::parse_from_rfc3339(message["created_at"].as_str().unwrap()).unwrap()
    );
    for _ in 0..2 {
        let response = request!(app, delete, &path, &author);
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(test::read_body(response).await.is_empty());
        quiet(&mut body).await;
    }
    let retained = state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT deleted_at FROM messages WHERE id = ?",
            [id.parse::<i64>().unwrap().into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        retained.try_get::<String>("", "deleted_at").unwrap(),
        deleted_at
    );
    // New subscriptions do not replay a deletion observed before readiness.
    let mut fresh = request!(app, get, "/api/v1/events", &author).into_body();
    assert_eq!(frame(&mut fresh).await, "event: ready\ndata: {}\n\n");
    quiet(&mut fresh).await;
}
