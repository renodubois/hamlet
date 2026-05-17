use std::path::PathBuf;

use crate::api::ApiError;
use crate::auth::AuthSession;
use crate::avatar::AvatarImageError;
use crate::embeds::EmbedImageError;
use crate::emoji::EmojiPickerNavigation;
use crate::external_open::ExternalOpenError;
use crate::protocol::{
    Channel, ChannelKind, Id, Message, MessageEmbedsUpdatedEvent, User, VoiceParticipant,
    VoiceToken,
};
use crate::realtime::{RealtimeError, RealtimeEvent};
use crate::storage::Preferences;
use crate::voice::VoiceEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMoveDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMessage {
    PreferencesLoaded(Result<Preferences, String>),
    ServerUrlEdited(String),
    SaveServerUrlRequested,
    ServerUrlSaved(Result<Preferences, String>),
    UsernameEdited(String),
    PasswordEdited(String),
    LoginPressed,
    RegisterPressed,
    UseDevCredentials,
    AuthCompleted(Result<AuthSession, ApiError>),
    SessionRestoreCompleted(Result<AuthSession, ApiError>),
    LogoutPressed,
    LogoutCompleted(Result<(), ApiError>),
    OpenSettingsPressed,
    CloseSettingsPressed,
    ProfileDisplayNameEdited(String),
    SaveDisplayNamePressed,
    ClearDisplayNamePressed,
    ProfileUpdated(Result<User, ApiError>),
    SelectAvatarPressed,
    AvatarFileSelected(Result<Option<PathBuf>, String>),
    DeleteAvatarPressed,
    AvatarUploaded(Result<User, ApiError>),
    AvatarDeleted(Result<User, ApiError>),
    AvatarImageLoaded {
        url: String,
        result: Result<Vec<u8>, AvatarImageError>,
    },
    EmbedImageLoaded {
        url: String,
        result: Result<Vec<u8>, EmbedImageError>,
    },
    ChannelsLoaded(Result<Vec<Channel>, ApiError>),
    ChannelSelected(Id),
    AddChannelPressed,
    CancelCreateChannelPressed,
    CreateChannelNameEdited(String),
    CreateChannelKindSelected(ChannelKind),
    CreateChannelPressed,
    ChannelCreated(Result<Channel, ApiError>),
    MoveChannelRequested {
        channel_id: Id,
        direction: ChannelMoveDirection,
    },
    ChannelReorderCompleted(Result<Vec<Channel>, ApiError>),
    RetryChannelsPressed,
    MessageHistoryLoaded {
        channel_id: Id,
        result: Result<Vec<Message>, ApiError>,
    },
    RetryMessageHistoryPressed,
    DraftEdited(String),
    TypingPingPosted(Result<(), ApiError>),
    TypingTimerTick,
    ToggleEmojiPickerPressed,
    CloseEmojiPickerPressed,
    EmojiSearchEdited(String),
    EmojiPickerNavigate(EmojiPickerNavigation),
    EmojiPickerSelectFocused,
    EmojiSelected(String),
    SendMessagePressed,
    MessageSent {
        channel_id: Id,
        result: Result<Message, ApiError>,
    },
    EditMessagePressed(Id),
    EditMessageDraftEdited(String),
    SaveMessageEditPressed,
    CancelMessageEditPressed,
    MessageEdited {
        message_id: Id,
        channel_id: Id,
        result: Result<Message, ApiError>,
    },
    DeleteMessagePressed(Id),
    MessageDeleted {
        message_id: Id,
        channel_id: Id,
        result: Result<(), ApiError>,
    },
    SuppressEmbedsPressed(Id),
    EmbedsSuppressed {
        message_id: Id,
        channel_id: Id,
        result: Result<MessageEmbedsUpdatedEvent, ApiError>,
    },
    OpenExternalUrlRequested(String),
    ExternalUrlOpened {
        url: String,
        result: Result<(), ExternalOpenError>,
    },
    VoiceParticipantsLoaded(Result<Vec<VoiceParticipant>, ApiError>),
    VoiceJoinPressed(Id),
    VoiceLeavePressed,
    VoiceMutePressed,
    VoiceUnmutePressed,
    VoiceDeafenPressed,
    VoiceUndeafenPressed,
    VoiceMicrophoneDeviceEdited(String),
    VoiceOutputDeviceEdited(String),
    SaveVoicePreferencesPressed,
    VoicePreferencesSaved(Result<Preferences, String>),
    VoiceSpeakingPosted(Result<(), ApiError>),
    VoiceTokenLoaded {
        channel_id: Id,
        result: Result<VoiceToken, ApiError>,
    },
    VoiceWorkerEvent(VoiceEvent),
    RealtimeStarted(Result<(), RealtimeError>),
    RealtimeStopped(Result<(), RealtimeError>),
    RealtimeEventsReceived(Vec<RealtimeEvent>),
    RealtimeReconnectDue,
}
