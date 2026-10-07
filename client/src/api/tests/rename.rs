use super::*;

#[tokio::test]
async fn rename_contract_decoding_and_authenticated_name_only_request() {
    let adapter = Controlled::new([(
        StatusCode::OK,
        r#"{"id":"000000000000001","name":"New name","type":"text"}"#,
    )]);
    let client = HttpTransport::with_adapter(adapter.clone())
        .server("https://example.com")
        .unwrap()
        .restore_candidate("synthetic".into())
        .unwrap();
    let channel = client
        .rename_channel("000000000000001".into(), "New name".into())
        .await
        .unwrap();
    assert_eq!(channel.name, "New name");
    let requests = adapter.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method(), reqwest::Method::PATCH);
    assert_eq!(requests[0].url().path(), "/api/v1/channels/000000000000001");
    assert_eq!(
        requests[0].headers()[reqwest::header::AUTHORIZATION],
        "Bearer synthetic"
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            requests[0].body().unwrap().as_bytes().unwrap()
        )
        .unwrap(),
        serde_json::json!({"name":"New name"})
    );
}

#[tokio::test]
async fn rename_maps_rejections_and_unconfirmed_responses_without_replay() {
    for (status, body, expected) in [
        (
            StatusCode::BAD_REQUEST,
            r#"{"error":{"code":"bad_request"}}"#,
            ApiError::InvalidInput,
        ),
        (StatusCode::UNAUTHORIZED, "", ApiError::AlreadyInvalid),
        (
            StatusCode::NOT_FOUND,
            r#"{"error":{"code":"not_found"}}"#,
            ApiError::NotFound,
        ),
        (
            StatusCode::CONFLICT,
            r#"{"error":{"code":"conflict"}}"#,
            ApiError::Conflict,
        ),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "",
            ApiError::ServerFailure,
        ),
        (StatusCode::OK, "malformed", ApiError::InvalidResponse),
        (
            StatusCode::OK,
            r#"{"id":"wrong","name":"New","type":"text"}"#,
            ApiError::InvalidResponse,
        ),
        (
            StatusCode::OK,
            r#"{"id":"c","name":"New","type":"voice"}"#,
            ApiError::InvalidResponse,
        ),
        (
            StatusCode::CREATED,
            r#"{"id":"c","name":"New","type":"text"}"#,
            ApiError::Unavailable,
        ),
    ] {
        let adapter = Controlled::new([(status, body)]);
        let client = HttpTransport::with_adapter(adapter.clone())
            .server("https://example.com")
            .unwrap()
            .restore_candidate("synthetic".into())
            .unwrap();
        assert_eq!(
            client.rename_channel("c".into(), "New".into()).await,
            Err(expected)
        );
        assert_eq!(adapter.requests.lock().unwrap().len(), 1);
    }
}
