//! Dev-only seed data. Inserts a `general` text channel + a `voice` voice
//! channel, two dev users (`baipas` / `teo`), seeds an avatar for `baipas`,
//! and prints a fixed session token for the impatient.
//!
//! Currently runs unconditionally in `main.rs`. Once we move off the
//! in-memory DB this should be gated on a debug build / `HAMLET_SEED`.

use std::path::Path;

use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};

use crate::api::avatars::AVATARS_SUBDIR;
use crate::api::channels::{CHANNEL_TYPE_TEXT, CHANNEL_TYPE_VOICE};
use crate::auth;
use crate::entity;
use crate::util::{generate_id, now_unix_secs};

#[allow(clippy::unwrap_used)]
pub async fn seed_development_data(db: &DatabaseConnection, uploads_dir: &Path) {
    db.get_schema_registry("hamlet::entity::*")
        .sync(db)
        .await
        .unwrap();

    let general_channel = entity::channel::ActiveModel {
        id: Set(generate_id()),
        name: Set("general".to_owned()),
        position: Set(0),
        channel_type: Set(CHANNEL_TYPE_TEXT.to_owned()),
    };
    general_channel.insert(db).await.unwrap();

    let voice_channel = entity::channel::ActiveModel {
        id: Set(generate_id()),
        name: Set("voice".to_owned()),
        position: Set(1),
        channel_type: Set(CHANNEL_TYPE_VOICE.to_owned()),
    };
    voice_channel.insert(db).await.unwrap();

    let dev_user = auth::register_user(db, "baipas", "password", None)
        .await
        .unwrap();

    auth::register_user(db, "teo", "password", None)
        .await
        .unwrap();

    let avatars_dir = uploads_dir.join(AVATARS_SUBDIR);
    let now = now_unix_secs();
    if std::fs::create_dir_all(&avatars_dir).is_ok() {
        let filename = format!("{}.webp", dev_user.id);
        if std::fs::write(avatars_dir.join(&filename), default_avatar_webp()).is_ok() {
            let mut model: entity::user::ActiveModel = dev_user.clone().into();
            model.avatar_path = Set(Some(format!("{AVATARS_SUBDIR}/{filename}")));
            model.avatar_updated_at = Set(Some(now));
            let _ = model.update(db).await;
        }
    }

    const DEV_SESSION_TOKEN: &str =
        "devdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdevdev";
    entity::session::ActiveModel {
        token: Set(DEV_SESSION_TOKEN.to_owned()),
        user_id: Set(dev_user.id),
        created_at: Set(now),
        expires_at: Set(now + 60 * 60 * 24 * 365),
    }
    .insert(db)
    .await
    .unwrap();

    tracing::info!(
        token = DEV_SESSION_TOKEN,
        "DEV: baipas session active — set cookie: session=<token>"
    );
}

/// 256x256 placeholder webp (a simple two-color circle) used for the dev
/// user's seeded avatar.
fn default_avatar_webp() -> Vec<u8> {
    use std::io::Cursor;

    use image::{Rgb, RgbImage};
    let size = 256u32;
    let mut img = RgbImage::new(size, size);
    let cx = size as i32 / 2;
    let cy = size as i32 / 2;
    let r2 = (size as i32 / 2 - 8).pow(2);
    let bg = Rgb([44u8, 82, 130]);
    let fg = Rgb([129u8, 230, 217]);
    for y in 0..size {
        for x in 0..size {
            let dx = x as i32 - cx;
            let dy = y as i32 - cy;
            let pixel = if dx * dx + dy * dy < r2 { fg } else { bg };
            img.put_pixel(x, y, pixel);
        }
    }
    let mut out = Cursor::new(Vec::new());
    let _ = image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::WebP);
    out.into_inner()
}
