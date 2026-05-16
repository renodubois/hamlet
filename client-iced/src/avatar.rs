use std::collections::BTreeMap;

use iced::widget::image::Handle;
use thiserror::Error;
use url::Url;

use crate::protocol::Id;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarFetchRequest {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AvatarImageError {
    #[error("invalid avatar URL: {0}")]
    InvalidUrl(String),
    #[error("could not reach {url}: {message}")]
    Unreachable { url: String, message: String },
    #[error("avatar request returned {status}")]
    Server { status: u16 },
    #[error("could not read avatar image: {0}")]
    Read(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AvatarImageCache {
    entries: BTreeMap<String, AvatarImageStatus>,
}

impl AvatarImageCache {
    pub fn begin_load(
        &mut self,
        server_url: &str,
        avatar_url: Option<&str>,
    ) -> Option<AvatarFetchRequest> {
        let avatar_url = avatar_url?.trim();
        if avatar_url.is_empty() {
            return None;
        }

        let resolved = resolve_avatar_url(server_url, avatar_url).ok()?;
        if self.entries.contains_key(&resolved) {
            return None;
        }

        self.entries
            .insert(resolved.clone(), AvatarImageStatus::Loading);
        Some(AvatarFetchRequest { url: resolved })
    }

    pub fn complete_load(&mut self, url: String, result: Result<Vec<u8>, AvatarImageError>) {
        let status = match result {
            Ok(bytes) => AvatarImageStatus::Loaded {
                byte_len: bytes.len(),
                handle: Handle::from_bytes(bytes),
            },
            Err(error) => AvatarImageStatus::Failed(error.to_string()),
        };

        self.entries.insert(url, status);
    }

    pub fn handle_for(&self, server_url: &str, avatar_url: Option<&str>) -> Option<&Handle> {
        let resolved = resolve_avatar_url(server_url, avatar_url?).ok()?;

        match self.entries.get(&resolved) {
            Some(AvatarImageStatus::Loaded { handle, .. }) => Some(handle),
            Some(AvatarImageStatus::Loading | AvatarImageStatus::Failed(_)) | None => None,
        }
    }

    pub fn status_for(&self, resolved_url: &str) -> Option<&AvatarImageStatus> {
        self.entries.get(resolved_url)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvatarImageStatus {
    Loading,
    Loaded { byte_len: usize, handle: Handle },
    Failed(String),
}

impl AvatarImageStatus {
    pub fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded { .. })
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackAvatar {
    pub initials: String,
    pub background_rgb: [u8; 3],
    pub foreground_rgb: [u8; 3],
}

pub fn fallback_avatar(user_id: Id, username: &str, display_name: Option<&str>) -> FallbackAvatar {
    let hash = stable_identity_hash(user_id, username);
    let background_rgb = fallback_palette_color(hash);
    let foreground_rgb = readable_foreground(background_rgb);

    FallbackAvatar {
        initials: fallback_initials(username, display_name),
        background_rgb,
        foreground_rgb,
    }
}

pub fn resolve_avatar_url(server_url: &str, avatar_url: &str) -> Result<String, AvatarImageError> {
    let trimmed = avatar_url.trim();
    if trimmed.is_empty() {
        return Err(AvatarImageError::InvalidUrl(
            "avatar URL cannot be empty".to_string(),
        ));
    }

    if let Ok(absolute) = Url::parse(trimmed) {
        return match absolute.scheme() {
            "http" | "https" => Ok(absolute.to_string()),
            scheme => Err(AvatarImageError::InvalidUrl(format!(
                "avatar URL scheme {scheme:?} is not supported"
            ))),
        };
    }

    let base = relative_base_url(server_url)?;
    base.join(trimmed)
        .map(|url| url.to_string())
        .map_err(|error| AvatarImageError::InvalidUrl(error.to_string()))
}

pub fn fetch_avatar_image(url: String) -> (String, Result<Vec<u8>, AvatarImageError>) {
    let result = fetch_avatar_image_result(&url);

    (url, result)
}

fn fetch_avatar_image_result(url: &str) -> Result<Vec<u8>, AvatarImageError> {
    Url::parse(url).map_err(|error| AvatarImageError::InvalidUrl(error.to_string()))?;

    let response = reqwest::blocking::get(url).map_err(|error| AvatarImageError::Unreachable {
        url: url.to_string(),
        message: error.to_string(),
    })?;
    let status = response.status();

    if !status.is_success() {
        return Err(AvatarImageError::Server {
            status: status.as_u16(),
        });
    }

    response
        .bytes()
        .map(|bytes| bytes.to_vec())
        .map_err(|error| AvatarImageError::Read(error.to_string()))
}

fn relative_base_url(server_url: &str) -> Result<Url, AvatarImageError> {
    let trimmed = server_url.trim();
    let mut base =
        Url::parse(trimmed).map_err(|error| AvatarImageError::InvalidUrl(error.to_string()))?;

    match base.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(AvatarImageError::InvalidUrl(format!(
                "server URL scheme {scheme:?} is not supported"
            )));
        }
    }

    if !base.path().ends_with('/') {
        let path = format!("{}/", base.path().trim_end_matches('/'));
        base.set_path(&path);
    }

    Ok(base)
}

fn stable_identity_hash(user_id: Id, username: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = FNV_OFFSET;
    for byte in user_id.to_le_bytes().iter().chain(username.as_bytes()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    hash
}

fn fallback_palette_color(hash: u64) -> [u8; 3] {
    const PALETTE: [[u8; 3]; 12] = [
        [88, 166, 255],
        [163, 113, 247],
        [238, 121, 89],
        [63, 185, 80],
        [210, 153, 34],
        [219, 68, 85],
        [46, 160, 67],
        [251, 133, 0],
        [0, 120, 212],
        [132, 94, 194],
        [0, 153, 188],
        [190, 75, 219],
    ];

    PALETTE[hash as usize % PALETTE.len()]
}

fn readable_foreground(background: [u8; 3]) -> [u8; 3] {
    let luminance = 0.2126 * f32::from(background[0])
        + 0.7152 * f32::from(background[1])
        + 0.0722 * f32::from(background[2]);

    if luminance > 150.0 {
        [20, 20, 20]
    } else {
        [245, 245, 245]
    }
}

fn fallback_initials(username: &str, display_name: Option<&str>) -> String {
    let source = display_name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(username)
        .trim();
    let mut initials = String::new();

    for word in source.split_whitespace() {
        if let Some(ch) = word.chars().find(|ch| ch.is_alphanumeric()) {
            push_uppercase_initial(&mut initials, ch);
        }
        if initials.chars().count() >= 2 {
            break;
        }
    }

    if initials.is_empty()
        && let Some(ch) = source.chars().find(|ch| ch.is_alphanumeric())
    {
        push_uppercase_initial(&mut initials, ch);
    }

    if initials.is_empty() {
        "?".to_string()
    } else {
        initials
    }
}

fn push_uppercase_initial(initials: &mut String, ch: char) {
    if let Some(ch) = ch.to_uppercase().next() {
        initials.push(ch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_identity_is_deterministic() {
        let first = fallback_avatar(42, "alice", Some("Alice Example"));
        let second = fallback_avatar(42, "alice", Some("Alice Example"));
        let different_user = fallback_avatar(43, "alice", Some("Alice Example"));

        assert_eq!(first, second);
        assert_ne!(first.background_rgb, different_user.background_rgb);
        assert_eq!(first.initials, "AE");
    }

    #[test]
    fn relative_avatar_urls_resolve_against_server_url() {
        assert_eq!(
            resolve_avatar_url("http://localhost:3030", "/uploads/avatars/42.webp?v=7"),
            Ok("http://localhost:3030/uploads/avatars/42.webp?v=7".to_string())
        );
        assert_eq!(
            resolve_avatar_url("https://chat.example.test/api", "uploads/42.webp"),
            Ok("https://chat.example.test/api/uploads/42.webp".to_string())
        );
        assert_eq!(
            resolve_avatar_url("http://localhost:3030", "https://cdn.example/a.webp"),
            Ok("https://cdn.example/a.webp".to_string())
        );
    }

    #[test]
    fn cache_tracks_loads_and_reuses_successes() {
        let mut cache = AvatarImageCache::default();
        let request = match cache.begin_load("http://localhost:3030", Some("/uploads/a.webp?v=1")) {
            Some(request) => request,
            None => panic!("first avatar load should queue"),
        };

        assert_eq!(request.url, "http://localhost:3030/uploads/a.webp?v=1");
        assert!(
            cache
                .status_for("http://localhost:3030/uploads/a.webp?v=1")
                .is_some_and(AvatarImageStatus::is_loading)
        );
        assert_eq!(
            cache.begin_load("http://localhost:3030", Some("/uploads/a.webp?v=1")),
            None
        );

        cache.complete_load(request.url.clone(), Ok(vec![1, 2, 3]));

        assert!(
            cache
                .status_for(&request.url)
                .is_some_and(AvatarImageStatus::is_loaded)
        );
        assert!(
            cache
                .handle_for("http://localhost:3030", Some("/uploads/a.webp?v=1"))
                .is_some()
        );
        assert_eq!(
            cache.begin_load("http://localhost:3030", Some("/uploads/a.webp?v=1")),
            None
        );
    }

    #[test]
    fn cache_keeps_failed_fetches_from_retrying_in_a_loop() {
        let mut cache = AvatarImageCache::default();
        let request = match cache.begin_load("http://localhost:3030", Some("/uploads/missing.webp"))
        {
            Some(request) => request,
            None => panic!("first avatar load should queue"),
        };

        cache.complete_load(
            request.url.clone(),
            Err(AvatarImageError::Server { status: 404 }),
        );

        assert!(
            cache
                .status_for(&request.url)
                .is_some_and(AvatarImageStatus::is_failed)
        );
        assert_eq!(
            cache.begin_load("http://localhost:3030", Some("/uploads/missing.webp")),
            None
        );
    }
}
