#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};

use actix_web::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use actix_web::{App, http::StatusCode, test, web};
use common::{TestCtx, insert_message, make_tmp_uploads_dir};
use hamlet::{AttachmentStorage, configure_app, entity, generate_id, now_unix_micros};
use sea_orm::{ActiveModelTrait, Set};

const FULL_BYTES: &[u8] = b"private full image bytes";
const THUMB_BYTES: &[u8] = b"private thumbnail bytes";

async fn insert_attachment(
    ctx: &TestCtx,
    message_id: i64,
    storage_path: &str,
    thumbnail_storage_path: &str,
) -> i64 {
    let id = generate_id();
    entity::message_attachment::ActiveModel {
        id: Set(id),
        message_id: Set(message_id),
        position: Set(0),
        content_type: Set("image/jpeg".to_owned()),
        byte_size: Set(FULL_BYTES.len() as i64),
        width: Set(640),
        height: Set(480),
        storage_path: Set(storage_path.to_owned()),
        thumbnail_content_type: Set("image/webp".to_owned()),
        thumbnail_byte_size: Set(THUMB_BYTES.len() as i64),
        thumbnail_width: Set(160),
        thumbnail_height: Set(120),
        thumbnail_storage_path: Set(thumbnail_storage_path.to_owned()),
        created_at: Set(now_unix_micros()),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    id
}

fn write_file(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

#[actix_web::test]
async fn test_attachment_routes_require_auth() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let message_id = insert_message(&ctx.db, alice.user_id, ctx.channel_id, "photo").await;
    let private_dir = make_tmp_uploads_dir();
    write_file(&private_dir, "photos/full.jpg", FULL_BYTES);
    write_file(&private_dir, "thumbs/full.webp", THUMB_BYTES);
    let attachment_id =
        insert_attachment(&ctx, message_id, "photos/full.jpg", "thumbs/full.webp").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let full_req = test::TestRequest::get()
        .uri(&format!("/attachments/{attachment_id}"))
        .to_request();
    assert_eq!(
        test::call_service(&app, full_req).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let thumb_req = test::TestRequest::get()
        .uri(&format!("/attachments/{attachment_id}/thumbnail"))
        .to_request();
    assert_eq!(
        test::call_service(&app, thumb_req).await.status(),
        StatusCode::UNAUTHORIZED
    );

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_serves_full_and_thumbnail_from_rows_with_private_headers() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let message_id = insert_message(&ctx.db, alice.user_id, ctx.channel_id, "photo").await;
    let private_dir = make_tmp_uploads_dir();
    write_file(&private_dir, "photos/full.jpg", FULL_BYTES);
    write_file(&private_dir, "thumbs/full.webp", THUMB_BYTES);
    let attachment_id =
        insert_attachment(&ctx, message_id, "photos/full.jpg", "thumbs/full.webp").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let full_req = test::TestRequest::get()
        .uri(&format!("/attachments/{attachment_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let full_resp = test::call_service(&app, full_req).await;
    assert_eq!(full_resp.status(), StatusCode::OK);
    assert_eq!(full_resp.headers().get(CONTENT_TYPE).unwrap(), "image/jpeg");
    assert_eq!(
        full_resp.headers().get("X-Content-Type-Options").unwrap(),
        "nosniff"
    );
    assert!(
        full_resp
            .headers()
            .get(CACHE_CONTROL)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("private")
    );
    let full_body = test::read_body(full_resp).await;
    assert_eq!(full_body.as_ref(), FULL_BYTES);

    let thumb_req = test::TestRequest::get()
        .uri(&format!("/attachments/{attachment_id}/thumbnail"))
        .insert_header(alice.cookie_header())
        .to_request();
    let thumb_resp = test::call_service(&app, thumb_req).await;
    assert_eq!(thumb_resp.status(), StatusCode::OK);
    assert_eq!(
        thumb_resp.headers().get(CONTENT_TYPE).unwrap(),
        "image/webp"
    );
    assert_eq!(
        thumb_resp.headers().get("X-Content-Type-Options").unwrap(),
        "nosniff"
    );
    assert!(
        thumb_resp
            .headers()
            .get(CACHE_CONTROL)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("private")
    );
    let thumb_body = test::read_body(thumb_resp).await;
    assert_eq!(thumb_body.as_ref(), THUMB_BYTES);

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_missing_row_and_missing_file_return_not_found_without_serving_bytes() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let message_id = insert_message(&ctx.db, alice.user_id, ctx.channel_id, "photo").await;
    let private_dir = make_tmp_uploads_dir();
    write_file(&private_dir, "photos/existing.jpg", FULL_BYTES);
    let attachment_id = insert_attachment(
        &ctx,
        message_id,
        "photos/missing.jpg",
        "thumbs/missing.webp",
    )
    .await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let missing_row_req = test::TestRequest::get()
        .uri(&format!("/attachments/{}", generate_id()))
        .insert_header(alice.cookie_header())
        .to_request();
    let missing_row_resp = test::call_service(&app, missing_row_req).await;
    assert_eq!(missing_row_resp.status(), StatusCode::NOT_FOUND);
    let missing_row_body = test::read_body(missing_row_resp).await;
    assert!(
        !missing_row_body
            .windows(FULL_BYTES.len())
            .any(|w| w == FULL_BYTES)
    );

    let missing_file_req = test::TestRequest::get()
        .uri(&format!("/attachments/{attachment_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let missing_file_resp = test::call_service(&app, missing_file_req).await;
    assert_eq!(missing_file_resp.status(), StatusCode::NOT_FOUND);
    let missing_file_body = test::read_body(missing_file_resp).await;
    assert!(
        !missing_file_body
            .windows(FULL_BYTES.len())
            .any(|w| w == FULL_BYTES)
    );

    std::fs::remove_dir_all(&private_dir).ok();
}

#[actix_web::test]
async fn test_stored_path_traversal_is_rejected() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let message_id = insert_message(&ctx.db, alice.user_id, ctx.channel_id, "photo").await;
    let private_dir = make_tmp_uploads_dir();
    let secret_dir = make_tmp_uploads_dir();
    write_file(&secret_dir, "secret.jpg", FULL_BYTES);
    let secret_dir_name = secret_dir.file_name().unwrap().to_string_lossy();
    let traversal_path = format!("../{secret_dir_name}/secret.jpg");
    let attachment_id =
        insert_attachment(&ctx, message_id, &traversal_path, "thumbs/missing.webp").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/attachments/{attachment_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = test::read_body(resp).await;
    assert!(!body.windows(FULL_BYTES.len()).any(|w| w == FULL_BYTES));

    std::fs::remove_dir_all(&private_dir).ok();
    std::fs::remove_dir_all(&secret_dir).ok();
}

#[actix_web::test]
async fn test_attachment_storage_is_not_exposed_by_public_upload_mount() {
    let ctx = TestCtx::new().await;
    let alice = ctx.register("alice", "hunter2").await;
    let message_id = insert_message(&ctx.db, alice.user_id, ctx.channel_id, "photo").await;
    let public_dir = make_tmp_uploads_dir();
    let private_dir = public_dir.join("message-attachments");
    std::fs::create_dir_all(&private_dir).unwrap();
    write_file(&private_dir, "photos/private.jpg", FULL_BYTES);
    write_file(&private_dir, "thumbs/private.webp", THUMB_BYTES);
    write_file(&public_dir, "avatars/public.webp", b"public avatar");
    write_file(&public_dir, "emojis/public.webp", b"public emoji");
    let attachment_id = insert_attachment(
        &ctx,
        message_id,
        "photos/private.jpg",
        "thumbs/private.webp",
    )
    .await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(AttachmentStorage {
                dir: private_dir.clone(),
            }))
            .service(actix_files::Files::new(
                "/uploads/avatars",
                public_dir.join("avatars"),
            ))
            .service(actix_files::Files::new(
                "/uploads/emojis",
                public_dir.join("emojis"),
            ))
            .configure(|cfg| configure_app(cfg, ctx.deps())),
    )
    .await;

    let public_private_req = test::TestRequest::get()
        .uri("/uploads/message-attachments/photos/private.jpg")
        .insert_header(alice.cookie_header())
        .to_request();
    let public_private_resp = test::call_service(&app, public_private_req).await;
    assert_eq!(public_private_resp.status(), StatusCode::NOT_FOUND);
    let public_private_body = test::read_body(public_private_resp).await;
    assert!(
        !public_private_body
            .windows(FULL_BYTES.len())
            .any(|w| w == FULL_BYTES)
    );

    let avatar_req = test::TestRequest::get()
        .uri("/uploads/avatars/public.webp")
        .to_request();
    let avatar_resp = test::call_service(&app, avatar_req).await;
    assert_eq!(avatar_resp.status(), StatusCode::OK);
    let avatar_body = test::read_body(avatar_resp).await;
    assert_eq!(avatar_body.as_ref(), b"public avatar");

    let emoji_req = test::TestRequest::get()
        .uri("/uploads/emojis/public.webp")
        .to_request();
    let emoji_resp = test::call_service(&app, emoji_req).await;
    assert_eq!(emoji_resp.status(), StatusCode::OK);
    let emoji_body = test::read_body(emoji_resp).await;
    assert_eq!(emoji_body.as_ref(), b"public emoji");

    let private_req = test::TestRequest::get()
        .uri(&format!("/attachments/{attachment_id}"))
        .insert_header(alice.cookie_header())
        .to_request();
    let private_resp = test::call_service(&app, private_req).await;
    assert_eq!(private_resp.status(), StatusCode::OK);
    let private_body = test::read_body(private_resp).await;
    assert_eq!(private_body.as_ref(), FULL_BYTES);

    std::fs::remove_dir_all(&public_dir).ok();
}
