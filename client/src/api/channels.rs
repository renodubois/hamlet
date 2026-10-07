use super::{ApiError, ApiFuture, AuthenticatedClient, Channel};
use super::{auth::protected_response, client::Response, error::ErrorResponse};
use hamlet_protocol::{
    Channel as ProtocolChannel, ChannelList, ChannelType, CreateChannel, RenameChannel,
};
use reqwest::{Method, StatusCode};

async fn decode_mutated_channel(
    response: Response,
    success: StatusCode,
) -> Result<Channel, ApiError> {
    match response.status() {
        status if status == success => {
            let wire: ProtocolChannel = response
                .json()
                .await
                .map_err(|_| ApiError::InvalidResponse)?;
            decode_channel(wire)
        }
        StatusCode::UNAUTHORIZED => Err(ApiError::AlreadyInvalid),
        StatusCode::NOT_FOUND => Err(ApiError::NotFound),
        StatusCode::BAD_REQUEST | StatusCode::CONFLICT => {
            let status = response.status();
            let body: ErrorResponse = response
                .json()
                .await
                .map_err(|_| ApiError::InvalidResponse)?;
            match (status, body.error.code.as_str()) {
                (StatusCode::BAD_REQUEST, "bad_request") => Err(ApiError::InvalidInput),
                (StatusCode::CONFLICT, "conflict") => Err(ApiError::Conflict),
                _ => Err(ApiError::InvalidResponse),
            }
        }
        StatusCode::INTERNAL_SERVER_ERROR => Err(ApiError::ServerFailure),
        _ => Err(ApiError::Unavailable),
    }
}

pub(super) fn decode_channel(wire: ProtocolChannel) -> Result<Channel, ApiError> {
    if wire.id.is_empty()
        || wire.name.is_empty()
        || !matches!(wire.kind, hamlet_protocol::ChannelType::Text)
    {
        return Err(ApiError::InvalidResponse);
    }
    Ok(Channel {
        id: wire.id,
        name: wire.name,
    })
}

impl AuthenticatedClient {
    pub fn rename_channel(&self, id: String, name: String) -> ApiFuture<Result<Channel, ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            let request = client
                .request(
                    Method::PATCH,
                    client.0.server.endpoint(&format!("api/v1/channels/{id}")),
                )
                .json(&RenameChannel { name });
            let channel = decode_mutated_channel(
                client.0.server.0.transport.send(request).await?,
                StatusCode::OK,
            )
            .await?;
            if channel.id != id {
                return Err(ApiError::InvalidResponse);
            }
            Ok(channel)
        })
    }
    pub fn channels(&self) -> ApiFuture<Result<Vec<Channel>, ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            let request = client.request(Method::GET, client.0.server.endpoint("api/v1/channels"));
            let wire: ChannelList =
                protected_response(client.0.server.0.transport.send(request).await?).await?;
            wire.items.into_iter().map(decode_channel).collect()
        })
    }

    pub fn create_channel(&self, name: String) -> ApiFuture<Result<Channel, ApiError>> {
        let client = self.clone();
        Box::pin(async move {
            let request = client
                .request(Method::POST, client.0.server.endpoint("api/v1/channels"))
                .json(&CreateChannel {
                    name,
                    kind: ChannelType::Text,
                });
            decode_mutated_channel(
                client.0.server.0.transport.send(request).await?,
                StatusCode::CREATED,
            )
            .await
        })
    }
}
