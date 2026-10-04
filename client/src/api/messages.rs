use super::{ApiError, ApiFuture, AuthenticatedClient, Message, Page};
use super::{auth::protected_response, client::Response, wire::*};
use reqwest::{Method, StatusCode, Url};

async fn decode_created_message(response: Response, channel_id: &str) -> Result<Message, ApiError> {
    match response.status() {
        StatusCode::CREATED => {
            let wire: WireMessage = response
                .json()
                .await
                .map_err(|_| ApiError::InvalidResponse)?;
            decode_message(wire, channel_id)
        }
        StatusCode::UNAUTHORIZED => Err(ApiError::AlreadyInvalid),
        StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND => {
            let status = response.status();
            let body: WireError = response
                .json()
                .await
                .map_err(|_| ApiError::InvalidResponse)?;
            match (status, body.error.code.as_str()) {
                (StatusCode::BAD_REQUEST, "bad_request") => Err(ApiError::InvalidInput),
                (StatusCode::NOT_FOUND, "not_found") => Err(ApiError::NotFound),
                _ => Err(ApiError::InvalidResponse),
            }
        }
        StatusCode::INTERNAL_SERVER_ERROR => Err(ApiError::ServerFailure),
        _ => Err(ApiError::Unavailable),
    }
}

pub(super) fn decode_message(wire: WireMessage, channel_id: &str) -> Result<Message, ApiError> {
    if wire.id.is_empty()
        || wire.channel_id.is_empty()
        || wire.channel_id != channel_id
        || wire.author.id.is_empty()
        || wire.author.display_name.is_empty()
    {
        return Err(ApiError::InvalidResponse);
    }
    Ok(Message {
        id: wire.id,
        channel_id: wire.channel_id,
        author_id: wire.author.id,
        author_name: wire.author.display_name,
        text: wire.text,
        created_at: wire.created_at.to_rfc3339(),
    })
}

impl AuthenticatedClient {
    fn message_url(&self, channel_id: &str) -> Result<Url, ApiError> {
        if channel_id.is_empty()
            || channel_id.contains(['/', '\\'])
            || channel_id == "."
            || channel_id == ".."
        {
            return Err(ApiError::InvalidResponse);
        }
        let mut url = self.server_url().clone();
        url.path_segments_mut()
            .map_err(|_| ApiError::InvalidResponse)?
            .extend(["api", "v1", "channels", channel_id, "messages"]);
        Ok(url)
    }

    pub fn send_message(
        &self,
        channel_id: String,
        text: String,
    ) -> ApiFuture<Result<Message, ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            // One POST only; timeout/malformed success never triggers replay.
            let request = client
                .request(Method::POST, client.message_url(&channel_id)?)
                .json(&serde_json::json!({"text": text}));
            decode_created_message(
                client.0.server.0.transport.send(request).await?,
                &channel_id,
            )
            .await
        })
    }

    pub fn history_page(
        &self,
        channel_id: String,
        before: Option<String>,
    ) -> ApiFuture<Result<Page, ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            let mut url = client.message_url(&channel_id)?;
            if let Some(cursor) = before.as_ref() {
                // Server-issued cursors remain opaque, including reserved URL characters.
                url.query_pairs_mut().append_pair("before", cursor);
            }
            let request = client.request(Method::GET, url);
            let wire: WireHistory =
                protected_response(client.0.server.0.transport.send(request).await?).await?;
            let items = wire
                .items
                .into_iter()
                .map(|item| decode_message(item, &channel_id))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Page {
                items,
                next_cursor: wire.next_cursor,
            })
        })
    }
}
