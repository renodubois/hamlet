use hamlet_protocol::{
    AuthResponse, Channel, ChannelList, ChannelType, Event, History, Message, User,
};
use serde_json::json;

#[test]
fn auth_response_and_current_user_match_the_wire_contract() {
    let user = json!({"id": "user-1", "username": "Alice"});
    let wire = json!({
        "user": user,
        "access_token": "test-token",
        "expires_at": "2026-01-02T03:04:05Z"
    });
    let decoded: AuthResponse = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(decoded.user.id, "user-1");
    assert_eq!(decoded.user.username, "Alice");
    assert_eq!(decoded.access_token, "test-token");
    assert_eq!(decoded.expires_at.timestamp(), 1767323045);
    assert_eq!(serde_json::to_value(&decoded).unwrap(), wire);
    let current_user: User = serde_json::from_value(user.clone()).unwrap();
    assert_eq!(current_user, decoded.user);
    assert_eq!(serde_json::to_value(current_user).unwrap(), user);
}

#[test]
fn collection_envelopes_match_the_wire_contract() {
    let channels = json!({"items": [{"id": "c", "name": "general", "type": "text"}]});
    let decoded: ChannelList = serde_json::from_value(channels.clone()).unwrap();
    assert_eq!(decoded.items[0].id, "c");
    assert_eq!(serde_json::to_value(decoded).unwrap(), channels);

    for cursor in [json!(null), json!("opaque-cursor")] {
        let history = json!({
            "items": [{
                "id": "m", "channel_id": "c",
                "author": {"id": "u", "display_name": "Alice"},
                "text": "hello", "created_at": "2026-01-02T03:04:05Z"
            }],
            "next_cursor": cursor
        });
        let decoded: History = serde_json::from_value(history.clone()).unwrap();
        assert_eq!(decoded.items[0].id, "m");
        assert_eq!(serde_json::to_value(decoded).unwrap(), history);
    }
    assert!(serde_json::from_value::<ChannelList>(json!({})).is_err());
    assert!(serde_json::from_value::<History>(json!({"items": null})).is_err());
}

#[test]
fn malformed_auth_responses_are_rejected() {
    for wire in [
        json!({"user": {"id": "u"}, "access_token": "t", "expires_at": "2026-01-02T03:04:05Z"}),
        json!({"user": {"id": "u", "username": "Alice"}, "expires_at": "2026-01-02T03:04:05Z"}),
        json!({"user": {"id": "u", "username": "Alice"}, "access_token": "t", "expires_at": "invalid"}),
    ] {
        assert!(serde_json::from_value::<AuthResponse>(wire).is_err());
    }
}

#[test]
fn message_creation_matches_the_language_neutral_wire_example() {
    let message = json!({
        "id": "msg-1", "channel_id": "channel-1",
        "author": {"id": "user-1", "display_name": "Alice"},
        "text": "Hello 世界\nsecond line", "created_at": "2026-01-02T03:04:05.123456Z"
    });
    let wire = json!({"type": "message_created", "message": message});
    let event: Event = serde_json::from_value(wire.clone()).unwrap();
    let Event::MessageCreated { message: decoded } = &event else {
        panic!("expected message creation");
    };
    assert_eq!(decoded.id, "msg-1");
    assert_eq!(decoded.channel_id, "channel-1");
    assert_eq!(decoded.author.id, "user-1");
    assert_eq!(decoded.text, "Hello 世界\nsecond line");
    assert_eq!(decoded.created_at.timestamp_micros(), 1767323045123456);
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
    assert_eq!(serde_json::to_value(decoded).unwrap(), message);
    let _: Message = serde_json::from_value(message).unwrap();
}

#[test]
fn channel_creation_matches_the_language_neutral_wire_example() {
    let channel = json!({"id": "channel-1", "name": "general", "type": "text"});
    let wire = json!({"type": "channel_created", "channel": channel});
    let event: Event = serde_json::from_value(wire.clone()).unwrap();
    let Event::ChannelCreated { channel: decoded } = &event else {
        panic!("expected channel creation");
    };
    assert_eq!(decoded.id, "channel-1");
    assert_eq!(decoded.name, "general");
    assert!(matches!(decoded.kind, ChannelType::Text));
    assert_eq!(serde_json::to_value(&event).unwrap(), wire);
    assert_eq!(serde_json::to_value(decoded).unwrap(), channel);
    let _: Channel = serde_json::from_value(channel).unwrap();
}

#[test]
fn maximum_message_and_additional_fields_round_trip() {
    // Server accepts 4000 Unicode scalar values, not 4000 bytes.
    let text = "🦀\n".repeat(2000);
    let wire = json!({
        "type": "message_created", "future": true,
        "message": {
            "id": "18446744073709551615", "channel_id": "c",
            "author": {"id": "u", "display_name": "名字", "future": 1},
            "text": text, "created_at": "2026-01-02T04:04:05+01:00", "future": {}
        }
    });
    let event: Event = serde_json::from_value(wire).unwrap();
    let serialized = serde_json::to_string(&event).unwrap();
    assert!(!serialized.contains('\n'));
    let Event::MessageCreated { message } = serde_json::from_str(&serialized).unwrap() else {
        panic!("expected message creation");
    };
    assert_eq!(message.text, text);
    assert_eq!(message.id, "18446744073709551615");
    assert_eq!(message.created_at.timestamp(), 1767323045);

    let channel: Event = serde_json::from_value(json!({
        "type": "channel_created", "extra": 1,
        "channel": {"id": "c", "name": "general", "type": "text", "extra": []}
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(channel).unwrap(),
        json!({
            "type": "channel_created", "channel": {"id": "c", "name": "general", "type": "text"}
        })
    );
}

#[test]
fn unsupported_channel_types_and_malformed_timestamps_are_not_valid_wire_entities() {
    for kind in ["voice", "TEXT", ""] {
        assert!(
            serde_json::from_value::<Channel>(json!({
                "id": "c", "name": "general", "type": kind
            }))
            .is_err()
        );
    }
    assert!(
        serde_json::from_value::<Message>(json!({
            "id": "m", "channel_id": "c", "author": {"id": "u", "display_name": "Alice"},
            "text": "hello", "created_at": "not a timestamp"
        }))
        .is_err()
    );
}

#[test]
fn unknown_event_types_deserialize_but_cannot_be_serialized() {
    for wire in [
        json!({"type": "future"}),
        json!({"type": "unknown", "payload": {"nested": [1, null, true]}}),
        json!({"type": "future", "message": "not a known payload"}),
    ] {
        let event: Event = serde_json::from_value(wire).unwrap();
        assert!(matches!(event, Event::Unknown));
        assert!(serde_json::to_value(&event).is_err());
        assert!(serde_json::to_string(&event).is_err());
    }
}

#[test]
fn malformed_events_do_not_fall_back_to_unknown() {
    for wire in [
        r#"{"type":"message_created"}"#,
        r#"{"type":"channel_created","channel":null}"#,
        r#"{"type":"channel_created","channel":{"id":"c","id":"d","name":"general","type":"text"}}"#,
        r#"{"type":"channel_created","type":"future"}"#,
        r#"{"type":"future","type":"channel_created"}"#,
        r#"{"type":"future","type":"future"}"#,
        r#"{"type":null}"#,
        r#"{"type":1}"#,
        r#"{}"#,
        r#"[]"#,
        r#"null"#,
        r#"{"type":"future","payload":}"#,
    ] {
        assert!(serde_json::from_str::<Event>(wire).is_err(), "{wire}");
    }
}

#[cfg(feature = "openapi")]
#[test]
fn openapi_describes_both_creation_payloads() {
    use utoipa::OpenApi;
    #[derive(OpenApi)]
    #[openapi(components(schemas(Event)))]
    struct Contract;
    let document = serde_json::to_value(Contract::openapi()).unwrap();
    let schemas = &document["components"]["schemas"];
    assert_eq!(
        schemas["Message"]["properties"]["created_at"]["format"],
        "date-time"
    );
    assert_eq!(schemas["ChannelType"]["enum"], json!(["text"]));
    assert_eq!(schemas["Event"]["oneOf"].as_array().unwrap().len(), 2);
    let variants = schemas["Event"]["oneOf"].as_array().unwrap();
    assert_eq!(
        variants[0]["properties"]["type"]["enum"],
        json!(["message_created"])
    );
    assert_eq!(
        variants[1]["properties"]["type"]["enum"],
        json!(["channel_created"])
    );
}
