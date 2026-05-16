use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageTextSegment {
    Text(String),
    Link { text: String, url: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ExternalLinkStatus {
    #[default]
    Idle,
    Opening {
        url: String,
    },
    Failed(String),
}

impl ExternalLinkStatus {
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Opening { .. } => Some("Opening link in your browser…"),
            Self::Failed(message) => Some(message.as_str()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExternalOpenError {
    #[error("malformed URL: {0}")]
    Malformed(String),
    #[error("unsupported URL scheme: {0}")]
    UnsupportedScheme(String),
    #[error("platform external-open failed: {0}")]
    Platform(String),
}

impl ExternalOpenError {
    pub fn user_message(&self, target: &str) -> String {
        match self {
            Self::Malformed(_) => format!("Could not open link: {target} is not a valid URL."),
            Self::UnsupportedScheme(_) => {
                "Could not open link: only http:// and https:// URLs are supported.".to_string()
            }
            Self::Platform(message) => format!("Could not open link: {message}"),
        }
    }
}

pub trait ExternalOpenService {
    fn open_external_url(&self, target: &str) -> Result<(), ExternalOpenError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PlatformExternalOpen;

impl ExternalOpenService for PlatformExternalOpen {
    fn open_external_url(&self, target: &str) -> Result<(), ExternalOpenError> {
        let url = validate_external_url(target)?;
        open_with_platform(url.as_str())
    }
}

pub fn parse_message_text(text: &str) -> Vec<MessageTextSegment> {
    let links = recognized_urls(text);

    if links.is_empty() {
        return vec![MessageTextSegment::Text(text.to_string())];
    }

    let mut segments = Vec::new();
    let mut cursor = 0;

    for link in links {
        if link.start > cursor {
            segments.push(MessageTextSegment::Text(
                text[cursor..link.start].to_string(),
            ));
        }

        let link_text = text[link.start..link.end].to_string();
        segments.push(MessageTextSegment::Link {
            text: link_text.clone(),
            url: link_text,
        });
        cursor = link.end;
    }

    if cursor < text.len() {
        segments.push(MessageTextSegment::Text(text[cursor..].to_string()));
    }

    segments
}

pub fn validate_external_url(target: &str) -> Result<Url, ExternalOpenError> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return Err(ExternalOpenError::Malformed("URL is empty".to_string()));
    }

    let url =
        Url::parse(trimmed).map_err(|error| ExternalOpenError::Malformed(error.to_string()))?;
    match url.scheme() {
        "http" | "https" => {}
        scheme => return Err(ExternalOpenError::UnsupportedScheme(scheme.to_string())),
    }

    if url.host_str().is_none() {
        return Err(ExternalOpenError::Malformed(
            "URL must include a host".to_string(),
        ));
    }

    Ok(url)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecognizedUrl {
    start: usize,
    end: usize,
}

fn recognized_urls(text: &str) -> Vec<RecognizedUrl> {
    let lower = text.to_ascii_lowercase();
    let mut links = Vec::new();
    let mut search_start = 0;

    while let Some((start, scheme_len)) = next_url_scheme(&lower, search_start) {
        let scheme_end = start.saturating_add(scheme_len);
        search_start = scheme_end;

        if !has_url_start_boundary(text, start) {
            continue;
        }

        let candidate_end = raw_candidate_end(text, start);
        let end = trim_trailing_url_punctuation(text, start, candidate_end);

        if end <= scheme_end {
            continue;
        }

        let candidate = &text[start..end];
        if validate_external_url(candidate).is_ok() {
            links.push(RecognizedUrl { start, end });
            search_start = end;
        }
    }

    links
}

fn next_url_scheme(lowercase_text: &str, search_start: usize) -> Option<(usize, usize)> {
    let http = lowercase_text[search_start..]
        .find("http://")
        .map(|offset| (search_start + offset, "http://".len()));
    let https = lowercase_text[search_start..]
        .find("https://")
        .map(|offset| (search_start + offset, "https://".len()));

    match (http, https) {
        (Some(http), Some(https)) => Some(if http.0 <= https.0 { http } else { https }),
        (Some(http), None) => Some(http),
        (None, Some(https)) => Some(https),
        (None, None) => None,
    }
}

fn has_url_start_boundary(text: &str, start: usize) -> bool {
    if start == 0 {
        return true;
    }

    text[..start]
        .chars()
        .next_back()
        .is_some_and(|ch| ch.is_whitespace() || is_opening_boundary(ch))
}

fn is_opening_boundary(ch: char) -> bool {
    matches!(
        ch,
        '(' | '[' | '{' | '<' | '"' | '\'' | '“' | '‘' | ':' | ';' | ','
    )
}

fn raw_candidate_end(text: &str, start: usize) -> usize {
    text[start..]
        .char_indices()
        .find_map(|(offset, ch)| is_candidate_delimiter(ch).then_some(start + offset))
        .unwrap_or(text.len())
}

fn is_candidate_delimiter(ch: char) -> bool {
    ch.is_whitespace() || ch.is_control() || ch == '<'
}

fn trim_trailing_url_punctuation(text: &str, start: usize, candidate_end: usize) -> usize {
    let mut end = candidate_end;

    while end > start {
        let Some(ch) = text[start..end].chars().next_back() else {
            break;
        };

        if is_always_trailing_punctuation(ch)
            || is_unbalanced_closing_delimiter(&text[start..end], ch)
        {
            end -= ch.len_utf8();
        } else {
            break;
        }
    }

    end
}

fn is_always_trailing_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '.' | ',' | '!' | '?' | ';' | ':' | '"' | '\'' | '”' | '’'
    )
}

fn is_unbalanced_closing_delimiter(candidate: &str, closing: char) -> bool {
    let Some(opening) = matching_opening_delimiter(closing) else {
        return false;
    };

    let openings = candidate.chars().filter(|ch| *ch == opening).count();
    let closings = candidate.chars().filter(|ch| *ch == closing).count();

    closings > openings
}

fn matching_opening_delimiter(closing: char) -> Option<char> {
    match closing {
        ')' => Some('('),
        ']' => Some('['),
        '}' => Some('{'),
        '>' => Some('<'),
        _ => None,
    }
}

fn open_with_platform(url: &str) -> Result<(), ExternalOpenError> {
    #[cfg(target_os = "macos")]
    {
        run_open_command("open", [url])
    }

    #[cfg(target_os = "windows")]
    {
        run_open_command("rundll32", ["url.dll,FileProtocolHandler", url])
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        run_open_command("xdg-open", [url])
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
    {
        let _ = url;
        Err(ExternalOpenError::Platform(
            "external link opening is not supported on this platform".to_string(),
        ))
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", unix))]
fn run_open_command<const N: usize>(
    command: &str,
    args: [&str; N],
) -> Result<(), ExternalOpenError> {
    let status = std::process::Command::new(command)
        .args(args)
        .status()
        .map_err(|error| ExternalOpenError::Platform(error.to_string()))?;

    if status.success() {
        Ok(())
    } else {
        Err(ExternalOpenError::Platform(format!(
            "platform opener exited with {status}"
        )))
    }
}
