#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::io::Cursor;
use std::path::PathBuf;

use actix_web::{App, http::StatusCode, test, web};
use hamlet::{AvatarStorage, auth, broadcast::Broadcaster, configure_app, entity, generate_id};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

const BOUNDARY: &str = "----hamlet-test-boundary-XYZ";

fn tmp_uploads_dir() -> PathBuf {
    let id = generate_id();
    let dir = std::env::temp_dir().join(format!("hamlet-test-uploads-{id}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
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

fn multipart_content_type() -> String {
    format!("multipart/form-data; boundary={BOUNDARY}")
}

#[actix_web::test]
async fn test_upload_avatar_sets_url_and_writes_file() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let uploads_dir = tmp_uploads_dir();
    let storage = web::Data::new(AvatarStorage {
        dir: uploads_dir.clone(),
    });
    let app = test::init_service(
        App::new()
            .app_data(storage)
            .service(actix_files::Files::new("/uploads", uploads_dir.clone()))
            .configure(|cfg| {
                configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
            }),
    )
    .await;

    let body = multipart_body("file", "avatar.png", "image/png", &make_png(64));
    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::post()
        .uri("/me/avatar")
        .insert_header(("content-type", multipart_content_type()))
        .insert_header((name, value))
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "status {:?}", resp.status());

    let body = test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let url = json["avatar_url"].as_str().expect("avatar_url present");
    assert!(url.starts_with(&format!("/uploads/avatars/{}.webp?v=", user.id)));

    let written = uploads_dir
        .join("avatars")
        .join(format!("{}.webp", user.id));
    assert!(written.exists(), "webp file should be written");
    let bytes = std::fs::read(&written).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");

    // static route should serve the bytes
    let clean_url = url.split('?').next().unwrap();
    let get = test::TestRequest::get().uri(clean_url).to_request();
    let get_resp = test::call_service(&app, get).await;
    assert!(get_resp.status().is_success());
    let served = test::read_body(get_resp).await;
    assert_eq!(&served[0..4], b"RIFF");
    assert_eq!(&served[8..12], b"WEBP");

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_upload_avatar_requires_auth() {
    let (db, _) = common::setup_db().await;
    let uploads_dir = tmp_uploads_dir();
    let storage = web::Data::new(AvatarStorage {
        dir: uploads_dir.clone(),
    });
    let app = test::init_service(App::new().app_data(storage).configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let body = multipart_body("file", "avatar.png", "image/png", &make_png(8));
    let req = test::TestRequest::post()
        .uri("/me/avatar")
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
async fn test_upload_avatar_rejects_wrong_mime() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let uploads_dir = tmp_uploads_dir();
    let storage = web::Data::new(AvatarStorage {
        dir: uploads_dir.clone(),
    });
    let app = test::init_service(App::new().app_data(storage).configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let body = multipart_body("file", "bad.txt", "text/plain", b"not an image");
    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::post()
        .uri("/me/avatar")
        .insert_header(("content-type", multipart_content_type()))
        .insert_header((name, value))
        .set_payload(body)
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_upload_avatar_rejects_oversized() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let uploads_dir = tmp_uploads_dir();
    let storage = web::Data::new(AvatarStorage {
        dir: uploads_dir.clone(),
    });
    let app = test::init_service(App::new().app_data(storage).configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    // 3 MiB of PNG-looking bytes — larger than the 2 MiB cap.
    let oversized = vec![0u8; 3 * 1024 * 1024];
    let body = multipart_body("file", "big.png", "image/png", &oversized);
    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::post()
        .uri("/me/avatar")
        .insert_header(("content-type", multipart_content_type()))
        .insert_header((name, value))
        .set_payload(body)
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_delete_avatar_clears_url_and_file() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    // Pre-seed an uploaded avatar on disk + in DB.
    let uploads_dir = tmp_uploads_dir();
    let avatars_dir = uploads_dir.join("avatars");
    std::fs::create_dir_all(&avatars_dir).unwrap();
    let filename = format!("{}.webp", user.id);
    let file_path = avatars_dir.join(&filename);
    std::fs::write(&file_path, b"not real webp but here").unwrap();
    let mut m: entity::user::ActiveModel = user.clone().into();
    m.avatar_path = Set(Some(format!("avatars/{filename}")));
    m.avatar_updated_at = Set(Some(1234567890));
    m.update(&db).await.unwrap();

    let storage = web::Data::new(AvatarStorage {
        dir: uploads_dir.clone(),
    });
    let app = test::init_service(App::new().app_data(storage).configure(|cfg| {
        configure_app(cfg, web::Data::new(db), web::Data::from(Broadcaster::new()))
    }))
    .await;

    let (name, value) = common::session_cookie_header(&session.token);
    let req = test::TestRequest::delete()
        .uri("/me/avatar")
        .insert_header((name, value))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body = test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["avatar_url"].is_null());
    assert!(!file_path.exists(), "file should be removed from disk");

    std::fs::remove_dir_all(&uploads_dir).ok();
}

#[actix_web::test]
async fn test_me_and_messages_carry_avatar_url() {
    let (db, chan_id) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();

    let uploads_dir = tmp_uploads_dir();
    let storage = web::Data::new(AvatarStorage {
        dir: uploads_dir.clone(),
    });
    let app = test::init_service(App::new().app_data(storage).configure(|cfg| {
        configure_app(
            cfg,
            web::Data::new(db.clone()),
            web::Data::from(Broadcaster::new()),
        )
    }))
    .await;

    // 1. Upload avatar.
    let body = multipart_body("file", "avatar.png", "image/png", &make_png(32));
    let (name, value) = common::session_cookie_header(&session.token);
    let up = test::TestRequest::post()
        .uri("/me/avatar")
        .insert_header(("content-type", multipart_content_type()))
        .insert_header((name.clone(), value.clone()))
        .set_payload(body)
        .to_request();
    assert!(test::call_service(&app, up).await.status().is_success());

    // 2. /me includes avatar_url.
    let me_req = test::TestRequest::get()
        .uri("/me")
        .insert_header((name.clone(), value.clone()))
        .to_request();
    let me_resp = test::call_service(&app, me_req).await;
    let me_body = test::read_body(me_resp).await;
    let me_json: serde_json::Value = serde_json::from_slice(&me_body).unwrap();
    assert!(me_json["avatar_url"].as_str().is_some());

    // 3. POSTing a message returns avatar_url.
    let msg_req = test::TestRequest::post()
        .uri(&format!("/message/{chan_id}"))
        .insert_header(actix_web::http::header::ContentType::json())
        .insert_header((name.clone(), value.clone()))
        .set_payload(serde_json::json!({"text": "hi"}).to_string())
        .to_request();
    let msg_resp = test::call_service(&app, msg_req).await;
    assert!(msg_resp.status().is_success());
    let msg_body = test::read_body(msg_resp).await;
    let msg_json: serde_json::Value = serde_json::from_slice(&msg_body).unwrap();
    assert!(msg_json["avatar_url"].as_str().is_some());

    // 4. GET /messages/{id} rows carry avatar_url.
    let list_req = test::TestRequest::get()
        .uri(&format!("/messages/{chan_id}"))
        .insert_header((name, value))
        .to_request();
    let list_resp = test::call_service(&app, list_req).await;
    let list_body = test::read_body(list_resp).await;
    let list_json: serde_json::Value = serde_json::from_slice(&list_body).unwrap();
    let rows = list_json.as_array().unwrap();
    assert!(!rows.is_empty());
    assert!(rows[0]["avatar_url"].as_str().is_some());

    std::fs::remove_dir_all(&uploads_dir).ok();
}

// Sanity: the `credential` + `session` joins the seeded data still work after the user entity
// picked up new columns. Regression guard — the entity change is the whole point of this file
// so keep it lightweight.
#[actix_web::test]
async fn test_new_user_has_null_avatar() {
    let (db, _) = common::setup_db().await;
    let user = auth::register_user(&db, "alice", "hunter2", None)
        .await
        .unwrap();
    let row = entity::user::Entity::find()
        .filter(entity::user::Column::Id.eq(user.id))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(row.avatar_path.is_none());
    assert!(row.avatar_updated_at.is_none());
}
