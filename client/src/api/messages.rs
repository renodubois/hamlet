use super::response::{BAD_REQUEST, FORBIDDEN, NOT_FOUND, expect, expect_json};
use super::{ApiError, ApiFuture, AuthenticatedClient, Message, Page};
use hamlet_protocol::{CreateMessage, History, Message as ProtocolMessage};
use reqwest::{Method, StatusCode};

pub(super) fn decode_message(wire: ProtocolMessage, channel_id: &str) -> Result<Message, ApiError> {
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
    pub fn history_page(
        &self,
        channel_id: String,
        before: Option<String>,
    ) -> ApiFuture<Result<Page, ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            let mut request = client.request(
                Method::GET,
                &["api", "v1", "channels", &channel_id, "messages"],
            )?;
            if let Some(cursor) = &before {
                // Server-issued cursors remain opaque, including reserved URL characters.
                request = request.query(&[("before", cursor)]);
            }
            let wire: History =
                expect_json(client.send(request).await?, StatusCode::OK, &[]).await?;
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

    pub fn send_message(
        &self,
        channel_id: String,
        text: String,
    ) -> ApiFuture<Result<Message, ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            // One POST only; timeout/malformed success never triggers replay.
            let request = client
                .request(
                    Method::POST,
                    &["api", "v1", "channels", &channel_id, "messages"],
                )?
                .json(&CreateMessage { text });
            let response = client.send(request).await?;
            let wire =
                expect_json(response, StatusCode::CREATED, &[BAD_REQUEST, NOT_FOUND]).await?;
            decode_message(wire, &channel_id)
        })
    }

    /// Delete once using this session's immutable server and bearer binding.
    pub fn delete_message(
        &self,
        channel_id: String,
        message_id: String,
    ) -> ApiFuture<Result<(), ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            // No body, optimistic mutation, or automatic retry.
            let request = client.request(
                Method::DELETE,
                &[
                    "api",
                    "v1",
                    "channels",
                    &channel_id,
                    "messages",
                    &message_id,
                ],
            )?;
            let response = client.send(request).await?;
            expect(
                response,
                StatusCode::NO_CONTENT,
                &[BAD_REQUEST, FORBIDDEN, NOT_FOUND],
            )
            .await?;
            Ok(())
        })
    }
}
