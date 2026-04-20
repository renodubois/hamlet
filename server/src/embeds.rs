//! URL extraction + embed fetcher used by the message-embed pipeline.
//!
//! The happy path is: `extract_urls(text)` pulls candidate `http(s)` URLs out
//! of message text; `fetch_embed(url)` turns each one into a `FetchedEmbed`.
//!
//! Embed resolution order, best → worst:
//!
//!  1. **Provider registry.** A small compiled-in allowlist of URL shapes
//!     that need special handling. Two reasons a provider lands here:
//!     (a) the page doesn't publish `<link rel="alternate"
//!     type="application/json+oembed">` to crawlers, so auto-discovery
//!     misses it (YouTube, as of 2026); or (b) the oEmbed response's
//!     `html` field isn't a bare iframe and needs provider-specific
//!     parsing (Bluesky's is a `<blockquote>` + embed.js). Matched URLs
//!     skip the HTML fetch and go straight to the provider's oEmbed
//!     endpoint.
//!  2. **oEmbed discovery.** Fetch the page HTML and look for the
//!     `<link rel="alternate" type="application/json+oembed">` tag. If
//!     present, fetch that JSON and merge it over whatever OG we found.
//!     Catches providers that still publish the discovery tag.
//!  3. **OpenGraph only.** Fall back to `og:*` / `twitter:*` / `<title>`
//!     from the HTML. Produces a plain link card.
//!
//! Safety constraints on every outbound request (both the HTML fetch and the
//! oEmbed fetch):
//!   - http/https only — no file://, gopher://, etc.
//!   - User-supplied URLs hit the network, so this is an SSRF surface. We
//!     reject hostnames that resolve to a private/loopback/link-local IP
//!     before issuing the request. Coarse — DNS rebinding can still bypass
//!     on a second resolution — but better than nothing.
//!   - 5s timeout, max 2 redirects, response body capped at 512 KiB.
//!   - We do not inject oEmbed-provided HTML directly into any page. Instead
//!     we parse out the first `<iframe src>` and require https; the client
//!     renders its own sandboxed iframe using just that src.

use std::net::IpAddr;
use std::time::Duration;

use scraper::{Html, Selector};
use serde::Deserialize;
use url::Url;

const MAX_BODY_BYTES: usize = 512 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REDIRECTS: usize = 2;
const USER_AGENT: &str = "hamlet-embed-fetcher/0.1 (+https://github.com/renodubois/hamlet)";

/// oEmbed response type (per oembed.com spec). Determines how the client
/// renders the embed: a `Video`/`Rich` gets an iframe, `Photo` gets a large
/// image, everything else is a plain link card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmbedType {
    #[default]
    Link,
    Photo,
    Video,
    Rich,
}

impl EmbedType {
    pub fn as_str(self) -> &'static str {
        match self {
            EmbedType::Link => "link",
            EmbedType::Photo => "photo",
            EmbedType::Video => "video",
            EmbedType::Rich => "rich",
        }
    }
}

/// The parsed embed metadata for a single URL. All fields except `url` and
/// `embed_type` are best-effort — a link with no OG tags, no discoverable
/// oEmbed, and no `<title>` still yields `FetchedEmbed { url, embed_type: Link, .. }`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FetchedEmbed {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub site_name: Option<String>,
    pub embed_type: EmbedType,
    /// For `Video`/`Rich`: the iframe `src` extracted from the oEmbed `html`
    /// field. Validated to be https. Clients are expected to render this
    /// inside a sandboxed iframe.
    pub iframe_url: Option<String>,
    pub iframe_width: Option<i32>,
    pub iframe_height: Option<i32>,
}

#[derive(Debug)]
pub enum FetchError {
    InvalidUrl,
    UnsafeHost,
    Network,
    BadStatus,
    NotHtml,
    TooLarge,
}

/// Extract unique http(s) URLs from free-form message text.
///
/// We scan token-by-token on whitespace and look for tokens starting with
/// `http://` or `https://`. Trailing punctuation that's commonly adjacent to
/// URLs in prose (.,;:!?)]}'"`>) is trimmed. Duplicate URLs in the same message
/// collapse to one so we don't fetch the same page twice.
pub fn extract_urls(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split_whitespace() {
        let Some(candidate) = trim_url(raw) else {
            continue;
        };
        if !(candidate.starts_with("http://") || candidate.starts_with("https://")) {
            continue;
        }
        let Ok(parsed) = Url::parse(candidate) else {
            continue;
        };
        if parsed.host_str().is_none() {
            continue;
        }
        let normalized = parsed.to_string();
        if !out.iter().any(|u| u == &normalized) {
            out.push(normalized);
        }
    }
    out
}

fn trim_url(raw: &str) -> Option<&str> {
    const TRAILING: &[char] = &[
        '.', ',', ';', ':', '!', '?', ')', ']', '}', '\'', '"', '`', '>',
    ];
    const LEADING: &[char] = &['(', '[', '{', '\'', '"', '`', '<'];
    let trimmed = raw.trim_start_matches(LEADING).trim_end_matches(TRAILING);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// True if `ip` is in a range we refuse to fetch from.
///
/// Matches `is_loopback`, RFC1918, link-local, CGNAT, and the unspecified
/// address; plus the v6 equivalents. Public IPs fall through and are allowed.
fn is_unsafe_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                // CGNAT 100.64.0.0/10
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // ULA fc00::/7
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
        }
    }
}

async fn host_is_safe(host: &str) -> bool {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return !is_unsafe_ip(ip);
    }
    let Ok(mut addrs) = tokio::net::lookup_host((host, 80u16)).await else {
        return false;
    };
    addrs.all(|sa| !is_unsafe_ip(sa.ip()))
}

/// A known oEmbed provider. Matches cover URL shapes where HTML auto-discovery
/// is unreliable: either the page doesn't ship a `<link rel="alternate"
/// type="application/json+oembed">` tag to crawlers (YouTube, as of 2026),
/// or the oEmbed response's `html` isn't a bare iframe that our generic
/// `extract_iframe` can consume (Bluesky ships a blockquote + embed.js).
///
/// `extract_rich` is a per-provider function that turns the oEmbed response
/// into a concrete iframe URL + suggested width/height. `None` means "use
/// the default iframe-src extractor" — that's fine when the provider's
/// `html` field already contains `<iframe src=…>`.
/// Iframe src + optional suggested dimensions, extracted from an oEmbed
/// response. Shared return type of every provider's rich extractor.
type RichExtract = (String, Option<i32>, Option<i32>);
type RichExtractor = fn(&OembedResponse) -> Option<RichExtract>;

struct Provider {
    oembed_endpoint: &'static str,
    embed_type: EmbedType,
    extract_rich: Option<RichExtractor>,
}

/// Return the oEmbed provider for `url`, if this URL shape is in our
/// allowlist. Only the URL shape determines the match — we never trust the
/// network response to tell us which provider to use.
fn match_provider(url: &Url) -> Option<Provider> {
    let host = url.host_str()?.to_ascii_lowercase();
    let host_root = host.strip_prefix("www.").unwrap_or(&host);
    match host_root {
        // bsky.app is a client-rendered SPA; crawlers get no usable HTML.
        // Its oEmbed endpoint returns a blockquote + embed.js, so we can't
        // rely on the generic iframe-src extractor — the DID is carried in
        // `data-bluesky-uri` on the blockquote and we rebuild the iframe URL
        // from it.
        "bsky.app" => {
            let segs: Vec<_> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
            if segs.len() >= 4 && segs[0] == "profile" && segs[2] == "post" {
                return Some(Provider {
                    oembed_endpoint: "https://embed.bsky.app/oembed",
                    embed_type: EmbedType::Rich,
                    extract_rich: Some(extract_bluesky_iframe),
                });
            }
            None
        }
        // YouTube's page HTML no longer ships the `<link rel=oembed>`
        // discovery tag for bot UAs, so auto-discovery misses it. Hit the
        // oEmbed endpoint directly; their `html` is a clean `<iframe src=…>`
        // so the default extractor handles it.
        "youtube.com" | "m.youtube.com" => Some(Provider {
            oembed_endpoint: "https://www.youtube.com/oembed",
            embed_type: EmbedType::Video,
            extract_rich: None,
        }),
        "youtu.be" => Some(Provider {
            oembed_endpoint: "https://www.youtube.com/oembed",
            embed_type: EmbedType::Video,
            extract_rich: None,
        }),
        _ => None,
    }
}

/// Pull the AT-protocol DID + rkey out of a Bluesky oEmbed blockquote and
/// build an iframe URL at `https://embed.bsky.app/embed/{did}/…`. We build
/// the iframe URL ourselves because the embed endpoint refuses handle-form
/// URLs (returns HTTP 400) — only DIDs work.
fn extract_bluesky_iframe(oembed: &OembedResponse) -> Option<RichExtract> {
    let html = oembed.html.as_deref()?;
    let doc = Html::parse_fragment(html);
    let sel = Selector::parse("blockquote[data-bluesky-uri]").ok()?;
    let el = doc.select(&sel).next()?;
    let at_uri = el.value().attr("data-bluesky-uri")?;
    // at://did:plc:XXX/app.bsky.feed.post/rkey
    let rest = at_uri.strip_prefix("at://")?;
    let (did, tail) = rest.split_once('/')?;
    let (collection, rkey) = tail.rsplit_once('/')?;
    if collection != "app.bsky.feed.post" || did.is_empty() || rkey.is_empty() {
        return None;
    }
    let iframe = format!("https://embed.bsky.app/embed/{did}/app.bsky.feed.post/{rkey}");
    // Bluesky's embed is self-resizing via postMessage, but we still need
    // something for the initial layout. 600×480 is a reasonable starting box
    // for a post card; the client's aspect-ratio style uses it as a hint.
    Some((
        iframe,
        oembed.width.or(Some(600)),
        oembed.height.or(Some(480)),
    ))
}

/// Fetch `url` and return the best-available embed metadata. Tries the
/// provider registry first, then oEmbed discovery via HTML, and falls back to
/// plain OpenGraph scraping.
pub async fn fetch_embed(url: &str) -> Result<FetchedEmbed, FetchError> {
    let parsed = Url::parse(url).map_err(|_| FetchError::InvalidUrl)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(FetchError::InvalidUrl);
    }
    let host = parsed.host_str().ok_or(FetchError::InvalidUrl)?;
    if !host_is_safe(host).await {
        return Err(FetchError::UnsafeHost);
    }

    // Known-provider fast path. If this URL matches our allowlist we skip
    // the HTML fetch entirely because the page won't have useful metadata
    // anyway (that's why it's on the list).
    if let Some(p) = match_provider(&parsed) {
        let oembed = fetch_oembed_json(p.oembed_endpoint, url).await?;
        return Ok(provider_embed(url.to_owned(), &oembed, &p));
    }

    // Generic path: fetch HTML, parse OG, optionally discover + fetch oEmbed.
    let client = build_client()?;
    let resp = client
        .get(parsed.clone())
        .send()
        .await
        .map_err(|_| FetchError::Network)?;
    if !resp.status().is_success() {
        return Err(FetchError::BadStatus);
    }
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !ct.is_empty() && !ct.contains("html") {
        return Err(FetchError::NotHtml);
    }
    let body = read_bounded(resp, MAX_BODY_BYTES).await?;
    let mut embed = parse_opengraph(url, &body);

    if let Some(href) = discover_oembed_link(&body) {
        // Resolve relative discovery URLs against the document.
        let resolved = Url::parse(url)
            .ok()
            .and_then(|b| b.join(&href).ok())
            .map(|u| u.to_string())
            .unwrap_or(href);
        if is_safe_http_url(&resolved).await
            && let Ok(oembed) = fetch_oembed_json(&resolved, url).await
        {
            embed = merge_oembed(url.to_owned(), oembed, embed);
        }
    }

    Ok(embed)
}

/// Is `url` an http(s) URL whose host resolves to a public IP? Checked before
/// every outbound request to avoid turning the server into an internal-network
/// probe.
async fn is_safe_http_url(url: &str) -> bool {
    let Ok(u) = Url::parse(url) else {
        return false;
    };
    if !matches!(u.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = u.host_str() else {
        return false;
    };
    host_is_safe(host).await
}

fn build_client() -> Result<reqwest::Client, FetchError> {
    reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|_| FetchError::Network)
}

/// Fetch an oEmbed endpoint with `url=<target>&format=json` appended to any
/// existing query params.
async fn fetch_oembed_json(endpoint: &str, target_url: &str) -> Result<OembedResponse, FetchError> {
    let mut oeu = Url::parse(endpoint).map_err(|_| FetchError::InvalidUrl)?;
    if !matches!(oeu.scheme(), "http" | "https") {
        return Err(FetchError::InvalidUrl);
    }
    let endpoint_host = oeu.host_str().ok_or(FetchError::InvalidUrl)?.to_owned();
    if !host_is_safe(&endpoint_host).await {
        return Err(FetchError::UnsafeHost);
    }
    {
        let has_url = oeu.query_pairs().any(|(k, _)| k == "url");
        let has_format = oeu.query_pairs().any(|(k, _)| k == "format");
        let mut q = oeu.query_pairs_mut();
        if !has_url {
            q.append_pair("url", target_url);
        }
        if !has_format {
            q.append_pair("format", "json");
        }
    }

    let client = build_client()?;
    let resp = client
        .get(oeu)
        .send()
        .await
        .map_err(|_| FetchError::Network)?;
    if !resp.status().is_success() {
        return Err(FetchError::BadStatus);
    }
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    // oEmbed providers commonly return application/json or application/json+oembed.
    // Don't hard-fail on missing content-type, but reject HTML/other.
    if !ct.is_empty() && !ct.contains("json") {
        return Err(FetchError::NotHtml);
    }
    let body = read_bounded(resp, MAX_BODY_BYTES).await?;
    serde_json::from_str(&body).map_err(|_| FetchError::Network)
}

/// Find the href of the first `<link rel="alternate" type="application/json+oembed">`
/// in the HTML. Returns None if no such link is present.
fn discover_oembed_link(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(r#"link[rel="alternate"][type="application/json+oembed"]"#).ok()?;
    doc.select(&sel)
        .next()
        .and_then(|el| el.value().attr("href"))
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// Parsed subset of the oEmbed response. Width/height arrive as integers per
/// spec, but some providers (notably early Bluesky responses) wrap them in
/// strings — we deserialize flexibly.
#[derive(Debug, Default, Deserialize)]
struct OembedResponse {
    #[serde(default, rename = "type")]
    resp_type: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    author_name: Option<String>,
    #[serde(default)]
    provider_name: Option<String>,
    #[serde(default)]
    thumbnail_url: Option<String>,
    #[serde(default)]
    html: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default, deserialize_with = "deserialize_flexible_i32")]
    width: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_flexible_i32")]
    height: Option<i32>,
}

fn deserialize_flexible_i32<'de, D>(de: D) -> Result<Option<i32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(de)?;
    match v {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => {
            let i = n.as_i64().or_else(|| n.as_f64().map(|f| f as i64));
            Ok(i.and_then(|i| i32::try_from(i).ok()))
        }
        Some(serde_json::Value::String(s)) => Ok(s.trim().parse::<i32>().ok()),
        _ => Ok(None),
    }
}

/// Produce a `FetchedEmbed` for a URL that matched the provider registry.
/// Skips HTML parsing (we never fetched the page) and relies on each
/// provider's declared `embed_type` + custom/default iframe extractor.
///
/// If the provider declared a rich embed type (Video/Rich) but we can't
/// extract an iframe URL, we degrade to Link — a metadata-only card is
/// better than a broken player.
fn provider_embed(url: String, oembed: &OembedResponse, provider: &Provider) -> FetchedEmbed {
    let mut embed = FetchedEmbed {
        url,
        title: oembed
            .title
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .or_else(|| {
                oembed
                    .author_name
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_owned)
            }),
        description: None,
        image_url: oembed
            .thumbnail_url
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned),
        site_name: oembed
            .provider_name
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned),
        embed_type: provider.embed_type,
        iframe_url: None,
        iframe_width: None,
        iframe_height: None,
    };

    let extracted = match provider.extract_rich {
        Some(f) => f(oembed),
        None => oembed.html.as_deref().and_then(extract_iframe),
    };

    match extracted {
        Some((src, w, h)) => {
            embed.iframe_url = Some(src);
            embed.iframe_width = w.or(oembed.width);
            embed.iframe_height = h.or(oembed.height);
        }
        None => {
            // Rich/video provider but no iframe available — degrade.
            embed.embed_type = EmbedType::Link;
        }
    }

    embed
}

/// Combine an oEmbed response with whatever OG data we already have. oEmbed is
/// provider-authoritative for title/site_name/thumbnail/iframe; OG fills gaps
/// (most importantly `description`, which oEmbed doesn't define).
fn merge_oembed(url: String, oembed: OembedResponse, mut og: FetchedEmbed) -> FetchedEmbed {
    og.url = url;
    let t = oembed.resp_type.as_deref().unwrap_or("link");
    og.embed_type = match t {
        "photo" => EmbedType::Photo,
        "video" => EmbedType::Video,
        "rich" => EmbedType::Rich,
        _ => EmbedType::Link,
    };
    if let Some(title) = oembed.title.filter(|s| !s.trim().is_empty()) {
        og.title = Some(title);
    } else if og.title.is_none()
        && let Some(author) = oembed.author_name.filter(|s| !s.trim().is_empty())
    {
        og.title = Some(author);
    }
    if let Some(name) = oembed.provider_name.filter(|s| !s.trim().is_empty()) {
        og.site_name = Some(name);
    }
    if let Some(thumb) = oembed.thumbnail_url.filter(|s| !s.trim().is_empty()) {
        og.image_url = Some(thumb);
    }

    match og.embed_type {
        EmbedType::Video | EmbedType::Rich => {
            // The only part of oEmbed `html` we trust is the iframe `src`; we
            // never inject the raw HTML. If there's no iframe we have nothing
            // playable, so degrade to a link card.
            let extracted = oembed.html.as_deref().and_then(extract_iframe);
            if let Some((src, w, h)) = extracted {
                og.iframe_url = Some(src);
                og.iframe_width = w.or(oembed.width);
                og.iframe_height = h.or(oembed.height);
            } else {
                og.embed_type = EmbedType::Link;
            }
        }
        EmbedType::Photo => {
            if let Some(u) = oembed.url.filter(|s| !s.trim().is_empty()) {
                og.image_url = Some(u);
            }
        }
        EmbedType::Link => {}
    }

    og
}

/// Extract the first `<iframe>`'s https `src` and optional width/height from
/// an HTML fragment. Returns None for any of: no iframe, non-https src,
/// malformed URL, malformed HTML.
fn extract_iframe(html_fragment: &str) -> Option<RichExtract> {
    let doc = Html::parse_fragment(html_fragment);
    let sel = Selector::parse("iframe").ok()?;
    let el = doc.select(&sel).next()?;
    let src = el.value().attr("src")?;
    let parsed = Url::parse(src).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    let w = el
        .value()
        .attr("width")
        .and_then(|s| s.trim().parse::<i32>().ok());
    let h = el
        .value()
        .attr("height")
        .and_then(|s| s.trim().parse::<i32>().ok());
    Some((parsed.to_string(), w, h))
}

async fn read_bounded(resp: reqwest::Response, max: usize) -> Result<String, FetchError> {
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp;
    loop {
        match stream.chunk().await {
            Ok(Some(chunk)) => {
                if buf.len() + chunk.len() > max {
                    // Keep what we have up to the cap — parsers only need the
                    // first few KB of head, and bailing here means giant
                    // pages don't starve the fetch pool.
                    let room = max.saturating_sub(buf.len());
                    buf.extend_from_slice(&chunk[..room.min(chunk.len())]);
                    break;
                }
                buf.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(_) => return Err(FetchError::Network),
        }
    }
    if buf.is_empty() {
        return Err(FetchError::TooLarge);
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Parse OG metadata out of an HTML body. `base_url` is the URL the document
/// was fetched from and is used to resolve relative `og:image` values.
pub fn parse_opengraph(base_url: &str, html: &str) -> FetchedEmbed {
    let doc = Html::parse_document(html);
    let base = Url::parse(base_url).ok();

    let meta_sel = Selector::parse("meta").ok();
    let title_sel = Selector::parse("title").ok();

    let mut og_title: Option<String> = None;
    let mut og_description: Option<String> = None;
    let mut og_image: Option<String> = None;
    let mut og_site: Option<String> = None;
    // Twitter cards are a reasonable fallback when OG is absent.
    let mut tw_title: Option<String> = None;
    let mut tw_description: Option<String> = None;
    let mut tw_image: Option<String> = None;
    let mut meta_description: Option<String> = None;

    if let Some(sel) = meta_sel.as_ref() {
        for el in doc.select(sel) {
            let attrs = el.value();
            let prop = attrs
                .attr("property")
                .or_else(|| attrs.attr("name"))
                .unwrap_or("")
                .to_ascii_lowercase();
            let content = attrs.attr("content").unwrap_or("").trim();
            if content.is_empty() {
                continue;
            }
            match prop.as_str() {
                "og:title" => og_title = Some(content.to_owned()),
                "og:description" => og_description = Some(content.to_owned()),
                "og:image" | "og:image:url" | "og:image:secure_url" => {
                    og_image = Some(content.to_owned())
                }
                "og:site_name" => og_site = Some(content.to_owned()),
                "twitter:title" => tw_title = Some(content.to_owned()),
                "twitter:description" => tw_description = Some(content.to_owned()),
                "twitter:image" | "twitter:image:src" => tw_image = Some(content.to_owned()),
                "description" => meta_description = Some(content.to_owned()),
                _ => {}
            }
        }
    }

    let fallback_title = title_sel
        .as_ref()
        .and_then(|sel| doc.select(sel).next())
        .map(|el| el.text().collect::<String>().trim().to_owned())
        .filter(|s| !s.is_empty());

    let image = og_image.or(tw_image).and_then(|raw| {
        // Resolve relative image URLs against the document URL.
        if let Some(b) = base.as_ref() {
            b.join(&raw).ok().map(|u| u.to_string())
        } else {
            Some(raw)
        }
    });

    FetchedEmbed {
        url: base_url.to_owned(),
        title: og_title.or(tw_title).or(fallback_title),
        description: og_description.or(tw_description).or(meta_description),
        image_url: image,
        site_name: og_site,
        embed_type: EmbedType::Link,
        iframe_url: None,
        iframe_width: None,
        iframe_height: None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn extract_urls_pulls_out_http_and_https_links() {
        let urls = extract_urls("hey check this https://example.com/foo and http://bar.test");
        assert_eq!(
            urls,
            vec![
                "https://example.com/foo".to_string(),
                "http://bar.test/".to_string(),
            ]
        );
    }

    #[test]
    fn extract_urls_ignores_non_http_schemes() {
        let urls = extract_urls("file:///etc/passwd ftp://example.com mailto:a@b.com");
        assert!(urls.is_empty());
    }

    #[test]
    fn extract_urls_strips_trailing_punctuation() {
        let urls = extract_urls("see (https://example.com/foo), ok?");
        assert_eq!(urls, vec!["https://example.com/foo".to_string()]);
    }

    #[test]
    fn extract_urls_dedupes_same_url() {
        let urls = extract_urls("https://example.com https://example.com");
        assert_eq!(urls.len(), 1);
    }

    #[test]
    fn extract_urls_ignores_plain_text() {
        let urls = extract_urls("no links here, just words");
        assert!(urls.is_empty());
    }

    #[test]
    fn parse_opengraph_extracts_og_tags() {
        let html = r#"
            <html><head>
            <meta property="og:title" content="The Title">
            <meta property="og:description" content="A description.">
            <meta property="og:image" content="https://example.com/img.png">
            <meta property="og:site_name" content="Example">
            </head></html>
        "#;
        let e = parse_opengraph("https://example.com/post", html);
        assert_eq!(e.title.as_deref(), Some("The Title"));
        assert_eq!(e.description.as_deref(), Some("A description."));
        assert_eq!(e.image_url.as_deref(), Some("https://example.com/img.png"));
        assert_eq!(e.site_name.as_deref(), Some("Example"));
        assert_eq!(e.embed_type, EmbedType::Link);
        assert!(e.iframe_url.is_none());
    }

    #[test]
    fn parse_opengraph_falls_back_to_title_and_meta_description() {
        let html = r#"
            <html><head>
            <title>Page Title</title>
            <meta name="description" content="meta desc">
            </head></html>
        "#;
        let e = parse_opengraph("https://example.com", html);
        assert_eq!(e.title.as_deref(), Some("Page Title"));
        assert_eq!(e.description.as_deref(), Some("meta desc"));
        assert!(e.image_url.is_none());
    }

    #[test]
    fn parse_opengraph_resolves_relative_image_url() {
        let html = r#"
            <html><head>
            <meta property="og:title" content="T">
            <meta property="og:image" content="/img.png">
            </head></html>
        "#;
        let e = parse_opengraph("https://example.com/a/b", html);
        assert_eq!(e.image_url.as_deref(), Some("https://example.com/img.png"));
    }

    #[test]
    fn parse_opengraph_prefers_og_over_twitter_over_title() {
        let html = r#"
            <html><head>
            <title>Title tag</title>
            <meta name="twitter:title" content="Twitter title">
            <meta property="og:title" content="OG title">
            </head></html>
        "#;
        let e = parse_opengraph("https://example.com", html);
        assert_eq!(e.title.as_deref(), Some("OG title"));
    }

    #[test]
    fn is_unsafe_ip_blocks_loopback_and_private() {
        assert!(is_unsafe_ip("127.0.0.1".parse().unwrap()));
        assert!(is_unsafe_ip("10.0.0.1".parse().unwrap()));
        assert!(is_unsafe_ip("192.168.1.1".parse().unwrap()));
        assert!(is_unsafe_ip("169.254.1.1".parse().unwrap()));
        assert!(is_unsafe_ip("100.64.0.1".parse().unwrap()));
        assert!(is_unsafe_ip("::1".parse().unwrap()));
        assert!(is_unsafe_ip("fc00::1".parse().unwrap()));
        assert!(is_unsafe_ip("fe80::1".parse().unwrap()));
    }

    #[test]
    fn is_unsafe_ip_allows_public() {
        assert!(!is_unsafe_ip("8.8.8.8".parse().unwrap()));
        assert!(!is_unsafe_ip("1.1.1.1".parse().unwrap()));
        assert!(!is_unsafe_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn discover_oembed_link_finds_alternate_link() {
        let html = r#"
            <html><head>
            <link rel="alternate" type="application/json+oembed" href="https://example.com/oembed?url=foo">
            </head></html>
        "#;
        assert_eq!(
            discover_oembed_link(html).as_deref(),
            Some("https://example.com/oembed?url=foo")
        );
    }

    #[test]
    fn discover_oembed_link_returns_none_when_absent() {
        let html = "<html><head><title>No oEmbed</title></head></html>";
        assert!(discover_oembed_link(html).is_none());
    }

    #[test]
    fn discover_oembed_link_ignores_non_json_alternates() {
        // text/xml+oembed is also in the spec but we only support JSON.
        let html = r#"
            <link rel="alternate" type="text/xml+oembed" href="https://example.com/oembed.xml">
            <link rel="alternate" type="application/rss+xml" href="https://example.com/feed">
        "#;
        assert!(discover_oembed_link(html).is_none());
    }

    #[test]
    fn match_provider_matches_bluesky_post_urls() {
        let u =
            Url::parse("https://bsky.app/profile/jonbois.bsky.social/post/3mjx337r4bk2a").unwrap();
        let p = match_provider(&u).unwrap();
        assert_eq!(p.oembed_endpoint, "https://embed.bsky.app/oembed");
        assert_eq!(p.embed_type, EmbedType::Rich);
        assert!(p.extract_rich.is_some());
    }

    #[test]
    fn match_provider_accepts_www_prefix() {
        let u = Url::parse("https://www.bsky.app/profile/alice.bsky.social/post/abc").unwrap();
        assert!(match_provider(&u).is_some());
    }

    #[test]
    fn match_provider_ignores_non_post_bluesky_urls() {
        let u = Url::parse("https://bsky.app/profile/alice.bsky.social").unwrap();
        assert!(match_provider(&u).is_none());
    }

    #[test]
    fn match_provider_matches_youtube_variants() {
        for url in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/abc",
        ] {
            let u = Url::parse(url).unwrap();
            let p = match_provider(&u).unwrap_or_else(|| panic!("expected match for {url}"));
            assert_eq!(p.oembed_endpoint, "https://www.youtube.com/oembed");
            assert_eq!(p.embed_type, EmbedType::Video);
            // YouTube uses the default iframe-src extractor (oEmbed html
            // already carries <iframe src=…>).
            assert!(p.extract_rich.is_none());
        }
    }

    #[test]
    fn match_provider_ignores_unrelated_hosts() {
        let u = Url::parse("https://example.com/anything").unwrap();
        assert!(match_provider(&u).is_none());
    }

    #[test]
    fn extract_bluesky_iframe_builds_did_form_iframe_url() {
        // Trimmed shape of Bluesky's oEmbed html response.
        let html = r#"<blockquote class="bluesky-embed" data-bluesky-uri="at://did:plc:abc/app.bsky.feed.post/3xyz"><p>…</p></blockquote><script async src="https://embed.bsky.app/static/embed.js"></script>"#;
        let oembed = OembedResponse {
            html: Some(html.to_owned()),
            width: Some(600),
            height: None,
            ..OembedResponse::default()
        };
        let (src, w, h) = extract_bluesky_iframe(&oembed).unwrap();
        assert_eq!(
            src,
            "https://embed.bsky.app/embed/did:plc:abc/app.bsky.feed.post/3xyz"
        );
        // Width comes from oEmbed when present; height falls back to our default.
        assert_eq!(w, Some(600));
        assert_eq!(h, Some(480));
    }

    #[test]
    fn extract_bluesky_iframe_returns_none_for_malformed_html() {
        let oembed = OembedResponse {
            html: Some("<blockquote>no data-bluesky-uri</blockquote>".to_owned()),
            ..OembedResponse::default()
        };
        assert!(extract_bluesky_iframe(&oembed).is_none());
    }

    #[test]
    fn provider_embed_builds_video_with_iframe() {
        let oembed = OembedResponse {
            resp_type: Some("video".to_owned()),
            title: Some("A video".to_owned()),
            provider_name: Some("YouTube".to_owned()),
            thumbnail_url: Some("https://i.ytimg.com/vi/abc/hqdefault.jpg".to_owned()),
            html: Some(
                r#"<iframe width="200" height="113" src="https://www.youtube.com/embed/abc"></iframe>"#
                    .to_owned(),
            ),
            width: Some(200),
            height: Some(113),
            ..OembedResponse::default()
        };
        let provider = Provider {
            oembed_endpoint: "https://www.youtube.com/oembed",
            embed_type: EmbedType::Video,
            extract_rich: None,
        };
        let e = provider_embed("https://youtu.be/abc".to_owned(), &oembed, &provider);
        assert_eq!(e.embed_type, EmbedType::Video);
        assert_eq!(
            e.iframe_url.as_deref(),
            Some("https://www.youtube.com/embed/abc")
        );
        assert_eq!(e.iframe_width, Some(200));
        assert_eq!(e.iframe_height, Some(113));
        assert_eq!(e.title.as_deref(), Some("A video"));
        assert_eq!(e.site_name.as_deref(), Some("YouTube"));
    }

    #[test]
    fn provider_embed_degrades_to_link_when_no_iframe_available() {
        let oembed = OembedResponse {
            resp_type: Some("video".to_owned()),
            title: Some("Broken".to_owned()),
            html: Some("<div>no iframe</div>".to_owned()),
            ..OembedResponse::default()
        };
        let provider = Provider {
            oembed_endpoint: "https://example.com/oembed",
            embed_type: EmbedType::Video,
            extract_rich: None,
        };
        let e = provider_embed("https://example.com/x".to_owned(), &oembed, &provider);
        assert_eq!(e.embed_type, EmbedType::Link);
        assert!(e.iframe_url.is_none());
    }

    #[test]
    fn extract_iframe_pulls_src_and_dimensions() {
        let html = r#"<iframe width="560" height="315" src="https://www.youtube.com/embed/abc" frameborder="0"></iframe>"#;
        let (src, w, h) = extract_iframe(html).unwrap();
        assert_eq!(src, "https://www.youtube.com/embed/abc");
        assert_eq!(w, Some(560));
        assert_eq!(h, Some(315));
    }

    #[test]
    fn extract_iframe_rejects_non_https() {
        let html = r#"<iframe src="http://example.com/player"></iframe>"#;
        assert!(extract_iframe(html).is_none());
    }

    #[test]
    fn extract_iframe_rejects_javascript_scheme() {
        let html = r#"<iframe src="javascript:alert(1)"></iframe>"#;
        assert!(extract_iframe(html).is_none());
    }

    #[test]
    fn extract_iframe_returns_none_when_no_iframe() {
        let html = r#"<blockquote>no iframe here</blockquote>"#;
        assert!(extract_iframe(html).is_none());
    }

    #[test]
    fn merge_oembed_video_produces_iframe_embed() {
        let oembed = OembedResponse {
            resp_type: Some("video".to_owned()),
            title: Some("Never Gonna Give You Up".to_owned()),
            provider_name: Some("YouTube".to_owned()),
            thumbnail_url: Some("https://i.ytimg.com/vi/abc/hqdefault.jpg".to_owned()),
            html: Some(
                r#"<iframe width="200" height="113" src="https://www.youtube.com/embed/abc"></iframe>"#
                    .to_owned(),
            ),
            width: Some(200),
            height: Some(113),
            ..OembedResponse::default()
        };
        let og = FetchedEmbed {
            description: Some("Rick Astley music video".to_owned()),
            ..FetchedEmbed::default()
        };
        let e = merge_oembed("https://youtu.be/abc".to_owned(), oembed, og);
        assert_eq!(e.embed_type, EmbedType::Video);
        assert_eq!(e.title.as_deref(), Some("Never Gonna Give You Up"));
        assert_eq!(e.site_name.as_deref(), Some("YouTube"));
        assert_eq!(
            e.iframe_url.as_deref(),
            Some("https://www.youtube.com/embed/abc")
        );
        assert_eq!(e.iframe_width, Some(200));
        assert_eq!(e.iframe_height, Some(113));
        // OG description survives (oEmbed doesn't define one).
        assert_eq!(e.description.as_deref(), Some("Rick Astley music video"));
    }

    #[test]
    fn merge_oembed_rich_without_iframe_degrades_to_link() {
        let oembed = OembedResponse {
            resp_type: Some("rich".to_owned()),
            title: Some("A post".to_owned()),
            html: Some("<blockquote>Just a quote, no iframe</blockquote>".to_owned()),
            ..OembedResponse::default()
        };
        let e = merge_oembed(
            "https://example.com/post".to_owned(),
            oembed,
            FetchedEmbed::default(),
        );
        assert_eq!(e.embed_type, EmbedType::Link);
        assert!(e.iframe_url.is_none());
    }

    #[test]
    fn merge_oembed_photo_uses_url_field_as_image() {
        let oembed = OembedResponse {
            resp_type: Some("photo".to_owned()),
            title: Some("A photo".to_owned()),
            url: Some("https://example.com/photo.jpg".to_owned()),
            width: Some(800),
            height: Some(600),
            ..OembedResponse::default()
        };
        let e = merge_oembed(
            "https://example.com/p/1".to_owned(),
            oembed,
            FetchedEmbed::default(),
        );
        assert_eq!(e.embed_type, EmbedType::Photo);
        assert_eq!(
            e.image_url.as_deref(),
            Some("https://example.com/photo.jpg")
        );
    }

    #[test]
    fn deserialize_flexible_i32_accepts_numbers_and_strings() {
        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "deserialize_flexible_i32")]
            v: Option<i32>,
        }
        let a: T = serde_json::from_str(r#"{"v": 42}"#).unwrap();
        assert_eq!(a.v, Some(42));
        let b: T = serde_json::from_str(r#"{"v": "42"}"#).unwrap();
        assert_eq!(b.v, Some(42));
        let c: T = serde_json::from_str(r#"{"v": null}"#).unwrap();
        assert_eq!(c.v, None);
        let d: T = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(d.v, None);
    }
}
