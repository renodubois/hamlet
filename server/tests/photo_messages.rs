#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::io::Cursor;
use std::path::{Path, PathBuf};

use actix_web::http::header::{CONTENT_TYPE, ContentType};
use actix_web::{App, http::StatusCode, test, web};
use common::{TestCtx, insert_message, make_tmp_uploads_dir};
use hamlet::{AttachmentStorage, configure_app, entity, generate_id, mentions, now_unix_micros};
use image::{ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};

struct MultipartPart {
    name: &'static str,
    filename: Option<&'static str>,
    content_type: Option<&'static str>,
    bytes: Vec<u8>,
}

fn text_part(value: &str) -> MultipartPart {
    field_part("text", value)
}

fn reply_to_part(value: &str) -> MultipartPart {
    field_part("reply_to_message_id", value)
}

fn field_part(name: &'static str, value: &str) -> MultipartPart {
    MultipartPart {
        name,
        filename: None,
        content_type: None,
        bytes: value.as_bytes().to_vec(),
    }
}

fn photo_part(bytes: Vec<u8>, content_type: &'static str) -> MultipartPart {
    MultipartPart {
        name: "photos",
        filename: Some("photo.png"),
        content_type: Some(content_type),
        bytes,
    }
}

async fn insert_attachment(db: &sea_orm::DatabaseConnection, message_id: i64) -> i64 {
    let id = generate_id();
    entity::message_attachment::ActiveModel {
        id: Set(id),
        message_id: Set(message_id),
        position: Set(0),
        content_type: Set("image/webp".to_owned()),
        byte_size: Set(12_345),
        width: Set(640),
        height: Set(480),
        storage_path: Set(format!("attachments/{id}/full.webp")),
        thumbnail_content_type: Set("image/webp".to_owned()),
        thumbnail_byte_size: Set(2_345),
        thumbnail_width: Set(320),
        thumbnail_height: Set(240),
        thumbnail_storage_path: Set(format!("attachments/{id}/thumb.webp")),
        created_at: Set(now_unix_micros()),
    }
    .insert(db)
    .await
    .unwrap();
    id
}

async fn insert_public_user(db: &sea_orm::DatabaseConnection, username: &str) -> i64 {
    let id = generate_id();
    entity::user::ActiveModel {
        id: Set(id),
        username: Set(username.to_owned()),
        display_name: Set(None),
        email: Set(Some(format!("{username}@example.test"))),
        email_verified: Set(false),
        avatar_path: Set(None),
        avatar_updated_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    id
}

async fn message_mention_rows(
    db: &sea_orm::DatabaseConnection,
    message_id: i64,
) -> Vec<entity::message_mention::Model> {
    entity::message_mention::Entity::find()
        .filter(entity::message_mention::Column::MessageId.eq(message_id))
        .order_by_asc(entity::message_mention::Column::Position)
        .all(db)
        .await
        .unwrap()
}

fn stored_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

fn multipart_payload(parts: Vec<MultipartPart>) -> (String, Vec<u8>) {
    let boundary = "hamlet-photo-test-boundary";
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match part.filename {
            Some(filename) => body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                    part.name, filename
                )
                .as_bytes(),
            ),
            None => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{}\"\r\n", part.name).as_bytes(),
            ),
        }
        if let Some(content_type) = part.content_type {
            body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
        }
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(&part.bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

fn assert_reply_metadata_null(message: &serde_json::Value) {
    assert_eq!(
        message.get("reply_to_message_id"),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(message.get("reply_to"), Some(&serde_json::Value::Null));
}

fn tiny_image(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    match format {
        ImageFormat::Jpeg => {
            image::DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([255, 0, 0])))
        }
        _ => image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            width,
            height,
            Rgba([255, 0, 0, 255]),
        )),
    }
    .write_to(&mut out, format)
    .unwrap();
    out.into_inner()
}

fn tiny_png(width: u32, height: u32) -> Vec<u8> {
    tiny_image(width, height, ImageFormat::Png)
}

fn tiny_jpeg(width: u32, height: u32) -> Vec<u8> {
    tiny_image(width, height, ImageFormat::Jpeg)
}

fn tiny_webp(width: u32, height: u32) -> Vec<u8> {
    tiny_image(width, height, ImageFormat::WebP)
}

fn animated_webp_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&22u32.to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(b"VP8X");
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.push(0b0000_0010);
    bytes.extend_from_slice(&[0; 9]);
    bytes
}

fn animated_gif_bytes() -> Vec<u8> {
    b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;".to_vec()
}

fn heic_bytes(brand: &[u8; 4]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&24u32.to_be_bytes());
    bytes.extend_from_slice(b"ftyp");
    bytes.extend_from_slice(brand);
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(brand);
    bytes.extend_from_slice(b"mif1");
    bytes
}

fn crc32(chunks: &[&[u8]]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for chunk in chunks {
        for byte in *chunk {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                if crc & 1 == 1 {
                    crc = (crc >> 1) ^ 0xedb8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
    }
    !crc
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = Vec::new();
    chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
    chunk.extend_from_slice(kind);
    chunk.extend_from_slice(data);
    chunk.extend_from_slice(&crc32(&[kind.as_slice(), data]).to_be_bytes());
    chunk
}

fn png_header_with_dimensions(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    bytes.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    // A tiny zlib stream with an empty DEFLATE block is enough for the PNG
    // decoder to recognize the file while letting the server reject by IHDR
    // dimensions before attempting a full decode.
    bytes.extend_from_slice(&png_chunk(
        b"IDAT",
        &[
            0x78, 0x01, 0x01, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01,
        ],
    ));
    bytes.extend_from_slice(&png_chunk(b"IEND", &[]));
    bytes
}

#[actix_web::test]
async fn test_json_text_only_message_create_returns_empty_attachments() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, ctx.deps()))).await;

    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header(ContentType::json())
        .insert_header(alice.cookie_header())
        .set_payload(serde_json::json!({"text": "still json"}).to_string())
        .to_request();
    let created: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;

    assert_eq!(created["text"], "still json");
    assert_eq!(created["attachments"], serde_json::json!([]));
}

#[actix_web::test]
async fn test_multipart_create_requires_auth() {
    let ctx = TestCtx::new().await;
    let private_dir = make_tmp_uploads_dir();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;
    let (content_type, body) = multipart_payload(vec![
        text_part("hello"),
        reply_to_part("123"),
        photo_part(tiny_png(4, 2), "image/png"),
    ]);

    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header((CONTENT_TYPE, content_type))
        .set_payload(body)
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::UNAUTHORIZED
    );

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_multipart_valid_photo_format_matrix_accepts_static_inputs() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let private_dir = make_tmp_uploads_dir();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    for (label, content_type, bytes) in [
        ("jpeg", "image/jpeg", tiny_jpeg(6, 4)),
        ("png", "image/png", tiny_png(6, 4)),
        ("webp", "image/webp", tiny_webp(6, 4)),
    ] {
        let (multipart_type, body) =
            multipart_payload(vec![text_part(label), photo_part(bytes, content_type)]);
        let req = test::TestRequest::post()
            .uri(&format!("/message/{}", ctx.channel_id))
            .insert_header((CONTENT_TYPE, multipart_type))
            .insert_header(alice.cookie_header())
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK, "{label} should be accepted");
        let created: serde_json::Value = test::read_body_json(resp).await;
        let attachment = &created["attachments"][0];
        assert_eq!(created["text"], label);
        assert_eq!(attachment["content_type"], "image/webp");
        assert_eq!(attachment["width"], 6);
        assert_eq!(attachment["height"], 4);
        assert_eq!(attachment["thumbnail_content_type"], "image/webp");
        assert_eq!(attachment["thumbnail_width"], 6);
        assert_eq!(attachment["thumbnail_height"], 4);
    }

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_multipart_text_plus_photo_creates_served_attachment_and_sse_payload() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let private_dir = make_tmp_uploads_dir();
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;
    let (content_type, body) = multipart_payload(vec![
        text_part("caption"),
        photo_part(tiny_png(8, 4), "image/png"),
    ]);

    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let created: serde_json::Value = test::read_body_json(resp).await;

    assert_eq!(created["text"], "caption");
    let attachment = &created["attachments"][0];
    let attachment_id = attachment["id"].as_i64().unwrap();
    assert_eq!(attachment["position"], 0);
    assert_eq!(attachment["content_type"], "image/webp");
    assert_eq!(attachment["width"], 8);
    assert_eq!(attachment["height"], 4);
    assert_eq!(attachment["thumbnail_content_type"], "image/webp");
    assert_eq!(attachment["thumbnail_width"], 8);
    assert_eq!(attachment["thumbnail_height"], 4);
    assert!(attachment["byte_size"].as_i64().unwrap() > 0);
    assert!(attachment["thumbnail_byte_size"].as_i64().unwrap() > 0);
    assert_eq!(attachment["url"], format!("/attachments/{attachment_id}"));
    assert_eq!(
        attachment["thumbnail_url"],
        format!("/attachments/{attachment_id}/thumbnail")
    );

    let row = entity::message_attachment::Entity::find_by_id(attachment_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert!(!row.storage_path.contains("photo.png"));
    let full_path = private_dir.join(&row.storage_path);
    let thumbnail_path = private_dir.join(&row.thumbnail_storage_path);
    assert!(full_path.exists());
    assert!(thumbnail_path.exists());
    assert!(std::fs::read(&full_path).unwrap().starts_with(b"RIFF"));
    assert!(std::fs::read(&thumbnail_path).unwrap().starts_with(b"RIFF"));

    let full_req = test::TestRequest::get()
        .uri(attachment["url"].as_str().unwrap())
        .insert_header(alice.cookie_header())
        .to_request();
    let full_resp = test::call_service(&app, full_req).await;
    assert_eq!(full_resp.status(), StatusCode::OK);
    assert_eq!(full_resp.headers().get(CONTENT_TYPE).unwrap(), "image/webp");
    let full_body = test::read_body(full_resp).await;
    assert!(full_body.starts_with(b"RIFF"));

    let thumb_req = test::TestRequest::get()
        .uri(attachment["thumbnail_url"].as_str().unwrap())
        .insert_header(alice.cookie_header())
        .to_request();
    let thumb_resp = test::call_service(&app, thumb_req).await;
    assert_eq!(thumb_resp.status(), StatusCode::OK);
    assert_eq!(
        thumb_resp.headers().get(CONTENT_TYPE).unwrap(),
        "image/webp"
    );
    let thumb_body = test::read_body(thumb_resp).await;
    assert!(thumb_body.starts_with(b"RIFF"));

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for photo message SSE")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"message\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{attachment_id}")));
    assert!(event_str.contains("\\\"attachments\\\":[{"));

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_multipart_photo_caption_mentions_persist_hydrate_history_and_sse() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let bob_id = insert_public_user(&ctx.db, "bob").await;
    let carol_id = insert_public_user(&ctx.db, "carol").await;
    let private_dir = make_tmp_uploads_dir();
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let text = format!("caption <@{bob_id}> repeat <@{bob_id}> and <@{carol_id}>");
    let (content_type, body) = multipart_payload(vec![
        text_part(&text),
        photo_part(tiny_png(8, 4), "image/png"),
    ]);
    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let created: serde_json::Value = test::read_body_json(resp).await;
    let created_id = created["id"].as_i64().unwrap();
    let attachment_id = created["attachments"][0]["id"].as_i64().unwrap();

    assert_eq!(created["text"], text);
    assert_eq!(created["attachments"].as_array().unwrap().len(), 1);
    assert_eq!(created["attachments"][0]["message_id"], created_id);
    assert_eq!(created["mentions"].as_array().unwrap().len(), 2);
    assert_eq!(created["mentions"][0]["id"], bob_id);
    assert_eq!(created["mentions"][0]["username"], "bob");
    assert_eq!(created["mentions"][1]["id"], carol_id);
    assert_eq!(created["mentions"][1]["username"], "carol");

    let mention_rows = message_mention_rows(&ctx.db, created_id).await;
    assert_eq!(mention_rows.len(), 2);
    assert_eq!(mention_rows[0].user_id, bob_id);
    assert_eq!(mention_rows[0].position, 0);
    assert_eq!(mention_rows[1].user_id, carol_id);
    assert_eq!(mention_rows[1].position, 1);

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for photo caption mention SSE")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"message\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{attachment_id}")));
    assert!(event_str.contains("\\\"attachments\\\":[{"));
    assert!(event_str.contains(&format!("\\\"id\\\":{bob_id}")));
    assert!(event_str.contains(&format!("\\\"id\\\":{carol_id}")));

    let history_req = test::TestRequest::get()
        .uri(&format!("/messages/{}", ctx.channel_id))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, history_req).await).await;
    let created_row = history
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == created_id)
        .unwrap();
    assert_eq!(created_row["attachments"][0]["id"], attachment_id);
    assert_eq!(created_row["mentions"][0]["id"], bob_id);
    assert_eq!(created_row["mentions"][1]["id"], carol_id);

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_thread_multipart_reply_mentions_persist_hydrate_thread_and_sse() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let bob_id = insert_public_user(&ctx.db, "bob").await;
    let root_id = insert_message(&ctx.db, alice.user_id, ctx.channel_id, "root").await;
    let private_dir = make_tmp_uploads_dir();
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let text = format!("multipart thread reply <@{bob_id}>");
    let (content_type, body) = multipart_payload(vec![text_part(&text)]);
    let req = test::TestRequest::post()
        .uri(&format!("/thread/{root_id}/reply"))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let created: serde_json::Value = test::read_body_json(resp).await;
    let reply_id = created["id"].as_i64().unwrap();

    assert_eq!(created["parent_id"], root_id);
    assert_eq!(created["text"], text);
    assert_eq!(created["mentions"].as_array().unwrap().len(), 1);
    assert_eq!(created["mentions"][0]["id"], bob_id);
    assert_eq!(created["mentions"][0]["username"], "bob");
    let mention_rows = message_mention_rows(&ctx.db, reply_id).await;
    assert_eq!(mention_rows.len(), 1);
    assert_eq!(mention_rows[0].user_id, bob_id);

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for multipart thread reply mention SSE")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"thread_reply_created\\\""));
    assert!(event_str.contains(&format!("\\\"id\\\":{reply_id}")));
    assert!(event_str.contains(&format!("\\\"id\\\":{bob_id}")));

    let req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(thread["replies"][0]["id"], reply_id);
    assert_eq!(thread["replies"][0]["mentions"][0]["id"], bob_id);

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_multipart_photo_caption_invalid_mentions_reject_without_orphans() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let private_dir = make_tmp_uploads_dir();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let mut over_cap_text = String::new();
    for index in 0..=mentions::MAX_UNIQUE_MENTIONS_PER_MESSAGE {
        let user_id = insert_public_user(&ctx.db, &format!("mention_cap_{index}")).await;
        over_cap_text.push_str(&format!("<@{user_id}> "));
    }

    for text in ["bad <@0>".to_owned(), over_cap_text] {
        let (content_type, body) = multipart_payload(vec![
            text_part(&text),
            photo_part(tiny_png(4, 4), "image/png"),
        ]);
        let req = test::TestRequest::post()
            .uri(&format!("/message/{}", ctx.channel_id))
            .insert_header((CONTENT_TYPE, content_type))
            .insert_header(alice.cookie_header())
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["error"]["kind"], "invalid_request");

        assert!(
            entity::message::Entity::find()
                .all(&ctx.db)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            entity::message_attachment::Entity::find()
                .all(&ctx.db)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            entity::message_mention::Entity::find()
                .all(&ctx.db)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(stored_files(&private_dir).is_empty());
    }

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_multipart_inline_reply_persists_reference_and_attachment() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let target_id =
        insert_message(&ctx.db, alice.user_id, ctx.channel_id, "photo reply target").await;
    let private_dir = make_tmp_uploads_dir();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;
    let (content_type, body) = multipart_payload(vec![
        text_part("photo inline reply"),
        reply_to_part(&target_id.to_string()),
        photo_part(tiny_png(4, 4), "image/png"),
    ]);

    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let created: serde_json::Value = test::read_body_json(resp).await;
    let created_id = created["id"].as_i64().unwrap();

    assert_eq!(created["parent_id"], serde_json::Value::Null);
    assert_eq!(created["reply_to_message_id"], target_id);
    assert_eq!(created["reply_to"]["id"], target_id);
    assert_eq!(created["reply_to"]["text"], "photo reply target");
    assert_eq!(created["attachments"].as_array().unwrap().len(), 1);

    let stored = entity::message::Entity::find_by_id(created_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.parent_id, None);
    assert_eq!(stored.reply_to_message_id, Some(target_id));

    let history_req = test::TestRequest::get()
        .uri(&format!("/messages/{}", ctx.channel_id))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, history_req).await).await;
    let created_row = history
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == created_id)
        .unwrap();
    assert_eq!(created_row["reply_to"]["id"], target_id);
    assert_eq!(created_row["attachments"].as_array().unwrap().len(), 1);

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_multipart_inline_reply_rejects_invalid_targets_with_clear_errors() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let other_channel_id = generate_id();
    entity::channel::ActiveModel {
        id: Set(other_channel_id),
        name: Set("random".to_owned()),
        position: Set(1),
        channel_type: Set("text".to_owned()),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    let same_channel_target =
        insert_message(&ctx.db, alice.user_id, ctx.channel_id, "target").await;
    let cross_channel_target =
        insert_message(&ctx.db, alice.user_id, other_channel_id, "cross").await;
    let thread_reply_target = generate_id();
    entity::message::ActiveModel {
        id: Set(thread_reply_target),
        user_id: Set(alice.user_id),
        channel_id: Set(ctx.channel_id),
        parent_id: Set(Some(same_channel_target)),
        reply_to_message_id: Set(None),
        created_at: Set(1_700_000_000_000_000),
        deleted_at: Set(None),
        text: Set("thread reply target".to_owned()),
        suppress_embeds: Set(false),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    let deleted_target = generate_id();
    entity::message::ActiveModel {
        id: Set(deleted_target),
        user_id: Set(alice.user_id),
        channel_id: Set(ctx.channel_id),
        parent_id: Set(None),
        reply_to_message_id: Set(None),
        created_at: Set(1_700_000_001_000_000),
        deleted_at: Set(Some(1_700_000_002_000_000)),
        text: Set(String::new()),
        suppress_embeds: Set(true),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    let private_dir = make_tmp_uploads_dir();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    for (raw_target, status, kind) in [
        (
            "not-a-number".to_owned(),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            "0".to_owned(),
            StatusCode::BAD_REQUEST,
            "reply_target_unsafe",
        ),
        (
            "-1".to_owned(),
            StatusCode::BAD_REQUEST,
            "reply_target_unsafe",
        ),
        (
            "9007199254740992".to_owned(),
            StatusCode::BAD_REQUEST,
            "reply_target_unsafe",
        ),
        (
            "9000000000000000".to_owned(),
            StatusCode::NOT_FOUND,
            "reply_target_not_found",
        ),
        (
            cross_channel_target.to_string(),
            StatusCode::BAD_REQUEST,
            "reply_target_cross_channel",
        ),
        (
            thread_reply_target.to_string(),
            StatusCode::BAD_REQUEST,
            "reply_target_not_top_level",
        ),
        (
            deleted_target.to_string(),
            StatusCode::BAD_REQUEST,
            "reply_target_deleted",
        ),
    ] {
        let (content_type, body) = multipart_payload(vec![
            text_part("bad multipart target"),
            reply_to_part(&raw_target),
        ]);
        let req = test::TestRequest::post()
            .uri(&format!("/message/{}", ctx.channel_id))
            .insert_header((CONTENT_TYPE, content_type))
            .insert_header(alice.cookie_header())
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), status);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["error"]["kind"], kind);
    }

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_multipart_thread_reply_rejects_inline_reply_target() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message(&ctx.db, alice.user_id, ctx.channel_id, "root").await;
    let target_id = insert_message(&ctx.db, alice.user_id, ctx.channel_id, "target").await;
    let private_dir = make_tmp_uploads_dir();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let (content_type, body) = multipart_payload(vec![
        text_part("thread reply with inline target"),
        reply_to_part(&target_id.to_string()),
    ]);
    let req = test::TestRequest::post()
        .uri(&format!("/thread/{root_id}/reply"))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["kind"], "thread_inline_reply_not_allowed");

    let (content_type, body) = multipart_payload(vec![
        text_part("ordinary thread reply"),
        reply_to_part("null"),
    ]);
    let req = test::TestRequest::post()
        .uri(&format!("/thread/{root_id}/reply"))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let created: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(created["parent_id"], root_id);
    assert_reply_metadata_null(&created);

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_multipart_photo_only_message_succeeds_and_history_reloads_attachment() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let private_dir = make_tmp_uploads_dir();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;
    let (content_type, body) = multipart_payload(vec![photo_part(tiny_png(3, 5), "image/png")]);

    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let created: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let message_id = created["id"].as_i64().unwrap();
    let attachment_id = created["attachments"][0]["id"].as_i64().unwrap();
    assert_eq!(created["text"], "");

    let history_req = test::TestRequest::get()
        .uri(&format!("/messages/{}", ctx.channel_id))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, history_req).await).await;
    assert_eq!(history[0]["id"], message_id);
    assert_eq!(history[0]["attachments"][0]["id"], attachment_id);
    assert_eq!(
        history[0]["attachments"][0]["url"],
        format!("/attachments/{attachment_id}")
    );

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_multipart_inline_reply_text_plus_photo_hydrates_reference_and_sse_payload() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let bob = ctx.register("bob", "hunter2").await;
    let target_id = insert_message(&ctx.db, bob.user_id, ctx.channel_id, "target caption").await;
    let private_dir = make_tmp_uploads_dir();
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;
    let (content_type, body) = multipart_payload(vec![
        text_part("reply caption"),
        reply_to_part(&target_id.to_string()),
        photo_part(tiny_png(8, 4), "image/png"),
    ]);

    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let created: serde_json::Value = test::read_body_json(resp).await;
    let created_id = created["id"].as_i64().unwrap();
    let attachment_id = created["attachments"][0]["id"].as_i64().unwrap();

    assert_eq!(created["parent_id"], serde_json::Value::Null);
    assert_eq!(created["reply_to_message_id"], target_id);
    assert_eq!(created["reply_to"]["id"], target_id);
    assert_eq!(created["reply_to"]["text"], "target caption");
    assert_eq!(created["reply_to"]["username"], "bob");
    assert_eq!(created["text"], "reply caption");
    assert_eq!(created["attachments"][0]["message_id"], created_id);

    let stored = entity::message::Entity::find_by_id(created_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.parent_id, None);
    assert_eq!(stored.reply_to_message_id, Some(target_id));

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for multipart inline reply SSE")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"message\\\""));
    assert!(event_str.contains(&format!("\\\"reply_to_message_id\\\":{target_id}")));
    assert!(event_str.contains("\\\"reply_to\\\":{"));
    assert!(event_str.contains(&format!("\\\"id\\\":{attachment_id}")));

    let history_req = test::TestRequest::get()
        .uri(&format!("/messages/{}", ctx.channel_id))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, history_req).await).await;
    let created_history = history
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["id"] == created_id)
        .unwrap();
    assert_eq!(created_history["reply_to"]["id"], target_id);
    assert_eq!(created_history["attachments"][0]["id"], attachment_id);

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_multipart_photo_only_inline_reply_to_attachment_target_hydrates_metadata() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let target_id = insert_message(&ctx.db, alice.user_id, ctx.channel_id, "").await;
    insert_attachment(&ctx.db, target_id).await;
    let private_dir = make_tmp_uploads_dir();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;
    let (content_type, body) = multipart_payload(vec![
        reply_to_part(&target_id.to_string()),
        photo_part(tiny_png(3, 5), "image/png"),
    ]);

    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let created: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let created_id = created["id"].as_i64().unwrap();
    let attachment_id = created["attachments"][0]["id"].as_i64().unwrap();

    assert_eq!(created["text"], "");
    assert_eq!(created["reply_to_message_id"], target_id);
    assert_eq!(created["reply_to"]["id"], target_id);
    assert_eq!(created["reply_to"]["text"], "");
    assert_eq!(created["reply_to"]["attachment_count"], 1);
    assert_eq!(created["attachments"][0]["message_id"], created_id);

    let history_req = test::TestRequest::get()
        .uri(&format!("/messages/{}", ctx.channel_id))
        .insert_header(alice.cookie_header())
        .to_request();
    let history: serde_json::Value =
        test::read_body_json(test::call_service(&app, history_req).await).await;
    let created_history = history
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["id"] == created_id)
        .unwrap();
    assert_eq!(created_history["reply_to"]["attachment_count"], 1);
    assert_eq!(created_history["attachments"][0]["id"], attachment_id);

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_multipart_thread_reply_text_plus_photo_creates_attachment_and_sse_payload() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message(&ctx.db, alice.user_id, ctx.channel_id, "root").await;
    let private_dir = make_tmp_uploads_dir();
    let mut rx = ctx.broadcaster.test_client();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;
    let (content_type, body) = multipart_payload(vec![
        text_part("thread caption"),
        photo_part(tiny_png(8, 4), "image/png"),
    ]);

    let req = test::TestRequest::post()
        .uri(&format!("/thread/{root_id}/reply"))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let created: serde_json::Value = test::read_body_json(resp).await;

    assert_eq!(created["parent_id"], root_id);
    assert_eq!(created["channel_id"], ctx.channel_id);
    assert_eq!(created["text"], "thread caption");
    let attachment = &created["attachments"][0];
    let attachment_id = attachment["id"].as_i64().unwrap();
    assert_eq!(attachment["position"], 0);
    assert_eq!(attachment["message_id"], created["id"]);
    assert_eq!(attachment["content_type"], "image/webp");
    assert_eq!(attachment["width"], 8);
    assert_eq!(attachment["height"], 4);

    let row = entity::message_attachment::Entity::find_by_id(attachment_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .unwrap();
    let full_path = private_dir.join(&row.storage_path);
    let thumbnail_path = private_dir.join(&row.thumbnail_storage_path);
    assert!(full_path.exists());
    assert!(thumbnail_path.exists());
    assert!(std::fs::read(&full_path).unwrap().starts_with(b"RIFF"));
    assert!(std::fs::read(&thumbnail_path).unwrap().starts_with(b"RIFF"));

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for thread reply photo SSE")
        .expect("broadcast channel closed");
    let event_str = format!("{:?}", event);
    assert!(event_str.contains("kind\\\":\\\"thread_reply_created\\\""));
    assert!(event_str.contains("thread caption"));
    assert!(event_str.contains(&format!("\\\"id\\\":{attachment_id}")));
    assert!(event_str.contains("\\\"attachments\\\":[{"));
    assert!(event_str.contains(&format!("\\\"root_message_id\\\":{}", root_id)));
    assert!(event_str.contains("\\\"reply_count\\\":1"));

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_multipart_photo_only_thread_reply_succeeds_and_thread_reloads_attachment() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let root_id = insert_message(&ctx.db, alice.user_id, ctx.channel_id, "root").await;
    let private_dir = make_tmp_uploads_dir();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;
    let (content_type, body) = multipart_payload(vec![photo_part(tiny_png(3, 5), "image/png")]);

    let req = test::TestRequest::post()
        .uri(&format!("/thread/{root_id}/reply"))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let created: serde_json::Value =
        test::read_body_json(test::call_service(&app, req).await).await;
    let reply_id = created["id"].as_i64().unwrap();
    let attachment_id = created["attachments"][0]["id"].as_i64().unwrap();
    assert_eq!(created["text"], "");
    assert_eq!(created["parent_id"], root_id);

    let thread_req = test::TestRequest::get()
        .uri(&format!("/thread/{root_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thread: serde_json::Value =
        test::read_body_json(test::call_service(&app, thread_req).await).await;
    assert_eq!(thread["root"]["attachments"], serde_json::json!([]));
    assert_eq!(thread["replies"][0]["id"], reply_id);
    assert_eq!(thread["replies"][0]["attachments"][0]["id"], attachment_id);
    assert_eq!(
        thread["replies"][0]["attachments"][0]["url"],
        format!("/attachments/{attachment_id}")
    );

    let participated_req = test::TestRequest::get()
        .uri("/threads/participated")
        .insert_header(alice.cookie_header())
        .to_request();
    let previews: serde_json::Value =
        test::read_body_json(test::call_service(&app, participated_req).await).await;
    assert_eq!(
        previews[0]["recent_replies"][0]["attachments"][0]["id"],
        attachment_id
    );

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_multipart_photo_validation_failures_return_clear_error_kinds() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let private_dir = make_tmp_uploads_dir();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let (content_type, body) = multipart_payload(vec![text_part("   ")]);
    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["kind"], "message_content_required");

    let too_many = (0..5)
        .map(|_| photo_part(tiny_png(1, 1), "image/png"))
        .collect();
    let (content_type, body) = multipart_payload(too_many);
    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["kind"], "too_many_attachments");

    for (label, bytes, declared_type) in [
        ("invalid bytes", b"not a png".to_vec(), "image/png"),
        ("mime mismatch", tiny_png(1, 1), "image/jpeg"),
        (
            "zero dimensions",
            png_header_with_dimensions(0, 1),
            "image/png",
        ),
        ("heic", heic_bytes(b"heic"), "image/heic"),
        ("heif", heic_bytes(b"heif"), "image/heif"),
        ("animated gif", animated_gif_bytes(), "image/gif"),
        ("animated webp", animated_webp_bytes(), "image/webp"),
    ] {
        let (content_type, body) = multipart_payload(vec![photo_part(bytes, declared_type)]);
        let req = test::TestRequest::post()
            .uri(&format!("/message/{}", ctx.channel_id))
            .insert_header((CONTENT_TYPE, content_type))
            .insert_header(alice.cookie_header())
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "{label} should be rejected"
        );
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["error"]["kind"], "unsupported_photo", "{label}");
    }

    let (content_type, body) =
        multipart_payload(vec![photo_part(vec![0; 10 * 1024 * 1024 + 1], "image/png")]);
    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["kind"], "payload_too_large");

    let (content_type, body) = multipart_payload(vec![photo_part(
        png_header_with_dimensions(6_000, 5_000),
        "image/png",
    )]);
    let req = test::TestRequest::post()
        .uri(&format!("/message/{}", ctx.channel_id))
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header(alice.cookie_header())
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["kind"], "photo_dimensions_too_large");

    std::fs::remove_dir_all(&private_dir).ok();
}
