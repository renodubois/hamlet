use hamlet_client_iced::protocol::{
    BroadcastEvent, ChannelKind, CreateChannelRequest, Embed, ErrorDetails, ErrorEnvelope,
    LoginRequest, Message, MessageEmbedsUpdatedEvent, RegisterRequest, ReorderChannelsRequest,
    SendMessageRequest, SuppressEmbedsRequest, VoiceParticipant, VoiceParticipantLeftEvent,
    VoiceParticipantSpeakingEvent, VoiceSpeakingRequest, VoiceToken,
};

#[test]
fn auth_requests_match_server_json_shape() -> Result<(), Box<dyn std::error::Error>> {
    let login = serde_json::to_value(LoginRequest {
        username: "alice".to_string(),
        password: "secret".to_string(),
    })?;
    let register = serde_json::to_value(RegisterRequest {
        username: "alice".to_string(),
        password: "secret".to_string(),
        email: None,
    })?;

    assert_eq!(login["username"], "alice");
    assert_eq!(login["password"], "secret");
    assert_eq!(register["username"], "alice");
    assert_eq!(register["password"], "secret");
    assert!(register.get("email").is_none());

    let create_channel = serde_json::to_value(CreateChannelRequest {
        name: "native".to_string(),
        kind: ChannelKind::Voice,
    })?;
    assert_eq!(create_channel["name"], "native");
    assert_eq!(create_channel["type"], "voice");

    let reorder_channels = serde_json::to_value(ReorderChannelsRequest { ids: vec![2, 1] })?;
    assert_eq!(reorder_channels["ids"][0], 2);
    assert_eq!(reorder_channels["ids"][1], 1);

    let send_message = serde_json::to_value(SendMessageRequest {
        text: "hello".to_string(),
    })?;
    assert_eq!(send_message["text"], "hello");

    let suppress_embeds = serde_json::to_value(SuppressEmbedsRequest { suppress: true })?;
    assert_eq!(suppress_embeds["suppress"], true);

    Ok(())
}

#[test]
fn embed_dtos_match_server_json_shape() -> Result<(), Box<dyn std::error::Error>> {
    let message: Message = serde_json::from_str(
        r#"{
            "id":99,
            "user_id":42,
            "channel_id":10,
            "text":"https://example.test/article",
            "username":"alice",
            "display_name":null,
            "avatar_url":null,
            "suppress_embeds":false,
            "embeds":[{
                "id":201,
                "message_id":99,
                "url":"https://example.test/article",
                "title":"Article title",
                "description":"Article description",
                "image_url":"https://cdn.example.test/preview.jpg",
                "site_name":"Example",
                "embed_type":"link",
                "iframe_url":null,
                "iframe_width":null,
                "iframe_height":null
            }]
        }"#,
    )?;

    assert_eq!(message.embeds.len(), 1);
    assert_eq!(message.embeds[0].title.as_deref(), Some("Article title"));
    assert_eq!(
        message.embeds[0].image_url.as_deref(),
        Some("https://cdn.example.test/preview.jpg")
    );
    assert_eq!(message.embeds[0].embed_type, "link");

    let update = BroadcastEvent::MessageEmbedsUpdated(MessageEmbedsUpdatedEvent {
        id: message.id,
        channel_id: message.channel_id,
        suppress_embeds: true,
        embeds: message.embeds.clone(),
    });
    let value = serde_json::to_value(update)?;

    assert_eq!(value["kind"], "message_embeds_updated");
    assert_eq!(value["data"]["id"], 99);
    assert_eq!(value["data"]["suppress_embeds"], true);
    assert_eq!(
        value["data"]["embeds"][0]["image_url"],
        "https://cdn.example.test/preview.jpg"
    );

    let embed: Embed = serde_json::from_value(value["data"]["embeds"][0].clone())?;
    assert_eq!(embed.site_name.as_deref(), Some("Example"));

    Ok(())
}

#[test]
fn voice_token_dto_matches_server_json_shape() -> Result<(), Box<dyn std::error::Error>> {
    let token: VoiceToken =
        serde_json::from_str(r#"{"url":"ws://localhost:7880","token":"jwt","room":"channel-11"}"#)?;

    assert_eq!(token.url, "ws://localhost:7880");
    assert_eq!(token.token, "jwt");
    assert_eq!(token.room, "channel-11");

    Ok(())
}

#[test]
fn voice_participant_dtos_match_server_json_shape() -> Result<(), Box<dyn std::error::Error>> {
    let participant: VoiceParticipant = serde_json::from_str(
        r#"{"user_id":42,"channel_id":11,"username":"alice","avatar_url":"/avatars/a.png"}"#,
    )?;

    assert_eq!(participant.user_id, 42);
    assert_eq!(participant.channel_id, 11);
    assert_eq!(participant.username, "alice");
    assert_eq!(participant.avatar_url.as_deref(), Some("/avatars/a.png"));

    let joined = serde_json::to_value(BroadcastEvent::VoiceParticipantJoined(participant.clone()))?;
    assert_eq!(joined["kind"], "voice_participant_joined");
    assert_eq!(joined["data"]["user_id"], 42);
    assert_eq!(joined["data"]["channel_id"], 11);

    let left = serde_json::to_value(BroadcastEvent::VoiceParticipantLeft(
        VoiceParticipantLeftEvent {
            channel_id: 11,
            user_id: 42,
        },
    ))?;
    assert_eq!(left["kind"], "voice_participant_left");
    assert_eq!(left["data"]["user_id"], 42);
    assert_eq!(left["data"]["channel_id"], 11);

    let speaking_request = serde_json::to_value(VoiceSpeakingRequest {
        channel_id: 11,
        speaking: true,
    })?;
    assert_eq!(speaking_request["channel_id"], 11);
    assert_eq!(speaking_request["speaking"], true);

    let speaking = serde_json::to_value(BroadcastEvent::VoiceParticipantSpeakingChanged(
        VoiceParticipantSpeakingEvent {
            channel_id: 11,
            user_id: 42,
            speaking: true,
        },
    ))?;
    assert_eq!(speaking["kind"], "voice_participant_speaking_changed");
    assert_eq!(speaking["data"]["channel_id"], 11);
    assert_eq!(speaking["data"]["user_id"], 42);
    assert_eq!(speaking["data"]["speaking"], true);

    Ok(())
}

#[test]
fn error_envelope_matches_server_json_shape() -> Result<(), Box<dyn std::error::Error>> {
    let envelope: ErrorEnvelope = serde_json::from_str(
        r#"{"error":{"kind":"invalid_credentials","message":"invalid credentials"}}"#,
    )?;

    assert_eq!(
        envelope,
        ErrorEnvelope {
            error: ErrorDetails {
                kind: "invalid_credentials".to_string(),
                message: "invalid credentials".to_string(),
            }
        }
    );

    Ok(())
}
