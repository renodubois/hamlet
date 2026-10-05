//! Bound protected requests through the real controls and shared execution path.
use super::bound_auth::*;
use super::*;
use serde_json::json;

struct Contexts {
    requests: Arc<Mutex<Vec<(String, String, String)>>>,
    old_channels: Mutex<Option<async_channel::Receiver<Result<Response, ApiError>>>>,
}

impl RequestAdapter for Contexts {
    fn execute(&self, request: Request) -> ApiFuture<Result<Response, ApiError>> {
        let origin = request.url().origin().ascii_serialization();
        let path = request.url().path().to_owned();
        let token = request
            .headers()
            .get("authorization")
            .map(|value| value.to_str().unwrap().to_owned())
            .unwrap_or_default();
        self.requests
            .lock()
            .unwrap()
            .push((origin.clone(), path.clone(), token));
        if path == "/api/v1/auth/login" {
            let token = if origin.contains("8081") {
                "old-secret"
            } else {
                "new-secret"
            };
            return Box::pin(async move {
                Ok(Response::controlled(
                    StatusCode::OK,
                    json!({
                        "user":{"id":"42", "username":"Ada"}, "access_token":token,
                        "expires_at":"2099-01-01T00:00:00Z"
                    })
                    .to_string(),
                ))
            });
        }
        if path == "/api/v1/channels" && origin.contains("8081") {
            let reply = self.old_channels.lock().unwrap().take().unwrap();
            return Box::pin(async move { reply.recv().await.unwrap() });
        }
        BoundAuth.execute(request)
    }
}

#[gpui_kit::test]
fn protected_contexts_survive_logout_changed_server_and_late_old_results(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for rejected in [false, true] {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (reply, pending) = async_channel::bounded(1);
        let transport = crate::test_support::live::transport(Arc::new(Contexts {
            requests: requests.clone(),
            old_channels: Mutex::new(Some(pending)),
        }));
        let execution =
            crate::runtime::Execution::controlled(cx.background_executor.clone(), 1_800_000_000);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = crate::views::app_shell::open(
                window,
                cx,
                transport,
                crate::storage::Config::default(),
                None,
                execution,
            );
            Root::new(view, window, cx)
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("username", cx);
            window.input("Ada", cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("logout", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("server-url", cx);
            window.press("ctrl-a", cx);
            window.input("http://127.0.0.1:8082", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("password", cx);
            window.input("pass", cx);
            window.click("login", cx);
        });
        cx.run_until_parked();
        // Session shutdown now cancels channel reads as well as selected history.
        assert!(
            reply
                .send_blocking(Ok(Response::controlled(
                    if rejected {
                        StatusCode::UNAUTHORIZED
                    } else {
                        StatusCode::OK
                    },
                    json!({"items":[{"id":"old", "name":"old channel", "type":"text"}]})
                        .to_string(),
                )))
                .is_err()
        );
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .find("session-status")
                    .label()
                    .unwrap()
                    .contains("8082")
            );
            assert_eq!(
                window.find("message-000000000000001").label(),
                Some("first line\nsecond line")
            );
            assert!(window.try_find("refresh-channels").is_none());
            assert!(window.try_find("refresh-history").is_none());
            window.click("channel-000000000000002", cx);
        });
        cx.run_until_parked();
        let requests = requests.lock().unwrap();
        assert!(
            requests
                .iter()
                .any(|(origin, path, token)| origin.ends_with("8081")
                    && path.ends_with("logout")
                    && token == "Bearer old-secret")
        );
        assert!(
            requests
                .iter()
                .filter(|(_, path, _)| path.ends_with("messages"))
                .count()
                >= 2
        );
        for (origin, path, token) in requests
            .iter()
            .filter(|(_, path, _)| !path.ends_with("login"))
        {
            assert_eq!(
                token,
                if origin.ends_with("8081") {
                    "Bearer old-secret"
                } else {
                    "Bearer new-secret"
                },
                "{path}"
            );
        }
    }
}
