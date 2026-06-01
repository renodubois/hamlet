#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::io::Cursor;
use std::time::Duration;

use actix_web::{App, http::StatusCode, http::header::ContentType, test};
use common::TestCtx;
use hamlet::{configure_app, entity, generate_id};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

const BOUNDARY: &str = "----hamlet-emoji-test-boundary";

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

fn make_jpeg(side: u32) -> Vec<u8> {
    use image::{Rgb, RgbImage};
    let img = RgbImage::from_pixel(side, side, Rgb([180, 90, 30]));
    let mut buf = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut buf, image::ImageFormat::Jpeg)
        .unwrap();
    buf.into_inner()
}

fn make_webp(side: u32) -> Vec<u8> {
    use image::{Rgb, RgbImage};
    let img = RgbImage::from_pixel(side, side, Rgb([40, 120, 220]));
    let mut buf = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut buf, image::ImageFormat::WebP)
        .unwrap();
    buf.into_inner()
}

fn make_static_gif() -> Vec<u8> {
    vec![
        0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xff, 0xff,
        0xff, 0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00,
        0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3b,
    ]
}

fn make_animated_gif() -> Vec<u8> {
    let mut gif = make_static_gif();
    gif.pop();
    gif.extend_from_slice(&[
        0x21, 0xf9, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
        0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3b,
    ]);
    gif
}

fn make_animated_webp() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&22u32.to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(b"VP8X");
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.push(0b0000_0010);
    bytes.extend_from_slice(&[0, 0, 0]);
    bytes.extend_from_slice(&[0, 0, 0]);
    bytes.extend_from_slice(&[0, 0, 0]);
    bytes
}

fn multipart_content_type() -> String {
    format!("multipart/form-data; boundary={BOUNDARY}")
}

async fn insert_emoji(
    ctx: &TestCtx,
    user_id: i64,
    name: &str,
    animated: bool,
    deleted_at: Option<i64>,
) -> entity::emoji::Model {
    let id = generate_id();
    entity::emoji::ActiveModel {
        id: Set(id),
        image_path: Set(format!("emojis/{id}.webp")),
        name: Set(name.to_owned()),
        normalized_name: Set(name.to_ascii_lowercase()),
        animated: Set(animated),
        created_by_user_id: Set(user_id),
        created_at: Set(1_700_000_000),
        updated_at: Set(1_700_000_010),
        deleted_at: Set(deleted_at),
    }
    .insert(&ctx.db)
    .await
    .unwrap()
}

fn emoji_multipart_body(name: Option<&str>, file: Option<(&str, &str, &[u8])>) -> Vec<u8> {
    let mut body = Vec::new();
    if let Some(name) = name {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"name\"\r\n\r\n");
        body.extend_from_slice(name.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    if let Some((filename, content_type, bytes)) = file {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

#[actix_web::test]
async fn test_list_emojis_requires_auth() {
    let ctx = TestCtx::new().await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get().uri("/emojis").to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn test_list_emojis_empty_initially() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::get()
        .uri("/emojis")
        .insert_header(alice.cookie_header())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    assert_eq!(json, serde_json::json!([]));
}

#[actix_web::test]
async fn test_list_emojis_includes_active_and_deleted_response_shape() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;

    let active_id = generate_id();
    entity::emoji::ActiveModel {
        id: Set(active_id),
        image_path: Set(format!("emojis/{active_id}.webp")),
        name: Set("PartyParrot".to_owned()),
        normalized_name: Set("partyparrot".to_owned()),
        animated: Set(true),
        created_by_user_id: Set(alice.user_id),
        created_at: Set(1_700_000_000),
        updated_at: Set(1_700_000_010),
        deleted_at: Set(None),
    }
    .insert(&ctx.db)
    .await
    .unwrap();

    let deleted_id = generate_id();
    entity::emoji::ActiveModel {
        id: Set(deleted_id),
        image_path: Set(format!("emojis/{deleted_id}.webp")),
        name: Set("Retired".to_owned()),
        normalized_name: Set("retired".to_owned()),
        animated: Set(false),
        created_by_user_id: Set(alice.user_id),
        created_at: Set(1_700_000_020),
        updated_at: Set(1_700_000_030),
        deleted_at: Set(Some(1_700_000_040)),
    }
    .insert(&ctx.db)
    .await
    .unwrap();

    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;
    let req = test::TestRequest::get()
        .uri("/emojis")
        .insert_header(alice.cookie_header())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    let rows = json.as_array().unwrap();
    assert_eq!(rows.len(), 2);

    assert_eq!(rows[0]["id"], active_id);
    assert_eq!(rows[0]["name"], "PartyParrot");
    assert_eq!(
        rows[0]["image_url"],
        format!("/uploads/emojis/{active_id}.webp?v=1700000010")
    );
    assert_eq!(rows[0]["animated"], true);
    assert_eq!(rows[0]["created_by_user_id"], alice.user_id);
    assert_eq!(rows[0]["created_at"], 1_700_000_000);
    assert_eq!(rows[0]["updated_at"], 1_700_000_010);
    assert!(rows[0]["deleted_at"].is_null());

    assert_eq!(rows[1]["id"], deleted_id);
    assert_eq!(rows[1]["deleted_at"], 1_700_000_040);
}

#[actix_web::test]
async fn test_upload_static_png_creates_emoji_and_writes_normalized_asset() {
    let ctx = TestCtx::with_avatar_storage().await;
    let alice = ctx.register("alice", "hunter2").await;
    let uploads_dir = ctx.uploads_dir.clone().unwrap();
    let app = test::init_service(
        App::new()
            .service(actix_files::Files::new("/uploads", uploads_dir.clone()))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let body = emoji_multipart_body(
        Some("Party_Parrot"),
        Some(("party.png", "image/png", &make_png(80))),
    );
    let req = test::TestRequest::post()
        .uri("/emojis")
        .insert_header(("content-type", multipart_content_type()))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    let id = json["id"].as_i64().unwrap();
    assert_eq!(json["name"], "Party_Parrot");
    assert_eq!(
        json["image_url"],
        format!(
            "/uploads/emojis/{id}.webp?v={}",
            json["updated_at"].as_i64().unwrap()
        )
    );
    assert_eq!(json["animated"], false);
    assert_eq!(json["created_by_user_id"], alice.user_id);
    assert!(json["created_at"].as_i64().unwrap() > 0);
    assert_eq!(json["created_at"], json["updated_at"]);
    assert!(json["deleted_at"].is_null());

    let row = entity::emoji::Entity::find_by_id(id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.name, "Party_Parrot");
    assert_eq!(row.normalized_name, "party_parrot");
    assert_eq!(row.image_path, format!("emojis/{id}.webp"));
    assert!(!row.animated);
    assert_eq!(row.created_by_user_id, alice.user_id);
    assert!(row.deleted_at.is_none());

    let written = uploads_dir.join("emojis").join(format!("{id}.webp"));
    assert!(written.exists(), "webp file should be written");
    let bytes = std::fs::read(&written).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!(decoded.width(), 256);
    assert_eq!(decoded.height(), 256);

    let clean_url = json["image_url"]
        .as_str()
        .unwrap()
        .split('?')
        .next()
        .unwrap();
    let get = test::TestRequest::get().uri(clean_url).to_request();
    let get_resp = test::call_service(&app, get).await;
    assert!(get_resp.status().is_success());

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_upload_static_jpeg_and_webp_are_accepted() {
    let ctx = TestCtx::with_avatar_storage().await;
    let alice = ctx.register("alice", "hunter2").await;
    let uploads_dir = ctx.uploads_dir.clone().unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    for (name, filename, content_type, bytes) in [
        ("JpegOk", "emoji.jpg", "image/jpeg", make_jpeg(32)),
        ("WebpOk", "emoji.webp", "image/webp", make_webp(32)),
    ] {
        let body = emoji_multipart_body(Some(name), Some((filename, content_type, &bytes)));
        let req = test::TestRequest::post()
            .uri("/emojis")
            .insert_header(("content-type", multipart_content_type()))
            .insert_header(alice.cookie_header())
            .set_payload(body)
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::CREATED
        );
    }

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_upload_animated_gif_and_webp_preserve_original_assets() {
    let ctx = TestCtx::with_avatar_storage().await;
    let alice = ctx.register("alice", "hunter2").await;
    let uploads_dir = ctx.uploads_dir.clone().unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    for (name, filename, content_type, bytes, extension) in [
        (
            "DanceGif",
            "dance.gif",
            "image/gif",
            make_animated_gif(),
            "gif",
        ),
        (
            "DanceWebp",
            "dance.webp",
            "image/webp",
            make_animated_webp(),
            "webp",
        ),
    ] {
        let body = emoji_multipart_body(Some(name), Some((filename, content_type, &bytes)));
        let req = test::TestRequest::post()
            .uri("/emojis")
            .insert_header(("content-type", multipart_content_type()))
            .insert_header(alice.cookie_header())
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
        let id = json["id"].as_i64().unwrap();
        assert_eq!(json["animated"], true);
        assert_eq!(
            json["image_url"],
            format!(
                "/uploads/emojis/{id}.{extension}?v={}",
                json["updated_at"].as_i64().unwrap()
            )
        );

        let row = entity::emoji::Entity::find_by_id(id)
            .one(&ctx.db)
            .await
            .unwrap()
            .unwrap();
        assert!(row.animated);
        assert_eq!(row.image_path, format!("emojis/{id}.{extension}"));

        let written = uploads_dir.join("emojis").join(format!("{id}.{extension}"));
        assert_eq!(std::fs::read(&written).unwrap(), bytes);
    }

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_upload_emoji_rejects_type_signature_confusion_and_static_gif() {
    let ctx = TestCtx::with_avatar_storage().await;
    let alice = ctx.register("alice", "hunter2").await;
    let uploads_dir = ctx.uploads_dir.clone().unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    for (name, filename, content_type, bytes) in [
        ("WrongPng", "dance.png", "image/png", make_animated_gif()),
        ("WrongGif", "dance.gif", "image/gif", make_webp(8)),
        ("StaticGif", "still.gif", "image/gif", make_static_gif()),
        ("WrongWebp", "dance.webp", "image/webp", make_png(8)),
    ] {
        let body = emoji_multipart_body(Some(name), Some((filename, content_type, &bytes)));
        let req = test::TestRequest::post()
            .uri("/emojis")
            .insert_header(("content-type", multipart_content_type()))
            .insert_header(alice.cookie_header())
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
        assert_eq!(json["error"]["kind"], "unsupported_emoji_file");
    }

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_upload_emoji_requires_auth() {
    let ctx = TestCtx::with_avatar_storage().await;
    let uploads_dir = ctx.uploads_dir.clone().unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let body = emoji_multipart_body(
        Some("party"),
        Some(("party.png", "image/png", &make_png(8))),
    );
    let req = test::TestRequest::post()
        .uri("/emojis")
        .insert_header(("content-type", multipart_content_type()))
        .set_payload(body)
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_upload_emoji_validation_failures_return_clear_errors() {
    let ctx = TestCtx::with_avatar_storage().await;
    let alice = ctx.register("alice", "hunter2").await;
    let uploads_dir = ctx.uploads_dir.clone().unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let cases: Vec<(Vec<u8>, StatusCode, &str)> = vec![
        (
            emoji_multipart_body(None, Some(("party.png", "image/png", &make_png(8)))),
            StatusCode::BAD_REQUEST,
            "emoji_name_required",
        ),
        (
            emoji_multipart_body(
                Some("bad-name"),
                Some(("party.png", "image/png", &make_png(8))),
            ),
            StatusCode::BAD_REQUEST,
            "invalid_emoji_name",
        ),
        (
            emoji_multipart_body(Some("party"), None),
            StatusCode::BAD_REQUEST,
            "emoji_file_required",
        ),
        (
            emoji_multipart_body(
                Some("party"),
                Some(("party.txt", "text/plain", b"not an image")),
            ),
            StatusCode::BAD_REQUEST,
            "unsupported_emoji_file",
        ),
    ];

    for (body, status, kind) in cases {
        let req = test::TestRequest::post()
            .uri("/emojis")
            .insert_header(("content-type", multipart_content_type()))
            .insert_header(alice.cookie_header())
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), status);
        let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
        assert_eq!(json["error"]["kind"], kind);
        assert!(json["error"]["message"].as_str().unwrap().contains("emoji"));
    }

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_upload_emoji_rejects_oversized_file() {
    let ctx = TestCtx::with_avatar_storage().await;
    let alice = ctx.register("alice", "hunter2").await;
    let uploads_dir = ctx.uploads_dir.clone().unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let oversized = vec![0u8; 3 * 1024 * 1024];
    let body = emoji_multipart_body(Some("party"), Some(("party.png", "image/png", &oversized)));
    let req = test::TestRequest::post()
        .uri("/emojis")
        .insert_header(("content-type", multipart_content_type()))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_upload_emoji_rejects_duplicate_active_name_case_insensitively() {
    let ctx = TestCtx::with_avatar_storage().await;
    let alice = ctx.register("alice", "hunter2").await;
    let uploads_dir = ctx.uploads_dir.clone().unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let first = emoji_multipart_body(Some("KEKW"), Some(("kekw.png", "image/png", &make_png(8))));
    let first_req = test::TestRequest::post()
        .uri("/emojis")
        .insert_header(("content-type", multipart_content_type()))
        .insert_header(alice.cookie_header())
        .set_payload(first)
        .to_request();
    assert_eq!(
        test::call_service(&app, first_req).await.status(),
        StatusCode::CREATED
    );

    let duplicate =
        emoji_multipart_body(Some("kekW"), Some(("kekw.png", "image/png", &make_png(8))));
    let dup_req = test::TestRequest::post()
        .uri("/emojis")
        .insert_header(("content-type", multipart_content_type()))
        .insert_header(alice.cookie_header())
        .set_payload(duplicate)
        .to_request();
    let resp = test::call_service(&app, dup_req).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    assert_eq!(json["error"]["kind"], "emoji_name_taken");

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_delete_soft_deletes_emoji_keeps_asset_and_broadcasts() {
    let ctx = TestCtx::with_avatar_storage().await;
    let alice = ctx.register("alice", "hunter2").await;
    let uploads_dir = ctx.uploads_dir.clone().unwrap();
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let body = emoji_multipart_body(
        Some("party"),
        Some(("party.png", "image/png", &make_png(8))),
    );
    let create_req = test::TestRequest::post()
        .uri("/emojis")
        .insert_header(("content-type", multipart_content_type()))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let create_resp = test::call_service(&app, create_req).await;
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created: serde_json::Value =
        serde_json::from_slice(&test::read_body(create_resp).await).unwrap();
    let id = created["id"].as_i64().unwrap();
    let written = uploads_dir.join("emojis").join(format!("{id}.webp"));
    assert!(
        written.exists(),
        "uploaded asset should exist before delete"
    );
    let _ = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for create broadcast");

    let delete_req = test::TestRequest::delete()
        .uri(&format!("/emojis/{id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let delete_resp = test::call_service(&app, delete_req).await;
    assert!(delete_resp.status().is_success());
    let deleted: serde_json::Value =
        serde_json::from_slice(&test::read_body(delete_resp).await).unwrap();
    assert_eq!(deleted["id"], id);
    assert!(deleted["deleted_at"].as_i64().unwrap() > 0);
    assert!(
        written.exists(),
        "soft delete must not remove uploaded asset"
    );

    let row = entity::emoji::Entity::find_by_id(id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.id, id);
    assert!(row.deleted_at.is_some());

    let list_req = test::TestRequest::get()
        .uri("/emojis")
        .insert_header(alice.cookie_header())
        .to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert!(list_resp.status().is_success());
    let list: serde_json::Value =
        serde_json::from_slice(&test::read_body(list_resp).await).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], id);
    assert!(list[0]["deleted_at"].as_i64().unwrap() > 0);

    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for delete broadcast")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"emoji_deleted\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{id}")));

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_restore_clears_deleted_at_and_conflicts_when_name_taken() {
    let ctx = TestCtx::with_avatar_storage().await;
    let alice = ctx.register("alice", "hunter2").await;
    let uploads_dir = ctx.uploads_dir.clone().unwrap();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let first_body = emoji_multipart_body(
        Some("party"),
        Some(("party.png", "image/png", &make_png(8))),
    );
    let first_req = test::TestRequest::post()
        .uri("/emojis")
        .insert_header(("content-type", multipart_content_type()))
        .insert_header(alice.cookie_header())
        .set_payload(first_body)
        .to_request();
    let first_resp = test::call_service(&app, first_req).await;
    assert_eq!(first_resp.status(), StatusCode::CREATED);
    let first: serde_json::Value =
        serde_json::from_slice(&test::read_body(first_resp).await).unwrap();
    let first_id = first["id"].as_i64().unwrap();

    let delete_first = test::TestRequest::delete()
        .uri(&format!("/emojis/{first_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert!(
        test::call_service(&app, delete_first)
            .await
            .status()
            .is_success()
    );

    let replacement_body = emoji_multipart_body(
        Some("PARTY"),
        Some(("party.png", "image/png", &make_png(8))),
    );
    let replacement_req = test::TestRequest::post()
        .uri("/emojis")
        .insert_header(("content-type", multipart_content_type()))
        .insert_header(alice.cookie_header())
        .set_payload(replacement_body)
        .to_request();
    let replacement_resp = test::call_service(&app, replacement_req).await;
    assert_eq!(replacement_resp.status(), StatusCode::CREATED);
    let replacement: serde_json::Value =
        serde_json::from_slice(&test::read_body(replacement_resp).await).unwrap();
    let replacement_id = replacement["id"].as_i64().unwrap();

    let conflict_req = test::TestRequest::post()
        .uri(&format!("/emojis/{first_id}/restore"))
        .insert_header(alice.cookie_header())
        .to_request();
    let conflict_resp = test::call_service(&app, conflict_req).await;
    assert_eq!(conflict_resp.status(), StatusCode::CONFLICT);
    let conflict: serde_json::Value =
        serde_json::from_slice(&test::read_body(conflict_resp).await).unwrap();
    assert_eq!(conflict["error"]["kind"], "emoji_name_taken");

    let delete_replacement = test::TestRequest::delete()
        .uri(&format!("/emojis/{replacement_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    assert!(
        test::call_service(&app, delete_replacement)
            .await
            .status()
            .is_success()
    );

    let restore_req = test::TestRequest::post()
        .uri(&format!("/emojis/{first_id}/restore"))
        .insert_header(alice.cookie_header())
        .to_request();
    let restore_resp = test::call_service(&app, restore_req).await;
    assert!(restore_resp.status().is_success());
    let restored: serde_json::Value =
        serde_json::from_slice(&test::read_body(restore_resp).await).unwrap();
    assert_eq!(restored["id"], first_id);
    assert!(restored["deleted_at"].is_null());

    let row = entity::emoji::Entity::find_by_id(first_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert!(row.deleted_at.is_none());

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_rename_emoji_updates_name_fields_preserves_asset_state_and_broadcasts() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let original = insert_emoji(&ctx, alice.user_id, "Party", true, Some(1_700_000_050)).await;
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::patch()
        .uri(&format!("/emojis/{}", original.id))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"name": "Renamed_Party"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();

    assert_eq!(json["id"], original.id);
    assert_eq!(json["name"], "Renamed_Party");
    assert_eq!(
        json["image_url"],
        format!(
            "/uploads/emojis/{}.webp?v={}",
            original.id,
            json["updated_at"].as_i64().unwrap()
        )
    );
    assert_eq!(json["animated"], true);
    assert_eq!(json["created_by_user_id"], alice.user_id);
    assert_eq!(json["created_at"], original.created_at);
    assert_eq!(json["deleted_at"], 1_700_000_050);
    assert!(json["updated_at"].as_i64().unwrap() >= original.updated_at);

    let row = entity::emoji::Entity::find_by_id(original.id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.name, "Renamed_Party");
    assert_eq!(row.normalized_name, "renamed_party");
    assert_eq!(row.image_path, original.image_path);
    assert_eq!(row.animated, original.animated);
    assert_eq!(row.created_by_user_id, original.created_by_user_id);
    assert_eq!(row.created_at, original.created_at);
    assert_eq!(row.deleted_at, original.deleted_at);

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for emoji_updated")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"emoji_updated\\\""));
    assert!(event_str.contains("Renamed_Party"));
    assert!(event_str.contains(&original.id.to_string()));
}

#[actix_web::test]
async fn test_rename_emoji_requires_auth_and_valid_name() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let original = insert_emoji(&ctx, alice.user_id, "Party", false, None).await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let unauth = test::TestRequest::patch()
        .uri(&format!("/emojis/{}", original.id))
        .insert_header(ContentType::json())
        .set_payload(serde_json::json!({"name": "Renamed"}).to_string())
        .to_request();
    assert_eq!(
        test::call_service(&app, unauth).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let invalid = test::TestRequest::patch()
        .uri(&format!("/emojis/{}", original.id))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"name": "bad-name"}).to_string())
        .to_request();
    let resp = test::call_service(&app, invalid).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    assert_eq!(json["error"]["kind"], "invalid_emoji_name");
}

#[actix_web::test]
async fn test_rename_emoji_rejects_duplicate_active_name_case_insensitively() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let original = insert_emoji(&ctx, alice.user_id, "Party", false, None).await;
    insert_emoji(&ctx, alice.user_id, "KEKW", false, None).await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::patch()
        .uri(&format!("/emojis/{}", original.id))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"name": "kekW"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let json: serde_json::Value = serde_json::from_slice(&test::read_body(resp).await).unwrap();
    assert_eq!(json["error"]["kind"], "emoji_name_taken");

    let unchanged = entity::emoji::Entity::find_by_id(original.id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(unchanged.name, "Party");
    assert_eq!(unchanged.normalized_name, "party");
}
