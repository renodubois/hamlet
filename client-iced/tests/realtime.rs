#![allow(clippy::expect_used)]

use hamlet_client_iced::protocol::{BroadcastEvent, MessageEmbedsUpdatedEvent};
use hamlet_client_iced::realtime::{
    RealtimeConnectionState, RealtimeError, RealtimeEvent, ReconnectPolicy, parse_sse_events,
};
use hamlet_client_iced::test_support::fixtures::{general_channel, message};

#[test]
fn parses_connected_frame_and_ignores_ping_comments() {
    let events = parse_sse_events(": ping\n\ndata: connected\n\n");

    assert_eq!(events, vec![Ok(RealtimeEvent::Connected)]);
}

#[test]
fn parses_broadcast_message_frame() {
    let channel = general_channel();
    let payload = serde_json::to_string(&BroadcastEvent::Message(message(1, channel.id, "hi")))
        .expect("serialize event");
    let events = parse_sse_events(&format!("data: {payload}\n\n"));

    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        Ok(RealtimeEvent::Broadcast(BroadcastEvent::Message(message))) if message.text == "hi"
    ));
}

#[test]
fn parses_channel_created_broadcast_frame() {
    let mut channel = general_channel();
    channel.id = 12;
    channel.name = "native-text".to_string();
    channel.position = 2;
    let payload = serde_json::to_string(&BroadcastEvent::ChannelCreated(channel.clone()))
        .expect("serialize event");
    let events = parse_sse_events(&format!("data: {payload}\n\n"));

    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        Ok(RealtimeEvent::Broadcast(BroadcastEvent::ChannelCreated(created))) if created == &channel
    ));
}

#[test]
fn parses_message_embeds_updated_broadcast_frame() {
    let channel = general_channel();
    let mut message = message(1, channel.id, "https://example.test");
    message.embeds.push(hamlet_client_iced::protocol::Embed {
        id: 77,
        message_id: message.id,
        url: "https://example.test".to_string(),
        title: Some("Example".to_string()),
        description: Some("Preview".to_string()),
        image_url: Some("https://cdn.example.test/preview.png".to_string()),
        site_name: Some("Example Site".to_string()),
        embed_type: "link".to_string(),
        iframe_url: None,
        iframe_width: None,
        iframe_height: None,
    });
    let update = MessageEmbedsUpdatedEvent {
        id: message.id,
        channel_id: channel.id,
        suppress_embeds: false,
        embeds: message.embeds.clone(),
    };
    let payload = serde_json::to_string(&BroadcastEvent::MessageEmbedsUpdated(update.clone()))
        .expect("serialize event");
    let events = parse_sse_events(&format!("data: {payload}\n\n"));

    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        Ok(RealtimeEvent::Broadcast(BroadcastEvent::MessageEmbedsUpdated(parsed)))
            if parsed == &update
    ));
}

#[test]
fn parses_channels_reordered_broadcast_frame() {
    let mut first = general_channel();
    first.position = 1;
    let mut second = general_channel();
    second.id = 12;
    second.name = "random".to_string();
    second.position = 0;
    let payload = serde_json::to_string(&BroadcastEvent::ChannelsReordered(vec![
        second.clone(),
        first.clone(),
    ]))
    .expect("serialize event");
    let events = parse_sse_events(&format!("data: {payload}\n\n"));

    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        Ok(RealtimeEvent::Broadcast(BroadcastEvent::ChannelsReordered(channels)))
            if channels == &vec![second, first]
    ));
}

#[test]
fn malformed_frame_returns_parse_error_without_panicking() {
    let events = parse_sse_events("data: not-json\n\n");

    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], Err(RealtimeError::Parse(_))));
}

#[test]
fn reconnect_policy_exponentially_backs_off_and_caps() {
    let policy = ReconnectPolicy::new(500, 2_000);

    assert_eq!(policy.delay_for_attempt(1), 500);
    assert_eq!(policy.delay_for_attempt(2), 1_000);
    assert_eq!(policy.delay_for_attempt(3), 2_000);
    assert_eq!(policy.delay_for_attempt(10), 2_000);
    assert_eq!(
        RealtimeConnectionState::BackingOff {
            attempt: 3,
            delay_ms: policy.delay_for_attempt(3),
        },
        RealtimeConnectionState::BackingOff {
            attempt: 3,
            delay_ms: 2_000,
        }
    );
}
