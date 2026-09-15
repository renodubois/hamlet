use reqwest::header::{self, HeaderMap};
use serde::Deserialize;

// NOTE(reno): This is copied directly over from the server - in the future
// I might want to make a shared types crate that both the server & the client can use
#[derive(Clone, Debug, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub email_verified: bool,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LoginResult {
    pub cookies: HeaderMap,
    pub user: User,
    pub server_url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChannelResponse {
    pub id: i64,
    pub name: String,
    pub position: i64,
    #[serde(rename = "type")]
    pub channel_type: String,
}

#[derive(Clone)]
pub struct ApiClient {
    pub url: String,
    pub cookies: HeaderMap,
}

impl ApiClient {
    pub async fn login(
        url: String,
        username: String,
        password: String,
    ) -> Result<LoginResult, reqwest::Error> {
        match reqwest::Client::new()
            .post(url.clone() + "/login")
            .json(&serde_json::json!({
                "username": username,
                "password": password
            }))
            .send()
            .await
        {
            Ok(res) => {
                let cookies: HeaderMap = res
                    .headers()
                    .iter()
                    .filter(|(k, _)| k.to_string() == "set-cookie")
                    // TODO(reno): If to_str errors, it's because we got non-ASCII values back,
                    // but I don't expect that from our server.
                    .map(|(_, v)| (header::COOKIE, v.to_owned()))
                    .collect();
                match res.json::<User>().await {
                    Ok(body) => {
                        return Ok(LoginResult {
                            cookies: cookies,
                            user: body,
                            server_url: url,
                        });
                    }
                    Err(err) => Err(err),
                }
            }
            Err(err) => Err(err),
        }
    }

    pub async fn get_channels(&self) -> Result<Vec<ChannelResponse>, reqwest::Error> {
        match reqwest::Client::new()
            .get(self.url.clone() + "/channels")
            .headers(self.cookies.clone())
            .send()
            .await?
            .json::<Vec<ChannelResponse>>()
            .await
        {
            Ok(res) => Ok(res),
            Err(err) => Err(err),
        }
    }

    pub async fn get_messages_for_channel(&self) -> Result<Vec, reqwest::Error> {}
}
