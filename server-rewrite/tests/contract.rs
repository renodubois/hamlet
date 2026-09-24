use actix_web::{App, http::StatusCode, test, web};
use hamlet_rewrite::{connect, contract, routes};
use serde_json::{Value, json};
use std::collections::BTreeSet;

// The explicit API inventory cross-checks annotations against Actix registrations.
// The source scan below fails if a new GET/POST handler is registered without inventory coverage.
const INVENTORY: &[(&str, &str, &str, u16, bool)] = &[
    ("post", "/api/v1/auth/signup", "signup", 201, false),
    ("post", "/api/v1/auth/login", "login", 200, false),
    ("post", "/api/v1/auth/logout", "logout", 204, true),
    ("get", "/api/v1/me", "me", 200, true),
    ("post", "/api/v1/channels", "create_route", 201, true),
    ("get", "/api/v1/channels", "list_route", 200, true),
    (
        "post",
        "/api/v1/channels/{channel_id}/messages",
        "post_route",
        201,
        true,
    ),
    (
        "get",
        "/api/v1/channels/{channel_id}/messages",
        "history_route",
        200,
        true,
    ),
];

#[actix_web::test]
async fn generated_contract_is_current_and_covers_every_registered_handler() {
    let generated = contract::generated_json();
    assert_eq!(
        generated,
        include_str!("../openapi.json"),
        "run: cargo run --bin generate-openapi > openapi.json"
    );
    let doc: Value = serde_json::from_str(&generated).unwrap();
    let documented: BTreeSet<_> = doc["paths"]
        .as_object()
        .unwrap()
        .iter()
        .flat_map(|(path, operations)| {
            operations
                .as_object()
                .unwrap()
                .keys()
                .map(move |method| (method.as_str().to_owned(), path.clone()))
        })
        .collect();
    let expected: BTreeSet<_> = INVENTORY
        .iter()
        .map(|(method, path, _, _, _)| (method.to_string(), path.to_string()))
        .collect();
    assert_eq!(documented, expected);
    let sources = [
        include_str!("../src/lib.rs"),
        include_str!("../src/channels.rs"),
        include_str!("../src/messages.rs"),
    ];
    assert!(sources[0].contains("web::scope(\"/api/v1\")"));
    let mut registered = Vec::new();
    let mut resource_count = 0;
    for source in sources {
        resource_count += source.matches("web::resource(\"").count();
        for (offset, _) in source.match_indices(".route(web::") {
            let prefix = &source[..offset];
            let resource = prefix
                .rsplit_once("web::resource(\"")
                .expect("route must belong to resource")
                .1
                .split('"')
                .next()
                .unwrap();
            let tail = &source[offset + ".route(web::".len()..];
            let (method, tail) = tail
                .split_once("().to(")
                .expect("standard method registration");
            let (handler, _) = tail.split_once(')').expect("handler registration");
            registered.push((
                method.to_owned(),
                format!("/api/v1{resource}"),
                handler.to_owned(),
            ));
        }
    }
    assert_eq!(
        resource_count, 6,
        "new Actix resources require an inventory entry"
    );
    registered.sort();
    let mut listed: Vec<_> = INVENTORY
        .iter()
        .map(|(method, path, handler, _, _)| {
            (method.to_string(), path.to_string(), handler.to_string())
        })
        .collect();
    listed.sort();
    assert_eq!(
        registered, listed,
        "Actix method/path registrations and contract inventory differ"
    );
    for &(method, path, _, success, protected) in INVENTORY {
        let operation = &doc["paths"][path][method];
        assert!(operation["responses"].get(success.to_string()).is_some());
        assert!(operation["responses"].get("500").is_some());
        if protected {
            assert_eq!(operation["security"][0]["bearer_auth"], json!([]));
            assert!(operation["responses"].get("401").is_some());
        } else {
            assert!(operation.get("security").is_none());
        }
        for (status, code) in [
            ("400", "bad_request"),
            ("401", "unauthorized"),
            ("404", "not_found"),
            ("409", "conflict"),
            ("500", "internal_error"),
        ] {
            if let Some(response) = operation["responses"].get(status) {
                assert_eq!(response["description"], format!("error.code = {code}"));
                assert_eq!(
                    response["content"]["application/json"]["schema"]["allOf"][0]["$ref"],
                    "#/components/schemas/ErrorBody"
                );
                assert_eq!(
                    response["content"]["application/json"]["schema"]["allOf"][1]["properties"]["error"]
                        ["properties"]["code"]["const"],
                    code
                );
            }
        }
    }
    assert_eq!(
        doc["components"]["schemas"]["ChannelType"]["enum"],
        json!(["text"])
    );
    assert_eq!(
        doc["components"]["schemas"]["User"]["properties"]["id"]["type"],
        "string"
    );
    assert_eq!(
        doc["components"]["schemas"]["Message"]["properties"]["id"]["type"],
        "string"
    );
    assert_eq!(
        doc["components"]["securitySchemes"]["bearer_auth"]["scheme"],
        "bearer"
    );
    let params = doc["paths"]["/api/v1/channels/{channel_id}/messages"]["get"]["parameters"]
        .as_array()
        .unwrap();
    let limit = params.iter().find(|p| p["name"] == "limit").unwrap();
    assert_eq!(limit["schema"]["minimum"], 1);
    assert_eq!(limit["schema"]["maximum"], 100);
}

#[actix_web::test]
async fn documented_security_and_error_shapes_match_http() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("contract.db").display()
    );
    let db = connect(&url).await.unwrap();
    let app = test::init_service(App::new().app_data(web::Data::new(db)).configure(routes)).await;
    let signup = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(json!({"username":"Alice", "password":"long password"}))
            .to_request(),
    )
    .await;
    assert_eq!(signup.status(), StatusCode::CREATED);
    let auth: Value = test::read_body_json(signup).await;
    let token = auth["access_token"].as_str().unwrap();
    for &(method, path, _, _, protected) in INVENTORY {
        if !protected {
            continue;
        }
        let path = path.replace("{channel_id}", "999999999999999");
        let request = if method == "get" {
            test::TestRequest::get()
        } else {
            test::TestRequest::post()
        };
        let response = test::call_service(&app, request.uri(&path).to_request()).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["error"]["code"], "unauthorized");
    }
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let me: Value = test::read_body_json(response).await;
    assert_eq!(me, auth["user"]);
    let channels = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/channels")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    let channels: Value = test::read_body_json(channels).await;
    let channel_id = channels["items"][0]["id"].as_str().unwrap();
    for &(method, path, handler, success, protected) in INVENTORY {
        if handler == "logout" {
            continue;
        }
        let path = path.replace("{channel_id}", channel_id);
        let mut request = if method == "get" {
            test::TestRequest::get()
        } else {
            test::TestRequest::post()
        };
        request = request.uri(&path);
        if protected {
            request = request.insert_header(("Authorization", format!("Bearer {token}")));
        }
        request = match handler {
            "signup" => {
                request.set_json(json!({"username":"ContractBob","password":"long password"}))
            }
            "login" => request.set_json(json!({"username":"Alice","password":"long password"})),
            "create_route" => request.set_json(json!({"name":"contract","type":"text"})),
            "post_route" => request.set_json(json!({"text":"contract message"})),
            _ => request,
        };
        let response = test::call_service(&app, request.to_request()).await;
        assert_eq!(response.status().as_u16(), success, "{method} {path}");
    }
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/logout")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = test::call_service(
        &app,
        test::TestRequest::get().uri("/does-not-exist").to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["error"]["code"], "not_found");
}
