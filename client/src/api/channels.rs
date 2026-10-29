use super::response::{BAD_REQUEST, CONFLICT, Coded, expect, expect_json};
use super::{ApiError, ApiFuture, AuthenticatedClient, Channel};
use hamlet_protocol::{
    Channel as ProtocolChannel, ChannelList, ChannelType, CreateChannel, RenameChannel,
};
use reqwest::{Method, StatusCode};

const MUTATION_ERRORS: &[Coded] = &[BAD_REQUEST, CONFLICT];

pub(super) fn decode_channel(wire: ProtocolChannel) -> Result<Channel, ApiError> {
    if wire.id.is_empty() || wire.name.is_empty() || !matches!(wire.kind, ChannelType::Text) {
        return Err(ApiError::InvalidResponse);
    }
    Ok(Channel {
        id: wire.id,
        name: wire.name,
    })
}

impl AuthenticatedClient {
    pub fn channels(&self) -> ApiFuture<Result<Vec<Channel>, ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            let request = client.request(Method::GET, &["api", "v1", "channels"])?;
            let wire: ChannelList =
                expect_json(client.send(request).await?, StatusCode::OK, &[]).await?;
            wire.items.into_iter().map(decode_channel).collect()
        })
    }

    pub fn create_channel(&self, name: String) -> ApiFuture<Result<Channel, ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            let request = client
                .request(Method::POST, &["api", "v1", "channels"])?
                .json(&CreateChannel {
                    name,
                    kind: ChannelType::Text,
                });
            let response = client.send(request).await?;
            decode_channel(expect_json(response, StatusCode::CREATED, MUTATION_ERRORS).await?)
        })
    }

    pub fn rename_channel(&self, id: String, name: String) -> ApiFuture<Result<Channel, ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            let request = client
                .request(Method::PATCH, &["api", "v1", "channels", &id])?
                .json(&RenameChannel { name });
            let response = client.send(request).await?;
            let channel =
                decode_channel(expect_json(response, StatusCode::OK, MUTATION_ERRORS).await?)?;
            if channel.id != id {
                return Err(ApiError::InvalidResponse);
            }
            Ok(channel)
        })
    }

    pub fn delete_channel(&self, id: String) -> ApiFuture<Result<(), ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            let request = client.request(Method::DELETE, &["api", "v1", "channels", &id])?;
            expect(
                client.send(request).await?,
                StatusCode::NO_CONTENT,
                MUTATION_ERRORS,
            )
            .await?;
            Ok(())
        })
    }
}
