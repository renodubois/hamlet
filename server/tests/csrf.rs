#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::io::Cursor;

use actix_web::body::MessageBody;
use actix_web::http::header::ContentType;
use actix_web::{App, http::StatusCode, test};
use common::{AuthSession, TestCtx};
use hamlet::{CSRF_COOKIE, CSRF_HEADER, auth, configure_app};

const ORIGIN: &str = "https://chat.example.test";
const BOUNDARY: &str = "----hamlet-csrf-test-boundary";

fn cookie_header_with_csrf(session: &AuthSession, csrf_token: &str) -> (String, String) {
    (
        "Cookie".to_owned(),
        format!(
            "{}={}; {CSRF_COOKIE}={csrf_token}",
            auth::SESSION_COOKIE,
            session.token
        ),
    )
}

fn multipart_content_type() -> String {
    format!("multipart/form-data; boundary={BOUNDARY}")
}

fn multipart_body(field: &str, filename: &str, content_type: &str, bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{field}\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

fn make_png(side: u32) -> Vec<u8> {
    use image::{Rgb, RgbImage};

    let mut img = RgbImage::new(side, side);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = Rgb([
            ((x * 255 / side.max(1)) & 0xff) as u8,
            ((y * 255 / side.max(1)) & 0xff) as u8,
            128,
        ]);
    }
    let mut buf = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut buf, image::ImageFormat::Png)
        .unwrap();
    buf.into_inner()
}

async fn read_csrf_token_response<B>(resp: actix_web::dev::ServiceResponse<B>) -> String
where
    B: MessageBody + 'static,
{
    assert_eq!(resp.status(), StatusCode::OK);
    let set_cookie = resp
        .headers()
        .get(actix_web::http::header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("csrf set-cookie header");
    assert!(set_cookie.contains(CSRF_COOKIE));
    assert!(!set_cookie.to_ascii_lowercase().contains("httponly"));

    let body = test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["token"].as_str().expect("csrf token").to_owned()
}

#[actix_web::test]
async fn csrf_endpoint_requires_an_authenticated_session() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get().uri("/csrf").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn login_from_a_fresh_browser_does_not_require_csrf() {
    let ctx = TestCtx::new().await;
    auth::register_user(&ctx.db, "alice", "hunter2", None)
        .await
        .unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri("/login")
        .insert_header(("Origin", ORIGIN))
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"username": "alice", "password": "hunter2"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn logout_with_a_stale_or_current_cookie_does_not_require_csrf() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri("/logout")
        .insert_header(("Origin", ORIGIN))
        .insert_header(alice.cookie_header())
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn browser_json_write_rejects_missing_csrf_token() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header(("Origin", ORIGIN))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "hello"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn browser_json_write_rejects_invalid_csrf_token() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header(("Origin", ORIGIN))
        .insert_header(ContentType::json())
        .insert_header(cookie_header_with_csrf(&alice, "invalid-token"))
        .insert_header((CSRF_HEADER, "invalid-token"))
        .set_payload(serde_json::json!({"text": "hello"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn browser_json_write_accepts_valid_csrf_token() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;
    let csrf_req = test::TestRequest::get()
        .uri("/csrf")
        .insert_header(alice.cookie_header())
        .to_request();
    let csrf_token = read_csrf_token_response(test::call_service(&app, csrf_req).await).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header(("Origin", ORIGIN))
        .insert_header(ContentType::json())
        .insert_header(cookie_header_with_csrf(&alice, &csrf_token))
        .insert_header((CSRF_HEADER, csrf_token))
        .set_payload(serde_json::json!({"text": "hello"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn browser_multipart_write_rejects_missing_csrf_token() {
    let ctx = TestCtx::with_avatar_storage().await;
    let alice = ctx.register("alice", "hunter2").await;
    let uploads_dir = ctx.uploads_dir.clone().unwrap();
    let app = test::init_service(
        App::new()
            .app_data(ctx.avatar_storage())
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/me/avatar")
        .insert_header(("Origin", ORIGIN))
        .insert_header(("content-type", multipart_content_type()))
        .insert_header(alice.cookie_header())
        .set_payload(multipart_body(
            "file",
            "avatar.png",
            "image/png",
            &make_png(8),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn browser_multipart_write_rejects_invalid_csrf_token() {
    let ctx = TestCtx::with_avatar_storage().await;
    let alice = ctx.register("alice", "hunter2").await;
    let uploads_dir = ctx.uploads_dir.clone().unwrap();
    let app = test::init_service(
        App::new()
            .app_data(ctx.avatar_storage())
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/me/avatar")
        .insert_header(("Origin", ORIGIN))
        .insert_header(("content-type", multipart_content_type()))
        .insert_header(cookie_header_with_csrf(&alice, "invalid-token"))
        .insert_header((CSRF_HEADER, "invalid-token"))
        .set_payload(multipart_body(
            "file",
            "avatar.png",
            "image/png",
            &make_png(8),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn browser_multipart_write_accepts_valid_csrf_token() {
    let ctx = TestCtx::with_avatar_storage().await;
    let alice = ctx.register("alice", "hunter2").await;
    let uploads_dir = ctx.uploads_dir.clone().unwrap();
    let app = test::init_service(
        App::new()
            .app_data(ctx.avatar_storage())
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;
    let csrf_req = test::TestRequest::get()
        .uri("/csrf")
        .insert_header(alice.cookie_header())
        .to_request();
    let csrf_token = read_csrf_token_response(test::call_service(&app, csrf_req).await).await;

    let req = test::TestRequest::post()
        .uri("/me/avatar")
        .insert_header(("Origin", ORIGIN))
        .insert_header(("content-type", multipart_content_type()))
        .insert_header(cookie_header_with_csrf(&alice, &csrf_token))
        .insert_header((CSRF_HEADER, csrf_token))
        .set_payload(multipart_body(
            "file",
            "avatar.png",
            "image/png",
            &make_png(8),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    std::fs::remove_dir_all(&uploads_dir).ok();
}
