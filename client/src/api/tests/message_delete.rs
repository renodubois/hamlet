use super::*;

#[tokio::test]
async fn message_delete_binds_both_identities_bearer_and_empty_body_and_decodes_failures() {
    for (status, body, expected) in [
        (StatusCode::NO_CONTENT, "", Ok(())),
        (StatusCode::UNAUTHORIZED, "", Err(ApiError::AlreadyInvalid)),
        (
            StatusCode::BAD_REQUEST,
            r#"{"error":{"code":"bad_request"}}"#,
            Err(ApiError::InvalidInput),
        ),
        (
            StatusCode::FORBIDDEN,
            r#"{"error":{"code":"forbidden"}}"#,
            Err(ApiError::Forbidden),
        ),
        (
            StatusCode::NOT_FOUND,
            r#"{"error":{"code":"not_found"}}"#,
            Err(ApiError::NotFound),
        ),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "",
            Err(ApiError::ServerFailure),
        ),
        (
            StatusCode::FORBIDDEN,
            "not JSON",
            Err(ApiError::InvalidResponse),
        ),
        (
            StatusCode::FORBIDDEN,
            r#"{"error":{"code":"not_found"}}"#,
            Err(ApiError::InvalidResponse),
        ),
        (
            StatusCode::NOT_FOUND,
            r#"{"error":{"code":"forbidden"}}"#,
            Err(ApiError::InvalidResponse),
        ),
        (
            StatusCode::BAD_REQUEST,
            r#"{"error":{"code":"other"}}"#,
            Err(ApiError::InvalidResponse),
        ),
        (StatusCode::OK, "{}", Err(ApiError::Unavailable)),
    ] {
        let adapter = Controlled::new([(status, body)]);
        let client = HttpTransport::with_adapter(adapter.clone())
            .server("https://example.com:8443")
            .unwrap()
            .restore_candidate("synthetic".into())
            .unwrap();
        assert_eq!(
            client
                .clone()
                .delete_message("000000000000001".into(), "000000000000002".into())
                .await,
            expected
        );
        let requests = adapter.requests.lock().unwrap();
        assert_eq!(requests.len(), 1, "never replay a delete");
        assert_eq!(requests[0].method(), reqwest::Method::DELETE);
        assert_eq!(
            requests[0].url().as_str(),
            "https://example.com:8443/api/v1/channels/000000000000001/messages/000000000000002"
        );
        assert_eq!(
            requests[0].headers()[reqwest::header::AUTHORIZATION],
            "Bearer synthetic"
        );
        assert!(requests[0].body().is_none());
    }
}

#[tokio::test]
async fn message_delete_rejects_unsafe_path_segments_before_transport() {
    let adapter = Controlled::new([]);
    let client = HttpTransport::with_adapter(adapter.clone())
        .server("https://example.com")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    for invalid in ["", ".", "..", "a/b", "a\\b"] {
        assert_eq!(
            client
                .delete_message(invalid.into(), "000000000000002".into())
                .await,
            Err(ApiError::InvalidResponse)
        );
        assert_eq!(
            client
                .delete_message("000000000000001".into(), invalid.into())
                .await,
            Err(ApiError::InvalidResponse)
        );
    }
    assert!(adapter.requests.lock().unwrap().is_empty());
}
