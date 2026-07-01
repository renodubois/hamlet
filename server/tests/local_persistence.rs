#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Cursor;
use std::path::{Path, PathBuf};

use actix_web::http::header::{CONTENT_TYPE, ContentType};
use actix_web::{App, http::StatusCode, test, web};
use hamlet::broadcast::Broadcaster;
use hamlet::voice::{VoiceConfig, VoiceState};
use hamlet::{
    AppDeps, AttachmentStorage, AvatarStorage, Config, DefaultChannelBootstrapOutcome,
    EmbedFetcher, EmojiStorage, ServerSettings, auth, bootstrap_default_channels, configure_app,
    connect_database, entity, generate_id,
};
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};

const AVATAR_BOUNDARY: &str = "----hamlet-persistence-avatar-boundary";
const EMOJI_BOUNDARY: &str = "----hamlet-persistence-emoji-boundary";
const PHOTO_BOUNDARY: &str = "----hamlet-persistence-photo-boundary";

struct TempRoot(PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct MultipartPart {
    name: &'static str,
    filename: Option<&'static str>,
    content_type: Option<&'static str>,
    bytes: Vec<u8>,
}

#[actix_web::test]
async fn file_backed_database_preserves_feature_surface_after_reconnect_and_reinit() {
    let root = tmp_root();
    let _cleanup = TempRoot(root.clone());
    let data_dir = root.join("data");
    let config = config_for_default_database_dir(&root, &data_dir);
    prepare_storage_dirs(&config);

    let db = connect_database(&config).await.unwrap();
    assert_eq!(
        bootstrap_default_channels(&db).await.unwrap(),
        DefaultChannelBootstrapOutcome::Created
    );

    let user = auth::register_user(&db, "durable_alice", "hunter2", None)
        .await
        .unwrap();
    let session = auth::create_session(&db, user.id).await.unwrap();
    let auth_cookie = format!("{}={}", auth::SESSION_COOKIE, session.token);

    let uploaded_avatar_url: String;
    let channel_id: i64;
    let root_message_id: i64;
    let inline_reply_id: i64;
    let thread_reply_id: i64;
    let photo_message_id: i64;
    let attachment_id: i64;
    let attachment_url: String;
    let attachment_thumbnail_url: String;
    let embed_id: i64;
    let emoji_id: i64;
    let emoji_url: String;

    {
        let deps = app_deps(db.clone(), &config.uploads_dir);
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(AvatarStorage {
                    dir: config.uploads_dir.clone(),
                }))
                .app_data(web::Data::new(AttachmentStorage {
                    dir: config.message_attachments_dir.clone(),
                }))
                .service(actix_files::Files::new(
                    "/uploads/avatars",
                    config.uploads_dir.join("avatars"),
                ))
                .service(actix_files::Files::new(
                    "/uploads/emojis",
                    config.uploads_dir.join("emojis"),
                ))
                .configure(|cfg| configure_app(cfg, deps.clone())),
        )
        .await;

        let req = test::TestRequest::put()
            .uri("/me")
            .insert_header(ContentType::json())
            .insert_header(("Cookie", auth_cookie.clone()))
            .set_payload(serde_json::json!({"display_name": "Durable Alice"}).to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let profile: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(profile["display_name"], "Durable Alice");

        let (content_type, body) = multipart_payload(
            AVATAR_BOUNDARY,
            vec![file_part(
                "file",
                "avatar.png",
                "image/png",
                make_png(32, 24),
            )],
        );
        let req = test::TestRequest::post()
            .uri("/me/avatar")
            .insert_header((CONTENT_TYPE, content_type))
            .insert_header(("Cookie", auth_cookie.clone()))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let avatar_profile: serde_json::Value = test::read_body_json(resp).await;
        uploaded_avatar_url = avatar_profile["avatar_url"].as_str().unwrap().to_owned();
        assert!(uploaded_avatar_url.starts_with(&format!("/uploads/avatars/{}.webp?v=", user.id)));

        let (content_type, body) = multipart_payload(
            EMOJI_BOUNDARY,
            vec![
                field_part("name", "DurableWave"),
                file_part("file", "wave.png", "image/png", make_png(20, 20)),
            ],
        );
        let req = test::TestRequest::post()
            .uri("/emojis")
            .insert_header((CONTENT_TYPE, content_type))
            .insert_header(("Cookie", auth_cookie.clone()))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let emoji: serde_json::Value = test::read_body_json(resp).await;
        emoji_id = emoji["id"].as_i64().unwrap();
        emoji_url = emoji["image_url"].as_str().unwrap().to_owned();
        assert_eq!(emoji["name"], "DurableWave");
        assert_eq!(emoji["created_by_user_id"], user.id);

        let req = test::TestRequest::post()
            .uri("/channel")
            .insert_header(ContentType::json())
            .insert_header(("Cookie", auth_cookie.clone()))
            .set_payload(serde_json::json!({"name": "durable-room"}).to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let channel: serde_json::Value = test::read_body_json(resp).await;
        channel_id = channel["id"].as_i64().unwrap();
        assert_eq!(channel["name"], "durable-room");
        assert_eq!(channel["type"], "text");

        let req = test::TestRequest::post()
            .uri(&format!("/message/{channel_id}"))
            .insert_header(ContentType::json())
            .insert_header(("Cookie", auth_cookie.clone()))
            .set_payload(
                serde_json::json!({"text": "root survives with https://example.invalid/card"})
                    .to_string(),
            )
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let root_message: serde_json::Value = test::read_body_json(resp).await;
        root_message_id = root_message["id"].as_i64().unwrap();

        embed_id = insert_embed(&db, root_message_id).await;

        let req = test::TestRequest::post()
            .uri(&format!("/message/{root_message_id}/reactions"))
            .insert_header(ContentType::json())
            .insert_header(("Cookie", auth_cookie.clone()))
            .set_payload(serde_json::json!({"kind": "native", "emoji": "👍"}).to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let req = test::TestRequest::post()
            .uri(&format!("/message/{root_message_id}/reactions"))
            .insert_header(ContentType::json())
            .insert_header(("Cookie", auth_cookie.clone()))
            .set_payload(serde_json::json!({"kind": "custom", "emoji_id": emoji_id}).to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let req = test::TestRequest::post()
            .uri(&format!("/message/{channel_id}"))
            .insert_header(ContentType::json())
            .insert_header(("Cookie", auth_cookie.clone()))
            .set_payload(
                serde_json::json!({
                    "text": "inline reply survives",
                    "reply_to_message_id": root_message_id
                })
                .to_string(),
            )
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let inline_reply: serde_json::Value = test::read_body_json(resp).await;
        inline_reply_id = inline_reply["id"].as_i64().unwrap();
        assert_eq!(inline_reply["reply_to"]["id"], root_message_id);

        let req = test::TestRequest::post()
            .uri(&format!("/thread/{root_message_id}/reply"))
            .insert_header(ContentType::json())
            .insert_header(("Cookie", auth_cookie.clone()))
            .set_payload(serde_json::json!({"text": "thread reply survives"}).to_string())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let thread_reply: serde_json::Value = test::read_body_json(resp).await;
        thread_reply_id = thread_reply["id"].as_i64().unwrap();
        assert_eq!(thread_reply["parent_id"], root_message_id);

        let (content_type, body) = multipart_payload(
            PHOTO_BOUNDARY,
            vec![
                field_part("text", "photo attachment survives"),
                file_part("photos", "photo.png", "image/png", make_png(18, 12)),
            ],
        );
        let req = test::TestRequest::post()
            .uri(&format!("/message/{channel_id}"))
            .insert_header((CONTENT_TYPE, content_type))
            .insert_header(("Cookie", auth_cookie.clone()))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let photo_message: serde_json::Value = test::read_body_json(resp).await;
        photo_message_id = photo_message["id"].as_i64().unwrap();
        let attachment = &photo_message["attachments"][0];
        attachment_id = attachment["id"].as_i64().unwrap();
        attachment_url = attachment["url"].as_str().unwrap().to_owned();
        attachment_thumbnail_url = attachment["thumbnail_url"].as_str().unwrap().to_owned();
    }

    db.close().await.unwrap();

    let reconnected = connect_database(&config).await.unwrap();
    assert_eq!(
        bootstrap_default_channels(&reconnected).await.unwrap(),
        DefaultChannelBootstrapOutcome::SkippedExistingChannels
    );

    {
        let deps = app_deps(reconnected.clone(), &config.uploads_dir);
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(AvatarStorage {
                    dir: config.uploads_dir.clone(),
                }))
                .app_data(web::Data::new(AttachmentStorage {
                    dir: config.message_attachments_dir.clone(),
                }))
                .service(actix_files::Files::new(
                    "/uploads/avatars",
                    config.uploads_dir.join("avatars"),
                ))
                .service(actix_files::Files::new(
                    "/uploads/emojis",
                    config.uploads_dir.join("emojis"),
                ))
                .configure(|cfg| configure_app(cfg, deps.clone())),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/me")
            .insert_header(("Cookie", auth_cookie.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let profile: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(profile["id"], user.id);
        assert_eq!(profile["username"], "durable_alice");
        assert_eq!(profile["display_name"], "Durable Alice");
        assert_eq!(profile["avatar_url"], uploaded_avatar_url);

        let req = test::TestRequest::get()
            .uri(asset_path_without_query(&uploaded_avatar_url))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let avatar_bytes = test::read_body(resp).await;
        assert_webp_bytes(&avatar_bytes);

        let req = test::TestRequest::get()
            .uri("/channels")
            .insert_header(("Cookie", auth_cookie.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let channels: serde_json::Value = test::read_body_json(resp).await;
        let channels = channels.as_array().unwrap();
        assert!(channels.iter().any(|channel| {
            channel["id"].as_i64() == Some(channel_id)
                && channel["name"] == "durable-room"
                && channel["type"] == "text"
        }));
        assert!(
            channels
                .iter()
                .any(|channel| { channel["name"] == "general" && channel["type"] == "text" })
        );
        assert!(
            channels
                .iter()
                .any(|channel| { channel["name"] == "voice" && channel["type"] == "voice" })
        );

        let req = test::TestRequest::get()
            .uri(&format!("/messages/{channel_id}"))
            .insert_header(("Cookie", auth_cookie.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let messages: serde_json::Value = test::read_body_json(resp).await;
        let messages = messages.as_array().unwrap();

        let root_message = json_row_by_id(messages, root_message_id);
        assert_eq!(
            root_message["text"],
            "root survives with https://example.invalid/card"
        );
        assert_eq!(root_message["avatar_url"], uploaded_avatar_url);
        assert_eq!(root_message["thread_summary"]["reply_count"], 1);
        assert_embed_present(root_message, embed_id);
        assert_native_reaction_present(root_message, "👍");
        assert_custom_reaction_present(root_message, emoji_id, &emoji_url);

        let inline_reply = json_row_by_id(messages, inline_reply_id);
        assert_eq!(inline_reply["text"], "inline reply survives");
        assert_eq!(inline_reply["reply_to_message_id"], root_message_id);
        assert_eq!(inline_reply["reply_to"]["id"], root_message_id);
        assert_eq!(inline_reply["reply_to"]["username"], "durable_alice");
        assert_eq!(inline_reply["reply_to"]["display_name"], "Durable Alice");

        let photo_message = json_row_by_id(messages, photo_message_id);
        assert_eq!(photo_message["text"], "photo attachment survives");
        let attachment = &photo_message["attachments"][0];
        assert_eq!(attachment["id"], attachment_id);
        assert_eq!(attachment["url"], attachment_url);
        assert_eq!(attachment["thumbnail_url"], attachment_thumbnail_url);
        assert_eq!(attachment["content_type"], "image/webp");

        let req = test::TestRequest::get()
            .uri(&format!("/thread/{root_message_id}"))
            .insert_header(("Cookie", auth_cookie.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let thread: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(thread["root"]["id"], root_message_id);
        assert!(thread["replies"].as_array().unwrap().iter().any(|reply| {
            reply["id"].as_i64() == Some(thread_reply_id)
                && reply["text"] == "thread reply survives"
        }));

        let req = test::TestRequest::get()
            .uri("/emojis")
            .insert_header(("Cookie", auth_cookie.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let emojis: serde_json::Value = test::read_body_json(resp).await;
        let emoji = json_row_by_id(emojis.as_array().unwrap(), emoji_id);
        assert_eq!(emoji["name"], "DurableWave");
        assert_eq!(emoji["image_url"], emoji_url);
        assert_eq!(emoji["animated"], false);
        assert_eq!(emoji["created_by_user_id"], user.id);
        assert!(emoji["deleted_at"].is_null());

        let req = test::TestRequest::get()
            .uri(asset_path_without_query(&emoji_url))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let emoji_bytes = test::read_body(resp).await;
        assert_webp_bytes(&emoji_bytes);

        let req = test::TestRequest::get()
            .uri(&attachment_url)
            .insert_header(("Cookie", auth_cookie.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get(CONTENT_TYPE).unwrap(), "image/webp");
        let full_attachment = test::read_body(resp).await;
        assert_webp_bytes(&full_attachment);

        let req = test::TestRequest::get()
            .uri(&attachment_thumbnail_url)
            .insert_header(("Cookie", auth_cookie.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get(CONTENT_TYPE).unwrap(), "image/webp");
        let thumbnail = test::read_body(resp).await;
        assert_webp_bytes(&thumbnail);
    }

    reconnected.close().await.unwrap();
}

fn app_deps(db: DatabaseConnection, uploads_dir: &Path) -> AppDeps {
    AppDeps {
        db: web::Data::new(db),
        broadcaster: web::Data::from(Broadcaster::new()),
        voice_cfg: web::Data::new(None::<VoiceConfig>),
        voice_state: web::Data::new(VoiceState::new()),
        embed_fetcher: web::Data::new(EmbedFetcher::Disabled),
        emoji_storage: web::Data::new(EmojiStorage {
            dir: uploads_dir.to_path_buf(),
        }),
    }
}

fn config_for_default_database_dir(root: &Path, data_dir: &Path) -> Config {
    Config {
        bind_addr: "127.0.0.1:0".to_owned(),
        database_url: Config::default_database_url_for_data_dir(data_dir),
        log_filter: "off".to_owned(),
        sentry_dsn: None,
        uploads_dir: root.join("uploads"),
        message_attachments_dir: root.join("private-uploads").join("message-attachments"),
        server_settings: ServerSettings::default(),
        settings_file: data_dir.join("server-config.json"),
        voice: None,
        embed_fetcher_enabled: false,
        bootstrap_default_channels: true,
        seed_dev_data: false,
    }
}

fn prepare_storage_dirs(config: &Config) {
    std::fs::create_dir_all(config.uploads_dir.join("avatars")).unwrap();
    std::fs::create_dir_all(config.uploads_dir.join("emojis")).unwrap();
    std::fs::create_dir_all(&config.message_attachments_dir).unwrap();
}

async fn insert_embed(db: &DatabaseConnection, message_id: i64) -> i64 {
    let embed_id = generate_id();
    entity::embed::ActiveModel {
        id: Set(embed_id),
        message_id: Set(message_id),
        url: Set("https://example.invalid/card".to_owned()),
        title: Set(Some("Durable Embed".to_owned())),
        description: Set(Some("embed metadata survives reconnect".to_owned())),
        image_url: Set(Some("https://example.invalid/card.png".to_owned())),
        site_name: Set(Some("Example Invalid".to_owned())),
        embed_type: Set("link".to_owned()),
        iframe_url: Set(None),
        iframe_width: Set(None),
        iframe_height: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    embed_id
}

fn field_part(name: &'static str, value: &str) -> MultipartPart {
    MultipartPart {
        name,
        filename: None,
        content_type: None,
        bytes: value.as_bytes().to_vec(),
    }
}

fn file_part(
    name: &'static str,
    filename: &'static str,
    content_type: &'static str,
    bytes: Vec<u8>,
) -> MultipartPart {
    MultipartPart {
        name,
        filename: Some(filename),
        content_type: Some(content_type),
        bytes,
    }
}

fn multipart_payload(boundary: &str, parts: Vec<MultipartPart>) -> (String, Vec<u8>) {
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

fn make_png(width: u32, height: u32) -> Vec<u8> {
    use image::{Rgb, RgbImage};

    let img = RgbImage::from_pixel(width, height, Rgb([80, 140, 220]));
    let mut buf = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut buf, image::ImageFormat::Png)
        .unwrap();
    buf.into_inner()
}

fn json_row_by_id(rows: &[serde_json::Value], id: i64) -> &serde_json::Value {
    rows.iter()
        .find(|row| row["id"].as_i64() == Some(id))
        .unwrap_or_else(|| panic!("missing JSON row with id {id}"))
}

fn asset_path_without_query(url: &str) -> &str {
    url.split('?').next().unwrap()
}

fn assert_webp_bytes(bytes: &[u8]) {
    assert!(bytes.len() >= 12, "response was too short to be WebP");
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
}

fn assert_embed_present(message: &serde_json::Value, embed_id: i64) {
    let embeds = message["embeds"].as_array().unwrap();
    let embed = json_row_by_id(embeds, embed_id);
    assert_eq!(embed["url"], "https://example.invalid/card");
    assert_eq!(embed["title"], "Durable Embed");
    assert_eq!(embed["site_name"], "Example Invalid");
}

fn assert_native_reaction_present(message: &serde_json::Value, emoji: &str) {
    let reactions = message["reactions"].as_array().unwrap();
    let reaction = reactions
        .iter()
        .find(|reaction| reaction["kind"] == "native" && reaction["emoji"] == emoji)
        .unwrap();
    assert_eq!(reaction["count"], 1);
    assert_eq!(reaction["me_reacted"], true);
    assert_eq!(reaction["reactors"], serde_json::json!(["You"]));
}

fn assert_custom_reaction_present(message: &serde_json::Value, emoji_id: i64, emoji_url: &str) {
    let reactions = message["reactions"].as_array().unwrap();
    let reaction = reactions
        .iter()
        .find(|reaction| {
            reaction["kind"] == "custom" && reaction["emoji_id"].as_i64() == Some(emoji_id)
        })
        .unwrap();
    assert_eq!(reaction["name"], "DurableWave");
    assert_eq!(reaction["image_url"], emoji_url);
    assert_eq!(reaction["animated"], false);
    assert_eq!(reaction["deleted_at"], serde_json::Value::Null);
    assert_eq!(reaction["count"], 1);
    assert_eq!(reaction["me_reacted"], true);
    assert_eq!(reaction["reactors"], serde_json::json!(["You"]));
}

fn tmp_root() -> PathBuf {
    std::env::temp_dir().join(format!("hamlet-feature-persistence-test-{}", generate_id()))
}
