use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, OnceLock};

use iced::futures::channel::mpsc::{self, UnboundedSender};
use iced::futures::stream::BoxStream;
use iced::futures::{SinkExt, StreamExt};
use thiserror::Error;

use crate::auth::AuthenticatedRequest;
use crate::protocol::{Id, VoiceToken};
use crate::storage::VoiceDevicePreferences;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceJoinRequest {
    pub channel_id: Id,
    pub url: String,
    pub token: String,
    pub room: String,
    pub device_preferences: VoiceDevicePreferences,
}

impl VoiceJoinRequest {
    pub fn from_token(channel_id: Id, token: VoiceToken) -> Self {
        Self::from_token_with_device_preferences(
            channel_id,
            token,
            VoiceDevicePreferences::default(),
        )
    }

    pub fn from_token_with_device_preferences(
        channel_id: Id,
        token: VoiceToken,
        device_preferences: VoiceDevicePreferences,
    ) -> Self {
        Self {
            channel_id,
            url: token.url,
            token: token.token,
            room: token.room,
            device_preferences,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceCommand {
    Join(VoiceJoinRequest),
    Leave,
    Mute,
    Unmute,
    Deafen,
    Undeafen,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceEvent {
    CommandAccepted,
    Connecting {
        channel_id: Id,
    },
    Connected {
        channel_id: Id,
        room: String,
    },
    Muted {
        channel_id: Id,
    },
    Unmuted {
        channel_id: Id,
    },
    Deafened {
        channel_id: Id,
    },
    Undeafened {
        channel_id: Id,
    },
    SpeakingChanged {
        channel_id: Id,
        user_id: Id,
        speaking: bool,
    },
    Disconnected {
        channel_id: Option<Id>,
        reason: Option<String>,
    },
    Reconnecting {
        channel_id: Id,
    },
    Reconnected {
        channel_id: Id,
    },
    Error(VoiceError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceErrorKind {
    Worker,
    LiveKit,
    Audio,
    Permission,
}

impl VoiceErrorKind {
    fn label(self) -> &'static str {
        match self {
            Self::Worker => "voice worker",
            Self::LiveKit => "LiveKit",
            Self::Audio => "audio",
            Self::Permission => "permission",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{kind_label}: {message}")]
pub struct VoiceError {
    pub kind: VoiceErrorKind,
    pub channel_id: Option<Id>,
    pub message: String,
    pub recoverable: bool,
    #[doc(hidden)]
    pub kind_label: &'static str,
}

impl VoiceError {
    pub fn worker(message: impl Into<String>) -> Self {
        Self::new(VoiceErrorKind::Worker, None, message, true)
    }

    pub fn livekit(channel_id: Id, message: impl Into<String>) -> Self {
        Self::new(VoiceErrorKind::LiveKit, Some(channel_id), message, true)
    }

    pub fn audio(channel_id: Id, message: impl Into<String>) -> Self {
        Self::new(VoiceErrorKind::Audio, Some(channel_id), message, true)
    }

    pub fn microphone_permission(channel_id: Id, details: impl Into<String>) -> Self {
        let details = details.into();
        let suffix = if details.trim().is_empty() {
            String::new()
        } else {
            format!(" Details: {details}")
        };

        Self::new(
            VoiceErrorKind::Permission,
            Some(channel_id),
            format!(
                "Hamlet could not access your microphone. Allow microphone access in your operating system settings, then retry joining voice.{suffix}"
            ),
            true,
        )
    }

    pub fn new(
        kind: VoiceErrorKind,
        channel_id: Option<Id>,
        message: impl Into<String>,
        recoverable: bool,
    ) -> Self {
        Self {
            kind,
            channel_id,
            message: message.into(),
            recoverable,
            kind_label: kind.label(),
        }
    }

    pub fn user_message(&self) -> String {
        let retry = if self.recoverable {
            " You can retry joining the voice channel."
        } else {
            ""
        };

        if self.kind == VoiceErrorKind::Permission {
            return format!("{}{retry}", self.message);
        }

        format!("{} error: {}{retry}", self.kind.label(), self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectedVoiceRoom {
    pub channel_id: Id,
    pub room: String,
}

pub trait VoiceBackend {
    fn connect(&mut self, request: &VoiceJoinRequest) -> Result<ConnectedVoiceRoom, VoiceError>;
    fn disconnect(&mut self) -> Result<Option<ConnectedVoiceRoom>, VoiceError>;

    fn set_muted(&mut self, _muted: bool) -> Result<(), VoiceError> {
        Ok(())
    }

    fn set_deafened(&mut self, _deafened: bool) -> Result<(), VoiceError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct VoiceCommandWorker<B: VoiceBackend> {
    backend: B,
    connected: Option<ConnectedVoiceRoom>,
    speaking_user_ids: BTreeSet<Id>,
}

impl<B> VoiceCommandWorker<B>
where
    B: VoiceBackend,
{
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            connected: None,
            speaking_user_ids: BTreeSet::new(),
        }
    }

    pub fn connected(&self) -> Option<&ConnectedVoiceRoom> {
        self.connected.as_ref()
    }

    pub fn handle_command(&mut self, command: VoiceCommand) -> Vec<VoiceEvent> {
        match command {
            VoiceCommand::Join(request) => self.join(request),
            VoiceCommand::Leave => self.leave(),
            VoiceCommand::Mute => self.set_muted(true),
            VoiceCommand::Unmute => self.set_muted(false),
            VoiceCommand::Deafen => self.set_deafened(true),
            VoiceCommand::Undeafen => self.set_deafened(false),
            VoiceCommand::Shutdown => self.cleanup(),
        }
    }

    pub fn handle_backend_event(&mut self, event: VoiceEvent) -> Vec<VoiceEvent> {
        match &event {
            VoiceEvent::SpeakingChanged {
                user_id, speaking, ..
            } => {
                if *speaking {
                    self.speaking_user_ids.insert(*user_id);
                } else {
                    self.speaking_user_ids.remove(user_id);
                }
                vec![event]
            }
            VoiceEvent::Disconnected { channel_id, .. } => {
                if channel_id.is_none() || self.connected_channel_id() == *channel_id {
                    let mut events = self.clear_speaking_events(*channel_id);
                    self.connected = None;
                    events.push(event);
                    events
                } else {
                    vec![event]
                }
            }
            VoiceEvent::Error(error) => {
                if error.channel_id.is_none() || self.connected_channel_id() == error.channel_id {
                    let mut events = self.clear_speaking_events(error.channel_id);
                    self.connected = None;
                    events.push(event);
                    events
                } else {
                    vec![event]
                }
            }
            VoiceEvent::CommandAccepted
            | VoiceEvent::Connecting { .. }
            | VoiceEvent::Connected { .. }
            | VoiceEvent::Muted { .. }
            | VoiceEvent::Unmuted { .. }
            | VoiceEvent::Deafened { .. }
            | VoiceEvent::Undeafened { .. }
            | VoiceEvent::Reconnecting { .. }
            | VoiceEvent::Reconnected { .. } => vec![event],
        }
    }

    pub fn cleanup(&mut self) -> Vec<VoiceEvent> {
        self.leave()
    }

    fn join(&mut self, request: VoiceJoinRequest) -> Vec<VoiceEvent> {
        let mut events = self.leave();
        events.push(VoiceEvent::Connecting {
            channel_id: request.channel_id,
        });

        match self.backend.connect(&request) {
            Ok(connected) => {
                self.connected = Some(connected.clone());
                events.push(VoiceEvent::Connected {
                    channel_id: connected.channel_id,
                    room: connected.room,
                });
            }
            Err(error) => {
                self.connected = None;
                events.push(VoiceEvent::Error(error));
            }
        }

        events
    }

    fn leave(&mut self) -> Vec<VoiceEvent> {
        let previous = self.connected.take();
        let Some(previous) = previous else {
            return Vec::new();
        };

        let mut events = self.clear_speaking_events(Some(previous.channel_id));

        match self.backend.disconnect() {
            Ok(disconnected) => {
                let disconnected = disconnected.unwrap_or(previous);
                events.push(VoiceEvent::Disconnected {
                    channel_id: Some(disconnected.channel_id),
                    reason: None,
                });
                events
            }
            Err(error) => {
                events.push(VoiceEvent::Error(error));
                events
            }
        }
    }

    fn set_muted(&mut self, muted: bool) -> Vec<VoiceEvent> {
        let Some(channel_id) = self.connected_channel_id() else {
            return vec![VoiceEvent::Error(VoiceError::worker(
                "Join a voice channel before changing microphone mute.",
            ))];
        };

        match self.backend.set_muted(muted) {
            Ok(()) if muted => vec![VoiceEvent::Muted { channel_id }],
            Ok(()) => vec![VoiceEvent::Unmuted { channel_id }],
            Err(error) => vec![VoiceEvent::Error(error)],
        }
    }

    fn set_deafened(&mut self, deafened: bool) -> Vec<VoiceEvent> {
        let Some(channel_id) = self.connected_channel_id() else {
            return vec![VoiceEvent::Error(VoiceError::worker(
                "Join a voice channel before changing deafen.",
            ))];
        };

        match self.backend.set_deafened(deafened) {
            Ok(()) if deafened => vec![VoiceEvent::Deafened { channel_id }],
            Ok(()) => vec![VoiceEvent::Undeafened { channel_id }],
            Err(error) => vec![VoiceEvent::Error(error)],
        }
    }

    fn clear_speaking_events(&mut self, channel_id: Option<Id>) -> Vec<VoiceEvent> {
        let Some(channel_id) = channel_id.or_else(|| self.connected_channel_id()) else {
            self.speaking_user_ids.clear();
            return Vec::new();
        };

        let events = self
            .speaking_user_ids
            .iter()
            .copied()
            .map(|user_id| VoiceEvent::SpeakingChanged {
                channel_id,
                user_id,
                speaking: false,
            })
            .collect();
        self.speaking_user_ids.clear();
        events
    }

    fn connected_channel_id(&self) -> Option<Id> {
        self.connected
            .as_ref()
            .map(|connected| connected.channel_id)
    }
}

impl<B> Drop for VoiceCommandWorker<B>
where
    B: VoiceBackend,
{
    fn drop(&mut self) {
        let _events = self.cleanup();
    }
}

#[derive(Debug, Clone)]
enum WorkerInput {
    Command(VoiceCommand),
    BackendEvent(VoiceEvent),
}

pub fn worker_stream(_request: &AuthenticatedRequest) -> BoxStream<'static, VoiceEvent> {
    iced::stream::channel(100, async move |mut output| {
        let (input_tx, mut input_rx) = mpsc::unbounded::<WorkerInput>();
        let _guard = install_runtime_sender(input_tx.clone());
        let backend = LiveKitVoiceBackend::new(input_tx);
        let mut worker = VoiceCommandWorker::new(backend);

        while let Some(input) = input_rx.next().await {
            let events = match input {
                WorkerInput::Command(command) => worker.handle_command(command),
                WorkerInput::BackendEvent(event) => worker.handle_backend_event(event),
            };

            for event in events {
                if output.send(event).await.is_err() {
                    return;
                }
            }
        }
    })
    .boxed()
}

pub async fn send_runtime_command(command: VoiceCommand) -> VoiceEvent {
    let Some(sender) = runtime_sender() else {
        return VoiceEvent::Error(VoiceError::worker(
            "Voice worker is not running yet. Try again in a moment.",
        ));
    };

    match sender.unbounded_send(WorkerInput::Command(command)) {
        Ok(()) => VoiceEvent::CommandAccepted,
        Err(_) => VoiceEvent::Error(VoiceError::worker(
            "Voice worker stopped before it could receive the command.",
        )),
    }
}

fn sender_slot() -> &'static Mutex<Option<UnboundedSender<WorkerInput>>> {
    static SENDER: OnceLock<Mutex<Option<UnboundedSender<WorkerInput>>>> = OnceLock::new();

    SENDER.get_or_init(|| Mutex::new(None))
}

fn runtime_sender() -> Option<UnboundedSender<WorkerInput>> {
    sender_slot()
        .lock()
        .ok()
        .and_then(|sender| sender.as_ref().cloned())
}

fn install_runtime_sender(sender: UnboundedSender<WorkerInput>) -> RuntimeSenderGuard {
    if let Ok(mut slot) = sender_slot().lock() {
        *slot = Some(sender);
    }

    RuntimeSenderGuard
}

#[derive(Debug)]
struct RuntimeSenderGuard;

impl Drop for RuntimeSenderGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = sender_slot().lock() {
            *slot = None;
        }
    }
}

struct LiveKitVoiceBackend {
    runtime: Result<tokio::runtime::Runtime, VoiceError>,
    input_tx: UnboundedSender<WorkerInput>,
    session: Option<LiveKitSession>,
}

struct LiveKitSession {
    channel_id: Id,
    room: livekit::prelude::Room,
    audio: livekit::prelude::PlatformAudio,
    track: livekit::prelude::LocalAudioTrack,
    _publication: livekit::prelude::LocalTrackPublication,
    remote_audio_tracks: Arc<Mutex<Vec<livekit::prelude::RemoteAudioTrack>>>,
    deafened: Arc<Mutex<bool>>,
    event_task: tokio::task::JoinHandle<()>,
}

impl LiveKitVoiceBackend {
    fn new(input_tx: UnboundedSender<WorkerInput>) -> Self {
        Self {
            runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("hamlet-livekit-voice")
                .build()
                .map_err(|error| {
                    VoiceError::worker(format!("Could not start LiveKit runtime: {error}"))
                }),
            input_tx,
            session: None,
        }
    }

    fn runtime(&self) -> Result<&tokio::runtime::Runtime, VoiceError> {
        self.runtime.as_ref().map_err(Clone::clone)
    }

    fn close_session(&mut self) -> Result<Option<ConnectedVoiceRoom>, VoiceError> {
        let Some(session) = self.session.take() else {
            return Ok(None);
        };
        let LiveKitSession {
            channel_id,
            room,
            audio: _audio,
            track: _track,
            _publication,
            remote_audio_tracks: _remote_audio_tracks,
            deafened: _deafened,
            event_task,
        } = session;
        let connected = ConnectedVoiceRoom {
            channel_id,
            room: room.name(),
        };
        event_task.abort();

        self.runtime()?
            .block_on(async move { room.close().await })
            .map_err(|error| {
                VoiceError::livekit(
                    connected.channel_id,
                    format!("Could not disconnect from LiveKit room: {error}"),
                )
            })?;

        Ok(Some(connected))
    }
}

fn microphone_access_error(channel_id: Id, error: impl std::fmt::Display) -> VoiceError {
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();

    if lower.contains("permission")
        || lower.contains("denied")
        || lower.contains("not authorized")
        || lower.contains("unauthorized")
    {
        VoiceError::microphone_permission(channel_id, message)
    } else {
        VoiceError::audio(
            channel_id,
            format!("Could not publish microphone to LiveKit: {message}"),
        )
    }
}

fn apply_device_preferences(
    channel_id: Id,
    audio: &livekit::prelude::PlatformAudio,
    preferences: &VoiceDevicePreferences,
) -> Result<(), VoiceError> {
    if let Some(device_id) = preferences.microphone_device_id.as_deref() {
        let device_id = livekit::prelude::RecordingDeviceId::from_unchecked_guid(device_id);
        audio.set_recording_device(&device_id).map_err(|error| match error {
            livekit::prelude::AudioError::DeviceNotFound => VoiceError::audio(
                channel_id,
                "Saved microphone device is not available. Update or clear voice preferences, then retry joining voice.",
            ),
            other => VoiceError::audio(
                channel_id,
                format!("Could not select the saved microphone device: {other}"),
            ),
        })?;
    }

    if let Some(device_id) = preferences.output_device_id.as_deref() {
        let device_id = livekit::prelude::PlayoutDeviceId::from_unchecked_guid(device_id);
        audio.set_playout_device(&device_id).map_err(|error| match error {
            livekit::prelude::AudioError::DeviceNotFound => VoiceError::audio(
                channel_id,
                "Saved output device is not available. Update or clear voice preferences, then retry joining voice.",
            ),
            other => VoiceError::audio(
                channel_id,
                format!("Could not select the saved output device: {other}"),
            ),
        })?;
    }

    Ok(())
}

fn voice_events_from_room_event(
    channel_id: Id,
    event: livekit::prelude::RoomEvent,
    remote_audio_tracks: &Arc<Mutex<Vec<livekit::prelude::RemoteAudioTrack>>>,
    deafened: &Arc<Mutex<bool>>,
    active_speakers: &mut BTreeSet<Id>,
) -> Vec<VoiceEvent> {
    match event {
        livekit::prelude::RoomEvent::Disconnected { reason } => vec![VoiceEvent::Disconnected {
            channel_id: Some(channel_id),
            reason: Some(format!("LiveKit disconnected: {reason:?}")),
        }],
        livekit::prelude::RoomEvent::Reconnecting => vec![VoiceEvent::Reconnecting { channel_id }],
        livekit::prelude::RoomEvent::Reconnected => vec![VoiceEvent::Reconnected { channel_id }],
        livekit::prelude::RoomEvent::TrackSubscribed { track, .. } => {
            if let livekit::prelude::RemoteTrack::Audio(track) = track {
                if deafened.lock().is_ok_and(|deafened| *deafened) {
                    track.disable();
                }
                if let Ok(mut tracks) = remote_audio_tracks.lock() {
                    tracks.push(track);
                }
            }
            Vec::new()
        }
        livekit::prelude::RoomEvent::TrackUnsubscribed { track, .. } => {
            if let livekit::prelude::RemoteTrack::Audio(track) = track
                && let Ok(mut tracks) = remote_audio_tracks.lock()
            {
                let sid = track.sid();
                tracks.retain(|existing| existing.sid() != sid);
            }
            Vec::new()
        }
        livekit::prelude::RoomEvent::ActiveSpeakersChanged { speakers } => {
            let next = speakers
                .into_iter()
                .filter_map(|participant| participant_id(&participant))
                .collect::<BTreeSet<_>>();
            let mut events = Vec::new();

            for user_id in active_speakers.difference(&next).copied() {
                events.push(VoiceEvent::SpeakingChanged {
                    channel_id,
                    user_id,
                    speaking: false,
                });
            }
            for user_id in next.difference(active_speakers).copied() {
                events.push(VoiceEvent::SpeakingChanged {
                    channel_id,
                    user_id,
                    speaking: true,
                });
            }

            *active_speakers = next;
            events
        }
        livekit::prelude::RoomEvent::TrackSubscriptionFailed { error, .. } => {
            vec![VoiceEvent::Error(VoiceError::livekit(
                channel_id,
                format!("Could not subscribe to a remote audio track: {error}"),
            ))]
        }
        _ => Vec::new(),
    }
}

fn participant_id(participant: &livekit::prelude::Participant) -> Option<Id> {
    participant.identity().to_string().parse().ok()
}

impl VoiceBackend for LiveKitVoiceBackend {
    fn connect(&mut self, request: &VoiceJoinRequest) -> Result<ConnectedVoiceRoom, VoiceError> {
        let _previous = self.close_session()?;
        let runtime = self.runtime()?;
        let channel_id = request.channel_id;
        let room_name = request.room.clone();
        let input_tx = self.input_tx.clone();
        let url = request.url.clone();
        let token = request.token.clone();
        let device_preferences = request.device_preferences.clone();

        let session = runtime.block_on(async move {
            let audio = livekit::prelude::PlatformAudio::new().map_err(|error| {
                VoiceError::microphone_permission(channel_id, error.to_string())
            })?;
            apply_device_preferences(channel_id, &audio, &device_preferences)?;
            let track = livekit::prelude::LocalAudioTrack::create_audio_track(
                "microphone",
                audio.rtc_source(),
            );
            let mut options = livekit::prelude::RoomOptions::default();
            options.adaptive_stream = true;
            options.dynacast = true;
            let (room, mut room_events) = livekit::prelude::Room::connect(&url, &token, options)
                .await
                .map_err(|error| {
                    VoiceError::livekit(
                        channel_id,
                        format!("Could not connect to LiveKit room: {error}"),
                    )
                })?;
            let publish_options = livekit::options::TrackPublishOptions {
                source: livekit::prelude::TrackSource::Microphone,
                ..Default::default()
            };
            let publication = room
                .local_participant()
                .publish_track(
                    livekit::prelude::LocalTrack::Audio(track.clone()),
                    publish_options,
                )
                .await
                .map_err(|error| microphone_access_error(channel_id, error))?;
            let remote_audio_tracks = Arc::new(Mutex::new(Vec::new()));
            let deafened = Arc::new(Mutex::new(false));
            let event_tracks = Arc::clone(&remote_audio_tracks);
            let event_deafened = Arc::clone(&deafened);
            let event_task = tokio::spawn(async move {
                let mut active_speakers = BTreeSet::new();
                while let Some(event) = room_events.recv().await {
                    let voice_events = voice_events_from_room_event(
                        channel_id,
                        event,
                        &event_tracks,
                        &event_deafened,
                        &mut active_speakers,
                    );

                    for event in voice_events {
                        let should_stop = matches!(event, VoiceEvent::Disconnected { .. });
                        let _send_result =
                            input_tx.unbounded_send(WorkerInput::BackendEvent(event));
                        if should_stop {
                            return;
                        }
                    }
                }
            });

            Ok::<LiveKitSession, VoiceError>(LiveKitSession {
                channel_id,
                room,
                audio,
                track,
                _publication: publication,
                remote_audio_tracks,
                deafened,
                event_task,
            })
        })?;

        self.session = Some(session);

        Ok(ConnectedVoiceRoom {
            channel_id,
            room: room_name,
        })
    }

    fn disconnect(&mut self) -> Result<Option<ConnectedVoiceRoom>, VoiceError> {
        self.close_session()
    }

    fn set_muted(&mut self, muted: bool) -> Result<(), VoiceError> {
        let Some(session) = self.session.as_ref() else {
            return Err(VoiceError::worker(
                "Voice is not connected yet. Join a voice channel and try again.",
            ));
        };

        if muted {
            session.track.mute();
        } else {
            session.track.unmute();
        }

        Ok(())
    }

    fn set_deafened(&mut self, deafened: bool) -> Result<(), VoiceError> {
        let Some(session) = self.session.as_ref() else {
            return Err(VoiceError::worker(
                "Voice is not connected yet. Join a voice channel and try again.",
            ));
        };

        *session.deafened.lock().map_err(|_| {
            VoiceError::audio(
                session.channel_id,
                "Could not update remote audio playback because the voice worker is busy.",
            )
        })? = deafened;
        let tracks = session.remote_audio_tracks.lock().map_err(|_| {
            VoiceError::audio(
                session.channel_id,
                "Could not update remote audio playback because the voice worker is busy.",
            )
        })?;

        for track in tracks.iter() {
            if deafened {
                track.disable();
            } else {
                track.enable();
            }
        }

        Ok(())
    }
}

impl Drop for LiveKitVoiceBackend {
    fn drop(&mut self) {
        let _result = self.close_session();
    }
}
