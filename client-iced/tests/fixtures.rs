use hamlet_client_iced::api::{ApiCall, ApiClient};
use hamlet_client_iced::protocol::BroadcastEvent;
use hamlet_client_iced::realtime::{RealtimeClient, RealtimeEvent, parse_sse_events};
use hamlet_client_iced::test_support::fake_api::FakeApi;
use hamlet_client_iced::test_support::fake_realtime::FakeRealtime;
use hamlet_client_iced::test_support::fixtures::{
    DEV_SESSION_TOKEN, dev_auth_session, dev_channels, general_channel, general_messages, message,
    sse_connected_frame, sse_message_frame, sse_voice_presence_frames, voice_channel,
    voice_presence, voice_token, voice_worker_commands, voice_worker_events,
};
use hamlet_client_iced::voice::{VoiceCommand, VoiceEvent};

#[test]
fn fake_api_records_base_url_changes() -> Result<(), Box<dyn std::error::Error>> {
    let api = FakeApi::default();

    api.set_base_url("http://127.0.0.1:3030".to_string())?;

    assert_eq!(api.base_url(), "http://127.0.0.1:3030");
    assert_eq!(
        api.calls()?,
        vec![ApiCall::SetBaseUrl("http://127.0.0.1:3030".to_string())]
    );

    Ok(())
}

#[test]
fn fake_realtime_drains_events_in_order() -> Result<(), Box<dyn std::error::Error>> {
    let realtime = FakeRealtime::default();
    let channel = general_channel();

    realtime.push(RealtimeEvent::Connected)?;
    realtime.push(RealtimeEvent::Broadcast(
        hamlet_client_iced::protocol::BroadcastEvent::Message(message(99, channel.id, "hello")),
    ))?;

    let events = realtime.drain_events()?;

    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], RealtimeEvent::Connected));
    assert!(realtime.drain_events()?.is_empty());

    Ok(())
}

#[test]
fn fake_api_retains_authenticated_user_after_login() -> Result<(), Box<dyn std::error::Error>> {
    let api = FakeApi::default();

    let session = api.login("baipas".to_string(), "password".to_string())?;
    let me = api.get_me()?;

    assert_eq!(me, session.user);
    assert_eq!(
        api.calls()?,
        vec![
            ApiCall::Login {
                username: "baipas".to_string()
            },
            ApiCall::GetMe,
        ]
    );

    Ok(())
}

#[test]
fn deterministic_native_alpha_fixtures_cover_core_contracts()
-> Result<(), Box<dyn std::error::Error>> {
    let auth = dev_auth_session();
    assert_eq!(auth.user.username, "baipas");
    assert_eq!(auth.session_token.as_deref(), Some(DEV_SESSION_TOKEN));

    assert_eq!(dev_channels(), vec![general_channel(), voice_channel()]);
    let messages = general_messages();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].text, "hello");
    assert_eq!(messages[1].text, "native alpha fixture");

    let connected_events = parse_sse_events(sse_connected_frame());
    assert_eq!(connected_events, vec![Ok(RealtimeEvent::Connected)]);

    let message_events = parse_sse_events(&sse_message_frame()?);
    assert!(matches!(
        message_events.as_slice(),
        [Ok(RealtimeEvent::Broadcast(BroadcastEvent::Message(message)))]
            if message.id == 102 && message.text == "sse hello"
    ));

    let voice_events = parse_sse_events(&sse_voice_presence_frames()?);
    assert_eq!(voice_events.len(), 2);
    assert!(matches!(
        &voice_events[0],
        Ok(RealtimeEvent::Broadcast(BroadcastEvent::VoiceParticipantJoined(participant)))
            if participant.username == "teo" && participant.channel_id == voice_channel().id
    ));
    assert!(matches!(
        &voice_events[1],
        Ok(RealtimeEvent::Broadcast(
            BroadcastEvent::VoiceParticipantSpeakingChanged(speaking)
        )) if speaking.user_id == 2 && speaking.speaking
    ));

    let presence = voice_presence();
    assert_eq!(presence.len(), 2);
    assert_eq!(presence[0].username, "baipas");
    assert_eq!(presence[1].username, "teo");

    let token = voice_token();
    assert_eq!(token.url, "ws://localhost:7880");
    assert_eq!(token.room, "channel-11");

    assert!(matches!(
        voice_worker_commands().as_slice(),
        [
            VoiceCommand::Join(_),
            VoiceCommand::Mute,
            VoiceCommand::Unmute,
            VoiceCommand::Deafen,
            VoiceCommand::Undeafen,
            VoiceCommand::Leave,
        ]
    ));
    assert_eq!(
        voice_worker_events(),
        vec![
            VoiceEvent::Connecting { channel_id: 11 },
            VoiceEvent::Connected {
                channel_id: 11,
                room: "channel-11".to_string(),
            },
            VoiceEvent::Muted { channel_id: 11 },
            VoiceEvent::Unmuted { channel_id: 11 },
            VoiceEvent::Deafened { channel_id: 11 },
            VoiceEvent::Undeafened { channel_id: 11 },
            VoiceEvent::Disconnected {
                channel_id: Some(11),
                reason: None,
            },
        ]
    );

    Ok(())
}
