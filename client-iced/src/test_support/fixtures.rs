use crate::auth::AuthSession;
use crate::protocol::{
    BroadcastEvent, Channel, ChannelKind, Id, Message, User, VoiceParticipant,
    VoiceParticipantSpeakingEvent, VoiceToken,
};
use crate::storage::{DEFAULT_SERVER_URL, Preferences};
use crate::voice::{VoiceCommand, VoiceEvent, VoiceJoinRequest};

pub const DEV_SESSION_TOKEN: &str = "fake-session-token";

pub fn preferences() -> Preferences {
    Preferences::default()
}

pub fn preferences_with_server_url(server_url: &str) -> Preferences {
    Preferences::with_server_url(server_url).unwrap_or_else(|_| Preferences::default())
}

pub fn dev_user() -> User {
    User {
        id: 1,
        username: "baipas".to_string(),
        display_name: Some("Baipas".to_string()),
        email: None,
        email_verified: false,
        avatar_url: None,
    }
}

pub fn dev_auth_session() -> AuthSession {
    AuthSession::new(dev_user(), Some(DEV_SESSION_TOKEN.to_string()))
}

pub fn general_channel() -> Channel {
    Channel {
        id: 10,
        name: "general".to_string(),
        position: 0,
        kind: ChannelKind::Text,
    }
}

pub fn voice_channel() -> Channel {
    Channel {
        id: 11,
        name: "voice".to_string(),
        position: 1,
        kind: ChannelKind::Voice,
    }
}

pub fn dev_channels() -> Vec<Channel> {
    vec![general_channel(), voice_channel()]
}

pub fn message(id: Id, channel_id: Id, text: &str) -> Message {
    let user = dev_user();

    Message {
        id,
        user_id: user.id,
        channel_id,
        text: text.to_string(),
        username: user.username,
        display_name: user.display_name,
        avatar_url: user.avatar_url,
        suppress_embeds: false,
        embeds: Vec::new(),
    }
}

pub fn general_messages() -> Vec<Message> {
    let channel = general_channel();

    vec![
        message(100, channel.id, "hello"),
        message(101, channel.id, "native alpha fixture"),
    ]
}

pub fn voice_participant(user_id: Id, channel_id: Id, username: &str) -> VoiceParticipant {
    VoiceParticipant {
        user_id,
        channel_id,
        username: username.to_string(),
        avatar_url: None,
    }
}

pub fn voice_presence() -> Vec<VoiceParticipant> {
    let channel = voice_channel();

    vec![
        voice_participant(1, channel.id, "baipas"),
        voice_participant(2, channel.id, "teo"),
    ]
}

pub fn voice_token() -> VoiceToken {
    let channel = voice_channel();

    VoiceToken {
        url: "ws://localhost:7880".to_string(),
        token: "fake-livekit-token".to_string(),
        room: format!("channel-{}", channel.id),
    }
}

pub fn voice_join_request() -> VoiceJoinRequest {
    VoiceJoinRequest::from_token(voice_channel().id, voice_token())
}

pub fn voice_worker_commands() -> Vec<VoiceCommand> {
    vec![
        VoiceCommand::Join(voice_join_request()),
        VoiceCommand::Mute,
        VoiceCommand::Unmute,
        VoiceCommand::Deafen,
        VoiceCommand::Undeafen,
        VoiceCommand::Leave,
    ]
}

pub fn voice_worker_events() -> Vec<VoiceEvent> {
    let channel = voice_channel();
    let room = format!("channel-{}", channel.id);

    vec![
        VoiceEvent::Connecting {
            channel_id: channel.id,
        },
        VoiceEvent::Connected {
            channel_id: channel.id,
            room,
        },
        VoiceEvent::Muted {
            channel_id: channel.id,
        },
        VoiceEvent::Unmuted {
            channel_id: channel.id,
        },
        VoiceEvent::Deafened {
            channel_id: channel.id,
        },
        VoiceEvent::Undeafened {
            channel_id: channel.id,
        },
        VoiceEvent::Disconnected {
            channel_id: Some(channel.id),
            reason: None,
        },
    ]
}

pub fn sse_connected_frame() -> &'static str {
    ": ping\n\ndata: connected\n\n"
}

pub fn sse_message_frame() -> Result<String, serde_json::Error> {
    let channel = general_channel();

    sse_frame(BroadcastEvent::Message(message(
        102,
        channel.id,
        "sse hello",
    )))
}

pub fn sse_voice_presence_frames() -> Result<String, serde_json::Error> {
    let channel = voice_channel();
    let joined = BroadcastEvent::VoiceParticipantJoined(voice_participant(2, channel.id, "teo"));
    let speaking = BroadcastEvent::VoiceParticipantSpeakingChanged(VoiceParticipantSpeakingEvent {
        channel_id: channel.id,
        user_id: 2,
        speaking: true,
    });

    Ok(format!("{}{}", sse_frame(joined)?, sse_frame(speaking)?))
}

pub fn default_server_url() -> &'static str {
    DEFAULT_SERVER_URL
}

fn sse_frame(event: BroadcastEvent) -> Result<String, serde_json::Error> {
    Ok(format!("data: {}\n\n", serde_json::to_string(&event)?))
}
