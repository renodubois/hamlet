use hamlet_client_iced::app::{AppEffect, AppMessage, Route};
use hamlet_client_iced::external_open::{
    ExternalLinkStatus, ExternalOpenError, MessageTextSegment, parse_message_text,
};
use hamlet_client_iced::test_support::harness::ReducerHarness;

fn link_urls(text: &str) -> Vec<String> {
    parse_message_text(text)
        .into_iter()
        .filter_map(|segment| match segment {
            MessageTextSegment::Link { url, .. } => Some(url),
            MessageTextSegment::Text(_) => None,
        })
        .collect()
}

fn signed_in_harness() -> ReducerHarness {
    let mut harness = ReducerHarness::boot();
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();
    assert_eq!(harness.state.route, Route::SignedIn);
    harness
}

#[test]
fn url_parsing_keeps_non_url_text_plain() {
    assert_eq!(
        parse_message_text("hello from hamlet"),
        vec![MessageTextSegment::Text("hello from hamlet".to_string())]
    );
    assert!(link_urls("email alice@example.test or say xhttps://example.test").is_empty());
}

#[test]
fn url_parsing_recognizes_http_and_https_urls() {
    assert_eq!(
        link_urls("open https://example.test and http://localhost:3030/path"),
        vec![
            "https://example.test".to_string(),
            "http://localhost:3030/path".to_string(),
        ]
    );
}

#[test]
fn url_parsing_handles_multiple_links_in_one_message() {
    let segments =
        parse_message_text("one https://one.example two https://two.example?q=1#frag done");

    assert_eq!(
        segments,
        vec![
            MessageTextSegment::Text("one ".to_string()),
            MessageTextSegment::Link {
                text: "https://one.example".to_string(),
                url: "https://one.example".to_string(),
            },
            MessageTextSegment::Text(" two ".to_string()),
            MessageTextSegment::Link {
                text: "https://two.example?q=1#frag".to_string(),
                url: "https://two.example?q=1#frag".to_string(),
            },
            MessageTextSegment::Text(" done".to_string()),
        ]
    );
}

#[test]
fn url_parsing_respects_punctuation_boundaries() {
    assert_eq!(
        link_urls("see (https://example.test/a), then https://example.org/b?x=1."),
        vec![
            "https://example.test/a".to_string(),
            "https://example.org/b?x=1".to_string(),
        ]
    );
    assert_eq!(
        link_urls("balanced https://example.test/path_(ok) end"),
        vec!["https://example.test/path_(ok)".to_string()]
    );
}

#[test]
fn unsupported_and_malformed_urls_are_not_linkified() {
    assert!(link_urls("ftp://example.test javascript:alert(1) https://").is_empty());
}

#[test]
fn opening_supported_link_delegates_to_external_open_service() {
    let mut harness = signed_in_harness();

    let effects = harness.dispatch(AppMessage::OpenExternalUrlRequested(
        "https://example.test/path".to_string(),
    ));

    assert_eq!(
        effects,
        vec![AppEffect::OpenExternalUrl(
            "https://example.test/path".to_string()
        )]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .map(|state| &state.external_link_status),
        Some(&ExternalLinkStatus::Opening {
            url: "https://example.test/path".to_string(),
        })
    );

    harness.run_all_effects();

    assert_eq!(
        harness.external_open.opened_urls().unwrap_or_default(),
        vec!["https://example.test/path".to_string()]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .map(|state| &state.external_link_status),
        Some(&ExternalLinkStatus::Idle)
    );
}

#[test]
fn opening_unsupported_link_fails_safely_without_platform_call() {
    let mut harness = signed_in_harness();

    let effects = harness.dispatch(AppMessage::OpenExternalUrlRequested(
        "file:///etc/passwd".to_string(),
    ));

    assert!(effects.is_empty());
    assert!(
        harness
            .external_open
            .opened_urls()
            .unwrap_or_default()
            .is_empty()
    );
    assert!(matches!(
        harness
            .state
            .signed_in
            .as_ref()
            .map(|state| &state.external_link_status),
        Some(ExternalLinkStatus::Failed(message)) if message.contains("only http:// and https://")
    ));
}

#[test]
fn platform_open_failure_is_visible() {
    let mut harness = signed_in_harness();
    assert!(
        harness
            .external_open
            .fail_next(ExternalOpenError::Platform(
                "no browser available".to_string()
            ))
            .is_ok()
    );

    harness.dispatch(AppMessage::OpenExternalUrlRequested(
        "https://example.test".to_string(),
    ));
    harness.run_all_effects();

    assert!(matches!(
        harness
            .state
            .signed_in
            .as_ref()
            .map(|state| &state.external_link_status),
        Some(ExternalLinkStatus::Failed(message)) if message.contains("no browser available")
    ));
}
