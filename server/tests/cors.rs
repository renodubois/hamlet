#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use actix_web::{
    App,
    http::{StatusCode, header},
    test, web,
};
use common::TestCtx;
use hamlet::{Config, CorsConfig, configure_app, cors_middleware};

fn config_with_cors(cors: CorsConfig) -> Config {
    let mut config = Config::from_env();
    config.cors = cors;
    config
}

#[actix_web::test]
async fn allowed_production_origin_gets_credentialed_cors_response() {
    let ctx = TestCtx::new().await;
    let config = config_with_cors(CorsConfig {
        allowed_origins: vec!["https://chat.example.com".to_owned()],
        allow_localhost_origins: false,
    });
    let app = test::init_service(
        App::new()
            .wrap(cors_middleware(&config.cors))
            .app_data(web::Data::new(config))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/config")
        .insert_header((header::ORIGIN, "https://chat.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "https://chat.example.com"
    );
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .unwrap(),
        "true"
    );
}

#[actix_web::test]
async fn allowed_preflight_uses_restricted_methods_and_headers() {
    let ctx = TestCtx::new().await;
    let config = config_with_cors(CorsConfig {
        allowed_origins: vec!["https://chat.example.com".to_owned()],
        allow_localhost_origins: false,
    });
    let app = test::init_service(
        App::new()
            .wrap(cors_middleware(&config.cors))
            .app_data(web::Data::new(config))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri("/login")
        .insert_header((header::ORIGIN, "https://chat.example.com"))
        .insert_header((header::ACCESS_CONTROL_REQUEST_METHOD, "POST"))
        .insert_header((
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "content-type,x-hamlet-csrf",
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "https://chat.example.com"
    );
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .unwrap(),
        "true"
    );
    let methods = resp
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_METHODS)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(methods.contains("POST"));
    assert!(methods.contains("PUT"));
    assert!(!methods.contains("TRACE"));

    let headers = resp
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
        .unwrap()
        .to_str()
        .unwrap()
        .to_ascii_lowercase();
    assert!(headers.contains("content-type"));
    assert!(headers.contains("x-hamlet-csrf"));
    assert!(!headers.contains("authorization"));
}

#[actix_web::test]
async fn disallowed_origin_does_not_get_allow_origin_header() {
    let ctx = TestCtx::new().await;
    let config = config_with_cors(CorsConfig {
        allowed_origins: vec!["https://chat.example.com".to_owned()],
        allow_localhost_origins: false,
    });
    let app = test::init_service(
        App::new()
            .wrap(cors_middleware(&config.cors))
            .app_data(web::Data::new(config))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/config")
        .insert_header((header::ORIGIN, "https://evil.example.com"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

#[actix_web::test]
async fn localhost_origin_remains_allowed_for_development_config() {
    let ctx = TestCtx::new().await;
    let config = config_with_cors(CorsConfig {
        allowed_origins: Vec::new(),
        allow_localhost_origins: true,
    });
    let app = test::init_service(
        App::new()
            .wrap(cors_middleware(&config.cors))
            .app_data(web::Data::new(config))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/config")
        .insert_header((header::ORIGIN, "http://127.0.0.1:1422"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "http://127.0.0.1:1422"
    );
}
