use std::collections::BTreeMap;

use iced::widget::image::Handle;
use thiserror::Error;
use url::Url;

use crate::protocol::Embed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedRenderMode {
    LinkCard,
    NativeImagePreview,
    ExternalOpenCard,
}

pub fn embed_render_mode(embed: &Embed) -> EmbedRenderMode {
    let kind = embed.embed_type.trim().to_ascii_lowercase();

    if matches!(kind.as_str(), "photo" | "image") && has_non_empty_value(&embed.image_url) {
        return EmbedRenderMode::NativeImagePreview;
    }

    if matches!(kind.as_str(), "rich" | "video") && has_non_empty_value(&embed.iframe_url) {
        return EmbedRenderMode::ExternalOpenCard;
    }

    EmbedRenderMode::LinkCard
}

pub fn embed_site_label(embed: &Embed) -> String {
    if let Some(site_name) = embed
        .site_name
        .as_deref()
        .filter(|site| !site.trim().is_empty())
    {
        return site_name.to_string();
    }

    Url::parse(&embed.url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| embed.url.clone())
}

pub fn embed_has_preview_image(embed: &Embed) -> bool {
    has_non_empty_value(&embed.image_url)
}

fn has_non_empty_value(value: &Option<String>) -> bool {
    value
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedImageFetchRequest {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EmbedImageError {
    #[error("invalid embed image URL: {0}")]
    InvalidUrl(String),
    #[error("could not reach {url}: {message}")]
    Unreachable { url: String, message: String },
    #[error("embed image request returned {status}")]
    Server { status: u16 },
    #[error("could not read embed image: {0}")]
    Read(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmbedImageCache {
    entries: BTreeMap<String, EmbedImageStatus>,
}

impl EmbedImageCache {
    pub fn begin_load(
        &mut self,
        server_url: &str,
        image_url: Option<&str>,
    ) -> Option<EmbedImageFetchRequest> {
        let image_url = image_url?.trim();
        if image_url.is_empty() {
            return None;
        }

        let resolved = resolve_embed_image_url(server_url, image_url).ok()?;
        if self.entries.contains_key(&resolved) {
            return None;
        }

        self.entries
            .insert(resolved.clone(), EmbedImageStatus::Loading);
        Some(EmbedImageFetchRequest { url: resolved })
    }

    pub fn complete_load(&mut self, url: String, result: Result<Vec<u8>, EmbedImageError>) {
        let status = match result {
            Ok(bytes) => EmbedImageStatus::Loaded {
                byte_len: bytes.len(),
                handle: Handle::from_bytes(bytes),
            },
            Err(error) => EmbedImageStatus::Failed(error.to_string()),
        };

        self.entries.insert(url, status);
    }

    pub fn handle_for(&self, server_url: &str, image_url: Option<&str>) -> Option<&Handle> {
        let resolved = resolve_embed_image_url(server_url, image_url?).ok()?;

        match self.entries.get(&resolved) {
            Some(EmbedImageStatus::Loaded { handle, .. }) => Some(handle),
            Some(EmbedImageStatus::Loading | EmbedImageStatus::Failed(_)) | None => None,
        }
    }

    pub fn status_for_image_url(
        &self,
        server_url: &str,
        image_url: &str,
    ) -> Option<&EmbedImageStatus> {
        let resolved = resolve_embed_image_url(server_url, image_url).ok()?;
        self.entries.get(&resolved)
    }

    pub fn status_for(&self, resolved_url: &str) -> Option<&EmbedImageStatus> {
        self.entries.get(resolved_url)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedImageStatus {
    Loading,
    Loaded { byte_len: usize, handle: Handle },
    Failed(String),
}

impl EmbedImageStatus {
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

pub fn resolve_embed_image_url(
    server_url: &str,
    image_url: &str,
) -> Result<String, EmbedImageError> {
    let trimmed = image_url.trim();
    if trimmed.is_empty() {
        return Err(EmbedImageError::InvalidUrl(
            "embed image URL cannot be empty".to_string(),
        ));
    }

    if let Ok(absolute) = Url::parse(trimmed) {
        return match absolute.scheme() {
            "http" | "https" => Ok(absolute.to_string()),
            scheme => Err(EmbedImageError::InvalidUrl(format!(
                "embed image URL scheme {scheme:?} is not supported"
            ))),
        };
    }

    let base = relative_base_url(server_url)?;
    base.join(trimmed)
        .map(|url| url.to_string())
        .map_err(|error| EmbedImageError::InvalidUrl(error.to_string()))
}

pub fn fetch_embed_image(url: String) -> (String, Result<Vec<u8>, EmbedImageError>) {
    let result = fetch_embed_image_result(&url);

    (url, result)
}

fn fetch_embed_image_result(url: &str) -> Result<Vec<u8>, EmbedImageError> {
    Url::parse(url).map_err(|error| EmbedImageError::InvalidUrl(error.to_string()))?;

    let response = reqwest::blocking::get(url).map_err(|error| EmbedImageError::Unreachable {
        url: url.to_string(),
        message: error.to_string(),
    })?;
    let status = response.status();

    if !status.is_success() {
        return Err(EmbedImageError::Server {
            status: status.as_u16(),
        });
    }

    response
        .bytes()
        .map(|bytes| bytes.to_vec())
        .map_err(|error| EmbedImageError::Read(error.to_string()))
}

fn relative_base_url(server_url: &str) -> Result<Url, EmbedImageError> {
    let trimmed = server_url.trim();
    let mut base =
        Url::parse(trimmed).map_err(|error| EmbedImageError::InvalidUrl(error.to_string()))?;

    match base.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(EmbedImageError::InvalidUrl(format!(
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
