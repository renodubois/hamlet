use super::*;

#[tokio::test]
async fn delete_decodes_empty_success_and_typed_failures_with_bound_bearer() {
    for (status, body, expected) in [
        (StatusCode::NO_CONTENT, "", Ok(())),
        (StatusCode::UNAUTHORIZED, "", Err(ApiError::AlreadyInvalid)),
        (StatusCode::NOT_FOUND, "", Err(ApiError::NotFound)),
        (
            StatusCode::CONFLICT,
            r#"{"error":{"code":"conflict","message":"Cannot delete the last active channel"}}"#,
            Err(ApiError::Conflict),
        ),
        (
            StatusCode::BAD_REQUEST,
            r#"{"error":{"code":"bad_request","message":"Invalid ID"}}"#,
            Err(ApiError::InvalidInput),
        ),
        (
            StatusCode::CONFLICT,
            "not JSON",
            Err(ApiError::InvalidResponse),
        ),
        (
            StatusCode::CONFLICT,
            r#"{"error":{"code":"other","message":"wrong"}}"#,
            Err(ApiError::InvalidResponse),
        ),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "",
            Err(ApiError::ServerFailure),
        ),
        (StatusCode::OK, "{}", Err(ApiError::Unavailable)),
    ] {
        let adapter = Controlled::new([(status, body)]);
        let client = HttpTransport::with_adapter(adapter.clone())
            .server("https://example.com")
            .unwrap()
            .restore_candidate("synthetic".into())
            .unwrap();
        assert_eq!(
            client.delete_channel("000000000000001".into()).await,
            expected
        );
        let requests = adapter.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method(), reqwest::Method::DELETE);
        assert_eq!(
            requests[0].url().as_str(),
            "https://example.com/api/v1/channels/000000000000001"
        );
        assert_eq!(
            requests[0].headers()[reqwest::header::AUTHORIZATION],
            "Bearer synthetic"
        );
        assert!(requests[0].body().is_none());
    }
}
