use crate::{connect_to_database, routes};
use actix_web::{App, http::StatusCode, test, web};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Notify;

// Narrow request-local gates prove the exact stale-check boundaries without
// sleeps, alternate workflow code, or production HTTP instrumentation.
struct Gate {
    history: bool,
    reached: Notify,
    resume: Notify,
}
tokio::task_local! { static GATE: Arc<Gate>; }

pub(super) async fn pause(history: bool) {
    if let Ok(gate) = GATE.try_with(Arc::clone)
        && gate.history == history
    {
        gate.reached.notify_one();
        gate.resume.notified().await;
    }
}

#[actix_web::test]
async fn send_and_history_are_ordered_against_deletion() {
    for history in [false, true] {
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
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/channels")
                .insert_header(("Authorization", bearer.clone()))
                .to_request(),
        )
        .await;
        let list: Value = test::read_body_json(response).await;
        let path = format!(
            "/api/v1/channels/{}",
            list["items"][0]["id"].as_str().unwrap()
        );
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/channels")
                .insert_header(("Authorization", bearer.clone()))
                .set_json(json!({"name":"Other","type":"text"}))
                .to_request(),
        )
        .await;
        let messages = format!("{path}/messages");
        assert_eq!(
            test::call_service(
                &app,
                test::TestRequest::post()
                    .uri(&messages)
                    .insert_header(("Authorization", bearer.clone()))
                    .set_json(json!({"text":"before delete"}))
                    .to_request()
            )
            .await
            .status(),
            StatusCode::CREATED
        );
        let gate = Arc::new(Gate {
            history,
            reached: Notify::new(),
            resume: Notify::new(),
        });
        let request = test::TestRequest::default()
            .method(if history {
                actix_web::http::Method::GET
            } else {
                actix_web::http::Method::POST
            })
            .uri(&messages)
            .insert_header(("Authorization", bearer.clone()))
            .set_json(json!({"text":"must not insert"}))
            .to_request();
        let reading = GATE.scope(gate.clone(), test::call_service(&app, request));
        let deleting = async {
            gate.reached.notified().await;
            let response = test::call_service(
                &app,
                test::TestRequest::delete()
                    .uri(&path)
                    .insert_header(("Authorization", bearer.clone()))
                    .to_request(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::NO_CONTENT);
            gate.resume.notify_one();
        };
        let (response, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(reading, deleting)
        })
        .await
        .expect("bounded gates");
        if history {
            assert_eq!(response.status(), StatusCode::OK);
            let page: Value = test::read_body_json(response).await;
            assert_eq!(page["items"].as_array().unwrap().len(), 1);
            assert_eq!(page["items"][0]["text"], "before delete");
        } else {
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
        assert_eq!(
            test::call_service(
                &app,
                test::TestRequest::get()
                    .uri(&messages)
                    .insert_header(("Authorization", bearer.clone()))
                    .to_request()
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );
        let row = state
            .db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS n FROM messages",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    }
}
