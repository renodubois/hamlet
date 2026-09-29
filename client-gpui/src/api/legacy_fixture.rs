//! Temporary authentication response adapter for unmigrated protected-operation fixtures.
//! No production dispatch uses this adapter. Remove with AuthApi in #52.
use super::{
    ApiError, ApiFuture,
    legacy::AuthApi,
    test_support::{RequestAdapter, Response},
};
use reqwest::{Request, StatusCode};
use std::sync::Arc;

pub(super) struct AuthenticationAdapter<A: AuthApi + ?Sized>(pub Arc<A>);
impl<A: AuthApi + ?Sized> RequestAdapter for AuthenticationAdapter<A> {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        let api = self.0.clone();
        Box::pin(async move {
            let origin = request.url().origin().ascii_serialization();
            let token = request
                .headers()
                .get("authorization")
                .and_then(|h| h.to_str().ok())
                .and_then(|h| h.strip_prefix("Bearer "))
                .unwrap_or("")
                .to_owned();
            match request.url().path() {
                "/api/v1/auth/login" | "/api/v1/auth/signup" => {
                    let signup = request.url().path().ends_with("signup");
                    let body: serde_json::Value =
                        serde_json::from_slice(request.body().unwrap().as_bytes().unwrap())
                            .unwrap();
                    let username = body["username"].as_str().unwrap().to_owned();
                    let password = body["password"].as_str().unwrap().to_owned();
                    let result = if signup {
                        api.signup(origin, username, password)
                    } else {
                        api.login(origin, username, password)
                    }
                    .await;
                    match result {
                        Ok(login) => Ok(Response::controlled(if signup { StatusCode::CREATED } else { StatusCode::OK }, serde_json::json!({
                            "user": { "id": login.user.id, "username": login.user.username },
                            "access_token": login.token,
                            "expires_at": chrono::DateTime::from_timestamp(login.expires_at, 0).unwrap().to_rfc3339(),
                        }).to_string())),
                        Err(error) => Err(error),
                    }
                }
                "/api/v1/me" => api.current_user(origin, token).await.map(|user| {
                    Response::controlled(
                        StatusCode::OK,
                        serde_json::json!({"id":user.id,"username":user.username}).to_string(),
                    )
                }),
                "/api/v1/auth/logout" => api
                    .logout(origin, token)
                    .await
                    .map(|()| Response::controlled(StatusCode::NO_CONTENT, "")),
                _ => Err(ApiError::Unavailable),
            }
        })
    }
}
