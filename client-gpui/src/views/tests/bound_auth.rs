//! Controlled wire responses for authentication/control workflows, after real client binding.
use super::*;
pub(super) use crate::api::test_support::{RequestAdapter, Response};
use crate::api::{HttpTransport, legacy::HttpAuth};
pub(super) use reqwest::{Request, StatusCode};
use serde_json::json;

pub(super) fn bound_api(adapter: impl RequestAdapter + 'static) -> Arc<dyn AuthApi> {
    Arc::new(HttpAuth::with_transport(HttpTransport::with_adapter(
        Arc::new(adapter),
    )))
}

pub(super) struct BoundAuth;
impl RequestAdapter for BoundAuth {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, AuthError>> {
        Box::pin(async move {
            let path = request.url().path();
            let (status, body) = match path {
                "/api/v1/auth/login" | "/api/v1/auth/signup" => {
                    assert!(request.headers().get("authorization").is_none());
                    let body: serde_json::Value =
                        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap())
                            .unwrap();
                    (
                        if path.ends_with("signup") {
                            StatusCode::CREATED
                        } else {
                            StatusCode::OK
                        },
                        json!({
                            "user": {"id": "42", "username": body["username"]},
                            "access_token": "secret", "expires_at": "2099-01-01T00:00:00Z"
                        }),
                    )
                }
                "/api/v1/me" => (StatusCode::OK, json!({"id":"42", "username":"Ada"})),
                "/api/v1/auth/logout" => (StatusCode::NO_CONTENT, json!(null)),
                "/api/v1/channels" => (
                    StatusCode::OK,
                    json!({"items":[
                        {"id":"000000000000001", "name":"alpha", "type":"text"},
                        {"id":"000000000000002", "name":"general", "type":"text"}
                    ]}),
                ),
                _ if path.ends_with("/messages") => {
                    let id = path.split('/').nth(4).unwrap();
                    (
                        StatusCode::OK,
                        json!({"items":[{
                        "id":id, "channel_id":id, "author":{"id":"42", "display_name":"Ada"},
                        "text": if id.ends_with('1') { "first line\nsecond line" } else { "other channel" },
                        "created_at":"2026-01-01T00:00:00Z"
                    }], "next_cursor":null}),
                    )
                }
                _ => return Err(AuthError::Unavailable),
            };
            Ok(Response::controlled(status, body.to_string()))
        })
    }
}
