#![allow(clippy::expect_used)]

use hamlet_client_iced::storage::VoiceDevicePreferences;
use hamlet_client_iced::voice::{
    ConnectedVoiceRoom, VoiceBackend, VoiceCommand, VoiceCommandWorker, VoiceError, VoiceEvent,
    VoiceJoinRequest,
};

#[test]
fn worker_join_command_connects_and_emits_connected_event() {
    let backend = ScriptedBackend::default();
    let mut worker = VoiceCommandWorker::new(backend);

    let events = worker.handle_command(VoiceCommand::Join(join_request(11)));

    assert_eq!(
        events,
        vec![
            VoiceEvent::Connecting { channel_id: 11 },
            VoiceEvent::Connected {
                channel_id: 11,
                room: "channel-11".to_string(),
            },
        ]
    );
    assert_eq!(worker.connected().map(|room| room.channel_id), Some(11));
}

#[test]
fn worker_leave_command_disconnects_current_room() {
    let backend = ScriptedBackend::default();
    let mut worker = VoiceCommandWorker::new(backend);
    worker.handle_command(VoiceCommand::Join(join_request(11)));

    let events = worker.handle_command(VoiceCommand::Leave);

    assert_eq!(
        events,
        vec![VoiceEvent::Disconnected {
            channel_id: Some(11),
            reason: None,
        }]
    );
    assert!(worker.connected().is_none());
}

#[test]
fn worker_join_while_connected_switches_rooms_without_stale_state() {
    let backend = ScriptedBackend::default();
    let mut worker = VoiceCommandWorker::new(backend);
    worker.handle_command(VoiceCommand::Join(join_request(11)));

    let events = worker.handle_command(VoiceCommand::Join(join_request(12)));

    assert_eq!(
        events,
        vec![
            VoiceEvent::Disconnected {
                channel_id: Some(11),
                reason: None,
            },
            VoiceEvent::Connecting { channel_id: 12 },
            VoiceEvent::Connected {
                channel_id: 12,
                room: "channel-12".to_string(),
            },
        ]
    );
    assert_eq!(worker.connected().map(|room| room.channel_id), Some(12));
}

#[test]
fn worker_error_event_clears_connected_state() {
    let backend =
        ScriptedBackend::with_next_connect_error(VoiceError::livekit(11, "LiveKit unavailable"));
    let mut worker = VoiceCommandWorker::new(backend);

    let events = worker.handle_command(VoiceCommand::Join(join_request(11)));

    assert!(
        matches!(events.as_slice(), [VoiceEvent::Connecting { channel_id: 11 }, VoiceEvent::Error(error)] if error.message.contains("LiveKit unavailable"))
    );
    assert!(worker.connected().is_none());
}

#[test]
fn worker_mute_unmute_commands_emit_state_events() {
    let backend = ScriptedBackend::default();
    let mut worker = VoiceCommandWorker::new(backend);
    worker.handle_command(VoiceCommand::Join(join_request(11)));

    assert_eq!(
        worker.handle_command(VoiceCommand::Mute),
        vec![VoiceEvent::Muted { channel_id: 11 }]
    );
    assert_eq!(
        worker.handle_command(VoiceCommand::Unmute),
        vec![VoiceEvent::Unmuted { channel_id: 11 }]
    );
}

#[test]
fn worker_deafen_undeafen_commands_emit_state_events() {
    let backend = ScriptedBackend::default();
    let mut worker = VoiceCommandWorker::new(backend);
    worker.handle_command(VoiceCommand::Join(join_request(11)));

    assert_eq!(
        worker.handle_command(VoiceCommand::Deafen),
        vec![VoiceEvent::Deafened { channel_id: 11 }]
    );
    assert_eq!(
        worker.handle_command(VoiceCommand::Undeafen),
        vec![VoiceEvent::Undeafened { channel_id: 11 }]
    );
}

#[test]
fn worker_speaking_events_clear_on_disconnect() {
    let backend = ScriptedBackend::default();
    let mut worker = VoiceCommandWorker::new(backend);
    worker.handle_command(VoiceCommand::Join(join_request(11)));
    worker.handle_backend_event(VoiceEvent::SpeakingChanged {
        channel_id: 11,
        user_id: 1,
        speaking: true,
    });

    let events = worker.handle_command(VoiceCommand::Leave);

    assert_eq!(
        events,
        vec![
            VoiceEvent::SpeakingChanged {
                channel_id: 11,
                user_id: 1,
                speaking: false,
            },
            VoiceEvent::Disconnected {
                channel_id: Some(11),
                reason: None,
            },
        ]
    );
}

#[test]
fn worker_microphone_permission_error_is_recoverable() {
    let backend = ScriptedBackend::with_next_connect_error(VoiceError::microphone_permission(
        11,
        "permission denied",
    ));
    let mut worker = VoiceCommandWorker::new(backend);

    let events = worker.handle_command(VoiceCommand::Join(join_request(11)));

    assert!(matches!(
        events.as_slice(),
        [VoiceEvent::Connecting { channel_id: 11 }, VoiceEvent::Error(error)]
            if error.recoverable && error.user_message().contains("Allow microphone access")
    ));
    assert!(worker.connected().is_none());
}

#[test]
fn worker_cleanup_disconnects_active_room() {
    let backend = ScriptedBackend::default();
    let mut worker = VoiceCommandWorker::new(backend);
    worker.handle_command(VoiceCommand::Join(join_request(11)));

    let events = worker.handle_command(VoiceCommand::Shutdown);

    assert_eq!(
        events,
        vec![VoiceEvent::Disconnected {
            channel_id: Some(11),
            reason: None,
        }]
    );
    assert!(worker.connected().is_none());
}

#[derive(Debug, Default)]
struct ScriptedBackend {
    connected: Option<ConnectedVoiceRoom>,
    next_connect_error: Option<VoiceError>,
}

impl ScriptedBackend {
    fn with_next_connect_error(error: VoiceError) -> Self {
        Self {
            connected: None,
            next_connect_error: Some(error),
        }
    }
}

impl VoiceBackend for ScriptedBackend {
    fn connect(&mut self, request: &VoiceJoinRequest) -> Result<ConnectedVoiceRoom, VoiceError> {
        if let Some(error) = self.next_connect_error.take() {
            return Err(error);
        }

        let connected = ConnectedVoiceRoom {
            channel_id: request.channel_id,
            room: request.room.clone(),
        };
        self.connected = Some(connected.clone());

        Ok(connected)
    }

    fn disconnect(&mut self) -> Result<Option<ConnectedVoiceRoom>, VoiceError> {
        Ok(self.connected.take())
    }
}

fn join_request(channel_id: i64) -> VoiceJoinRequest {
    VoiceJoinRequest {
        channel_id,
        url: "ws://localhost:7880".to_string(),
        token: "token".to_string(),
        room: format!("channel-{channel_id}"),
        device_preferences: VoiceDevicePreferences::default(),
    }
}
