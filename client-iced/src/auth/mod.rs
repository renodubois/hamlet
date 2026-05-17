use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::avatar::AvatarImageCache;
use crate::embeds::EmbedImageCache;
use crate::emoji::EmojiPickerState;
use crate::external_open::ExternalLinkStatus;
use crate::protocol::{Channel, ChannelKind, Id, Message, User, UserTypingEvent, VoiceParticipant};
use crate::realtime::RealtimeConnectionState;
use crate::storage::{DEFAULT_SERVER_URL, Preferences, VoiceDevicePreferences};

pub const TYPING_PING_INTERVAL_MS: u64 = 2_000;
pub const TYPING_EXPIRY_MS: u64 = 2_500;
pub const TYPING_SWEEP_MS: u64 = 500;
pub const TYPING_LIST_LIMIT: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedOutState {
    pub server_url: String,
    pub username: String,
    pub password: String,
    pub server_url_status: ServerUrlStatus,
    pub auth_status: AuthStatus,
    pub notice: Option<String>,
    pub voice_preferences: VoiceDevicePreferences,
}

impl SignedOutState {
    pub fn new(preferences: &Preferences) -> Self {
        Self {
            server_url: preferences.server_url.clone(),
            username: String::new(),
            password: String::new(),
            server_url_status: ServerUrlStatus::Clean,
            auth_status: AuthStatus::Idle,
            notice: None,
            voice_preferences: preferences.voice.clone(),
        }
    }

    pub fn default_server_url() -> String {
        DEFAULT_SERVER_URL.to_string()
    }

    pub fn clear_auth_feedback(&mut self) {
        if matches!(self.auth_status, AuthStatus::Failed(_)) {
            self.auth_status = AuthStatus::Idle;
        }
        self.notice = None;
    }
}

impl Default for SignedOutState {
    fn default() -> Self {
        Self::new(&Preferences::default())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedInState {
    pub user: User,
    pub server_url: String,
    pub session_token: Option<String>,
    pub logout_status: LogoutStatus,
    pub profile_settings: ProfileSettingsState,
    pub channels: ChannelListState,
    pub create_channel: CreateChannelState,
    pub channel_reorder: ChannelReorderState,
    pub selected_channel_id: Option<Id>,
    pub message_history: MessageHistoryState,
    pub message_actions: MessageActionState,
    pub draft: String,
    pub emoji_picker: EmojiPickerState,
    pub send_status: SendMessageStatus,
    pub typing: TypingState,
    pub external_link_status: ExternalLinkStatus,
    pub avatar_images: AvatarImageCache,
    pub embed_images: EmbedImageCache,
    pub voice_presence: VoicePresenceState,
    pub voice_connection: VoiceConnectionState,
    pub voice_settings: VoiceSettingsState,
    pub realtime_status: RealtimeConnectionState,
}

impl SignedInState {
    pub fn new(
        session: AuthSession,
        server_url: String,
        voice_preferences: VoiceDevicePreferences,
    ) -> Self {
        let user = session.user;
        let profile_settings = ProfileSettingsState::new(&user);
        let voice_settings = VoiceSettingsState::new(&voice_preferences);

        Self {
            user,
            server_url,
            session_token: session.session_token,
            logout_status: LogoutStatus::Idle,
            profile_settings,
            channels: ChannelListState::Loading,
            create_channel: CreateChannelState::default(),
            channel_reorder: ChannelReorderState::Idle,
            selected_channel_id: None,
            message_history: MessageHistoryState::NotLoaded,
            message_actions: MessageActionState::default(),
            draft: String::new(),
            emoji_picker: EmojiPickerState::default(),
            send_status: SendMessageStatus::Idle,
            typing: TypingState::default(),
            external_link_status: ExternalLinkStatus::default(),
            avatar_images: AvatarImageCache::default(),
            embed_images: EmbedImageCache::default(),
            voice_presence: VoicePresenceState::default(),
            voice_connection: VoiceConnectionState::default(),
            voice_settings,
            realtime_status: RealtimeConnectionState::Connecting,
        }
    }

    pub fn display_name(&self) -> &str {
        self.user
            .display_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&self.user.username)
    }

    pub fn authenticated_request(&self) -> AuthenticatedRequest {
        AuthenticatedRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
        }
    }

    pub fn message_history_request(&self, channel_id: Id) -> MessageHistoryRequest {
        MessageHistoryRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
            channel_id,
        }
    }

    pub fn create_channel_request(&self, name: String, kind: ChannelKind) -> CreateChannelRequest {
        CreateChannelRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
            name,
            kind,
        }
    }

    pub fn send_message_request(&self, channel_id: Id, text: String) -> SendMessageRequest {
        SendMessageRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
            channel_id,
            text,
        }
    }

    pub fn typing_ping_request(&self, channel_id: Id) -> TypingPingRequest {
        TypingPingRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
            channel_id,
        }
    }

    pub fn edit_message_request(
        &self,
        message_id: Id,
        channel_id: Id,
        text: String,
    ) -> EditMessageRequest {
        EditMessageRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
            message_id,
            channel_id,
            text,
        }
    }

    pub fn delete_message_request(&self, message_id: Id, channel_id: Id) -> DeleteMessageRequest {
        DeleteMessageRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
            message_id,
            channel_id,
        }
    }

    pub fn suppress_embeds_request(
        &self,
        message_id: Id,
        channel_id: Id,
        suppress: bool,
    ) -> SuppressMessageEmbedsRequest {
        SuppressMessageEmbedsRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
            message_id,
            channel_id,
            suppress,
        }
    }

    pub fn message_action_visibility(&self, message: &Message) -> MessageActionVisibility {
        message_action_visibility(self.user.id, message)
    }

    pub fn can_manage_message(&self, message: &Message) -> bool {
        self.message_action_visibility(message).has_any_action()
    }

    pub fn reorder_channels_request(&self, ids: Vec<Id>) -> ReorderChannelsRequest {
        ReorderChannelsRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
            ids,
        }
    }

    pub fn profile_update_request(&self, display_name: Option<String>) -> ProfileUpdateRequest {
        ProfileUpdateRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
            display_name,
        }
    }

    pub fn avatar_upload_request(&self, path: PathBuf) -> AvatarUploadRequest {
        AvatarUploadRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
            path,
        }
    }

    pub fn avatar_delete_request(&self) -> AvatarDeleteRequest {
        AvatarDeleteRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
        }
    }

    pub fn voice_participants_request(&self, channel_ids: Vec<Id>) -> VoiceParticipantsRequest {
        VoiceParticipantsRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
            channel_ids,
        }
    }

    pub fn voice_token_request(&self, channel_id: Id) -> VoiceTokenRequest {
        VoiceTokenRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
            channel_id,
        }
    }

    pub fn voice_speaking_request(&self, channel_id: Id, speaking: bool) -> VoiceSpeakingRequest {
        VoiceSpeakingRequest {
            server_url: self.server_url.clone(),
            session_token: self.session_token.clone(),
            channel_id,
            speaking,
        }
    }

    pub fn loaded_channels(&self) -> &[Channel] {
        match &self.channels {
            ChannelListState::Loaded(channels) => channels,
            ChannelListState::NotLoaded
            | ChannelListState::Loading
            | ChannelListState::Failed(_) => &[],
        }
    }

    pub fn selected_channel(&self) -> Option<&Channel> {
        let selected_channel_id = self.selected_channel_id?;

        self.loaded_channels()
            .iter()
            .find(|channel| channel.id == selected_channel_id)
    }

    pub fn selected_text_channel_id(&self) -> Option<Id> {
        self.selected_channel()
            .filter(|channel| channel.kind == ChannelKind::Text)
            .map(|channel| channel.id)
    }

    pub fn is_text_channel(&self, channel_id: Id) -> bool {
        self.loaded_channels()
            .iter()
            .any(|channel| channel.id == channel_id && channel.kind == ChannelKind::Text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSession {
    pub user: User,
    pub session_token: Option<String>,
}

impl AuthSession {
    pub fn new(user: User, session_token: Option<String>) -> Self {
        Self {
            user,
            session_token,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerUrlStatus {
    Clean,
    Dirty,
    Saving,
    Saved,
    Failed(String),
}

impl ServerUrlStatus {
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Clean | Self::Dirty | Self::Saving => None,
            Self::Saved => Some("Server URL saved."),
            Self::Failed(message) => Some(message.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthAction {
    Login,
    Register,
}

impl AuthAction {
    pub fn submitting_label(self) -> &'static str {
        match self {
            Self::Login => "Logging in…",
            Self::Register => "Registering…",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRequest {
    pub action: AuthAction,
    pub server_url: String,
    pub username: String,
    pub password: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRestoreRequest {
    pub server_url: String,
    pub session_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogoutRequest {
    pub server_url: String,
    pub session_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AuthenticatedRequest {
    pub server_url: String,
    pub session_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageHistoryRequest {
    pub server_url: String,
    pub session_token: Option<String>,
    pub channel_id: Id,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateChannelRequest {
    pub server_url: String,
    pub session_token: Option<String>,
    pub name: String,
    pub kind: ChannelKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendMessageRequest {
    pub server_url: String,
    pub session_token: Option<String>,
    pub channel_id: Id,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypingPingRequest {
    pub server_url: String,
    pub session_token: Option<String>,
    pub channel_id: Id,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditMessageRequest {
    pub server_url: String,
    pub session_token: Option<String>,
    pub message_id: Id,
    pub channel_id: Id,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteMessageRequest {
    pub server_url: String,
    pub session_token: Option<String>,
    pub message_id: Id,
    pub channel_id: Id,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuppressMessageEmbedsRequest {
    pub server_url: String,
    pub session_token: Option<String>,
    pub message_id: Id,
    pub channel_id: Id,
    pub suppress: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReorderChannelsRequest {
    pub server_url: String,
    pub session_token: Option<String>,
    pub ids: Vec<Id>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileUpdateRequest {
    pub server_url: String,
    pub session_token: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarUploadRequest {
    pub server_url: String,
    pub session_token: Option<String>,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarDeleteRequest {
    pub server_url: String,
    pub session_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceParticipantsRequest {
    pub server_url: String,
    pub session_token: Option<String>,
    pub channel_ids: Vec<Id>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceTokenRequest {
    pub server_url: String,
    pub session_token: Option<String>,
    pub channel_id: Id,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceSpeakingRequest {
    pub server_url: String,
    pub session_token: Option<String>,
    pub channel_id: Id,
    pub speaking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStatus {
    Idle,
    Submitting(AuthAction),
    Failed(String),
}

impl AuthStatus {
    pub fn is_submitting(&self) -> bool {
        matches!(self, Self::Submitting(_))
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Idle | Self::Submitting(_) => None,
            Self::Failed(message) => Some(message.as_str()),
        }
    }

    pub fn submitting_label(&self) -> Option<&'static str> {
        match self {
            Self::Submitting(action) => Some(action.submitting_label()),
            Self::Idle | Self::Failed(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogoutStatus {
    Idle,
    LoggingOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSettingsState {
    pub is_open: bool,
    pub display_name_input: String,
    pub status: ProfileUpdateStatus,
    pub avatar_status: AvatarUpdateStatus,
}

impl ProfileSettingsState {
    pub fn new(user: &User) -> Self {
        Self {
            is_open: false,
            display_name_input: user.display_name.clone().unwrap_or_default(),
            status: ProfileUpdateStatus::Idle,
            avatar_status: AvatarUpdateStatus::Idle,
        }
    }

    pub fn sync_from_user(&mut self, user: &User) {
        self.display_name_input = user.display_name.clone().unwrap_or_default();
    }

    pub fn clear_feedback(&mut self) {
        if matches!(
            self.status,
            ProfileUpdateStatus::Failed(_) | ProfileUpdateStatus::Saved
        ) {
            self.status = ProfileUpdateStatus::Idle;
        }
        if matches!(
            self.avatar_status,
            AvatarUpdateStatus::Failed(_) | AvatarUpdateStatus::Saved
        ) {
            self.avatar_status = AvatarUpdateStatus::Idle;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileUpdateStatus {
    Idle,
    Saving,
    Saved,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvatarUpdateStatus {
    Idle,
    Selecting,
    Uploading,
    Deleting,
    Saved,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceSettingsState {
    pub microphone_device_id_input: String,
    pub output_device_id_input: String,
    pub status: VoicePreferenceStatus,
}

impl VoiceSettingsState {
    pub fn new(preferences: &VoiceDevicePreferences) -> Self {
        Self {
            microphone_device_id_input: preferences
                .microphone_device_id
                .clone()
                .unwrap_or_default(),
            output_device_id_input: preferences.output_device_id.clone().unwrap_or_default(),
            status: VoicePreferenceStatus::Idle,
        }
    }

    pub fn preferences(&self) -> VoiceDevicePreferences {
        VoiceDevicePreferences::new(
            Some(self.microphone_device_id_input.clone()),
            Some(self.output_device_id_input.clone()),
        )
    }

    pub fn sync_from_preferences(&mut self, preferences: &VoiceDevicePreferences) {
        self.microphone_device_id_input =
            preferences.microphone_device_id.clone().unwrap_or_default();
        self.output_device_id_input = preferences.output_device_id.clone().unwrap_or_default();
    }

    pub fn clear_feedback(&mut self) {
        if matches!(
            self.status,
            VoicePreferenceStatus::Failed(_) | VoicePreferenceStatus::Saved
        ) {
            self.status = VoicePreferenceStatus::Idle;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoicePreferenceStatus {
    Idle,
    Saving,
    Saved,
    Failed(String),
}

impl VoicePreferenceStatus {
    pub fn is_saving(&self) -> bool {
        matches!(self, Self::Saving)
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Saving => Some("Saving voice preferences…"),
            Self::Saved => Some("Voice preferences saved."),
            Self::Failed(message) => Some(message.as_str()),
            Self::Idle => None,
        }
    }
}

impl ProfileUpdateStatus {
    pub fn is_saving(&self) -> bool {
        matches!(self, Self::Saving)
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Saving => Some("Saving profile…"),
            Self::Saved => Some("Profile updated."),
            Self::Failed(message) => Some(message.as_str()),
            Self::Idle => None,
        }
    }
}

impl AvatarUpdateStatus {
    pub fn is_busy(&self) -> bool {
        matches!(self, Self::Selecting | Self::Uploading | Self::Deleting)
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Selecting => Some("Opening image picker…"),
            Self::Uploading => Some("Uploading avatar…"),
            Self::Deleting => Some("Removing avatar…"),
            Self::Saved => Some("Avatar updated."),
            Self::Failed(message) => Some(message.as_str()),
            Self::Idle => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateChannelState {
    pub is_open: bool,
    pub name: String,
    pub kind: ChannelKind,
    pub status: CreateChannelStatus,
}

impl CreateChannelState {
    pub fn clear_failure(&mut self) {
        if matches!(self.status, CreateChannelStatus::Failed(_)) {
            self.status = CreateChannelStatus::Idle;
        }
    }
}

impl Default for CreateChannelState {
    fn default() -> Self {
        Self {
            is_open: false,
            name: String::new(),
            kind: ChannelKind::Text,
            status: CreateChannelStatus::Idle,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateChannelStatus {
    Idle,
    Creating,
    Failed(String),
}

impl CreateChannelStatus {
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message.as_str()),
            Self::Idle | Self::Creating => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelReorderState {
    Idle,
    Committing { previous_channels: Vec<Channel> },
    Failed(String),
}

impl ChannelReorderState {
    pub fn is_committing(&self) -> bool {
        matches!(self, Self::Committing { .. })
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Committing { .. } => Some("Saving channel order…"),
            Self::Failed(message) => Some(message.as_str()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelListState {
    NotLoaded,
    Loading,
    Loaded(Vec<Channel>),
    Failed(String),
}

impl ChannelListState {
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message.as_str()),
            Self::NotLoaded | Self::Loading | Self::Loaded(_) => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypingState {
    pub clock_ms: u64,
    pub outgoing_cooldown_ms: u64,
    pub indicators_by_channel: BTreeMap<Id, BTreeMap<Id, TypingIndicator>>,
}

impl TypingState {
    pub fn can_send_ping(&self) -> bool {
        self.outgoing_cooldown_ms == 0
    }

    pub fn mark_ping_sent(&mut self) {
        self.outgoing_cooldown_ms = TYPING_PING_INTERVAL_MS;
    }

    pub fn reset_outgoing(&mut self) {
        self.outgoing_cooldown_ms = 0;
    }

    pub fn tick(&mut self) {
        self.clock_ms = self.clock_ms.saturating_add(TYPING_SWEEP_MS);
        self.outgoing_cooldown_ms = self.outgoing_cooldown_ms.saturating_sub(TYPING_SWEEP_MS);
        self.expire_stale();
    }

    pub fn note_user_typing(&mut self, event: UserTypingEvent) {
        self.indicators_by_channel
            .entry(event.channel_id)
            .or_default()
            .insert(
                event.user_id,
                TypingIndicator {
                    username: event.username,
                    last_seen_ms: self.clock_ms,
                },
            );
    }

    pub fn clear_user(&mut self, channel_id: Id, user_id: Id) {
        let should_remove_channel =
            if let Some(indicators) = self.indicators_by_channel.get_mut(&channel_id) {
                indicators.remove(&user_id);
                indicators.is_empty()
            } else {
                false
            };

        if should_remove_channel {
            self.indicators_by_channel.remove(&channel_id);
        }
    }

    pub fn clear(&mut self) {
        self.outgoing_cooldown_ms = 0;
        self.indicators_by_channel.clear();
    }

    pub fn retain_channels(&mut self, channel_ids: &[Id]) {
        let channel_ids = channel_ids.iter().copied().collect::<BTreeSet<_>>();
        self.indicators_by_channel
            .retain(|channel_id, _| channel_ids.contains(channel_id));
    }

    pub fn usernames_for_channel(&self, channel_id: Id) -> Vec<String> {
        let mut usernames = self
            .indicators_by_channel
            .get(&channel_id)
            .map(|indicators| {
                indicators
                    .values()
                    .map(|indicator| indicator.username.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        usernames.sort();
        usernames
    }

    pub fn indicator_message(&self, channel_id: Id) -> Option<String> {
        format_typing_indicator(&self.usernames_for_channel(channel_id))
    }

    fn expire_stale(&mut self) {
        self.indicators_by_channel.retain(|_, indicators| {
            indicators.retain(|_, indicator| {
                self.clock_ms.saturating_sub(indicator.last_seen_ms) < TYPING_EXPIRY_MS
            });
            !indicators.is_empty()
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypingIndicator {
    pub username: String,
    pub last_seen_ms: u64,
}

pub fn format_typing_indicator(usernames: &[String]) -> Option<String> {
    if usernames.len() > TYPING_LIST_LIMIT {
        return Some("Several people are typing…".to_string());
    }

    match usernames {
        [] => None,
        [name] => Some(format!("{name} is typing…")),
        [first, second] => Some(format!("{first} and {second} are typing…")),
        [first, second, third] => Some(format!("{first}, {second}, and {third} are typing…")),
        _ => Some("Several people are typing…".to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoicePresenceState {
    pub status: VoicePresenceStatus,
    pub participants_by_channel: BTreeMap<Id, Vec<VoiceParticipant>>,
    pub speaking_by_channel: BTreeMap<Id, BTreeSet<Id>>,
}

impl VoicePresenceState {
    pub fn begin_loading(&mut self, channel_ids: &[Id]) {
        self.participants_by_channel.clear();
        self.speaking_by_channel.clear();
        for channel_id in channel_ids {
            self.participants_by_channel.entry(*channel_id).or_default();
            self.speaking_by_channel.entry(*channel_id).or_default();
        }
        self.status = if channel_ids.is_empty() {
            VoicePresenceStatus::Loaded
        } else {
            VoicePresenceStatus::Loading
        };
    }

    pub fn apply_snapshot(&mut self, channel_ids: &[Id], participants: Vec<VoiceParticipant>) {
        self.participants_by_channel.clear();
        self.speaking_by_channel.clear();
        for channel_id in channel_ids {
            self.participants_by_channel.entry(*channel_id).or_default();
            self.speaking_by_channel.entry(*channel_id).or_default();
        }
        for participant in participants
            .into_iter()
            .filter(|participant| channel_ids.contains(&participant.channel_id))
        {
            self.upsert_participant(participant);
        }
        self.status = VoicePresenceStatus::Loaded;
    }

    pub fn fail(&mut self, message: String) {
        self.status = VoicePresenceStatus::Failed(message);
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn participants(&self, channel_id: Id) -> &[VoiceParticipant] {
        self.participants_by_channel
            .get(&channel_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn is_speaking(&self, channel_id: Id, user_id: Id) -> bool {
        self.speaking_by_channel
            .get(&channel_id)
            .is_some_and(|speakers| speakers.contains(&user_id))
    }

    pub fn set_speaking(&mut self, channel_id: Id, user_id: Id, speaking: bool) {
        let speakers = self.speaking_by_channel.entry(channel_id).or_default();

        if speaking {
            speakers.insert(user_id);
        } else {
            speakers.remove(&user_id);
        }
    }

    pub fn clear_speaking_for_channel(&mut self, channel_id: Id) {
        if let Some(speakers) = self.speaking_by_channel.get_mut(&channel_id) {
            speakers.clear();
        }
    }

    pub fn upsert_participant(&mut self, participant: VoiceParticipant) {
        let participants = self
            .participants_by_channel
            .entry(participant.channel_id)
            .or_default();

        if let Some(existing) = participants
            .iter_mut()
            .find(|existing| existing.user_id == participant.user_id)
        {
            *existing = participant;
        } else {
            participants.push(participant);
        }

        sort_voice_participants(participants);
    }

    pub fn remove_participant(&mut self, channel_id: Id, user_id: Id) {
        if let Some(participants) = self.participants_by_channel.get_mut(&channel_id) {
            participants.retain(|participant| participant.user_id != user_id);
        }
        self.set_speaking(channel_id, user_id, false);
    }

    pub fn message(&self) -> Option<String> {
        match &self.status {
            VoicePresenceStatus::Loading => Some("Loading voice participants…".to_string()),
            VoicePresenceStatus::Failed(message) => {
                Some(format!("Voice presence unavailable: {message}"))
            }
            VoicePresenceStatus::NotLoaded | VoicePresenceStatus::Loaded => None,
        }
    }
}

impl Default for VoicePresenceState {
    fn default() -> Self {
        Self {
            status: VoicePresenceStatus::NotLoaded,
            participants_by_channel: BTreeMap::new(),
            speaking_by_channel: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoicePresenceStatus {
    NotLoaded,
    Loading,
    Loaded,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceConnectionState {
    pub status: VoiceConnectionStatus,
    pub error: Option<VoiceConnectionError>,
    pub muted: bool,
    pub deafened: bool,
}

impl VoiceConnectionState {
    pub fn begin_connecting(&mut self, channel_id: Id) {
        self.status = VoiceConnectionStatus::Connecting { channel_id };
        self.error = None;
        self.muted = false;
        self.deafened = false;
    }

    pub fn begin_disconnecting(&mut self) {
        self.error = None;
        self.status = self
            .target_channel_id()
            .map(|channel_id| VoiceConnectionStatus::Disconnecting { channel_id })
            .unwrap_or(VoiceConnectionStatus::Idle);
    }

    pub fn set_connected(&mut self, channel_id: Id, room: String) {
        self.status = VoiceConnectionStatus::Connected { channel_id, room };
        self.error = None;
    }

    pub fn set_idle(&mut self) {
        self.status = VoiceConnectionStatus::Idle;
        self.muted = false;
        self.deafened = false;
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    pub fn set_deafened(&mut self, deafened: bool) {
        self.deafened = deafened;
    }

    pub fn fail(&mut self, channel_id: Option<Id>, message: String) {
        self.status = VoiceConnectionStatus::Idle;
        self.muted = false;
        self.deafened = false;
        self.error = Some(VoiceConnectionError {
            channel_id,
            message,
        });
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn connected_channel_id(&self) -> Option<Id> {
        match self.status {
            VoiceConnectionStatus::Connected { channel_id, .. } => Some(channel_id),
            VoiceConnectionStatus::Idle
            | VoiceConnectionStatus::Connecting { .. }
            | VoiceConnectionStatus::Disconnecting { .. } => None,
        }
    }

    pub fn target_channel_id(&self) -> Option<Id> {
        match self.status {
            VoiceConnectionStatus::Connecting { channel_id }
            | VoiceConnectionStatus::Connected { channel_id, .. }
            | VoiceConnectionStatus::Disconnecting { channel_id } => Some(channel_id),
            VoiceConnectionStatus::Idle => None,
        }
    }

    pub fn is_connected_to(&self, channel_id: Id) -> bool {
        self.connected_channel_id() == Some(channel_id)
    }

    pub fn is_connecting_to(&self, channel_id: Id) -> bool {
        matches!(self.status, VoiceConnectionStatus::Connecting { channel_id: id } if id == channel_id)
    }

    pub fn is_disconnecting_from(&self, channel_id: Id) -> bool {
        matches!(self.status, VoiceConnectionStatus::Disconnecting { channel_id: id } if id == channel_id)
    }

    pub fn has_active_connection(&self) -> bool {
        !matches!(self.status, VoiceConnectionStatus::Idle)
    }

    pub fn message(&self) -> Option<String> {
        match &self.error {
            Some(error) => Some(format!("Voice connection error: {}", error.message)),
            None => match &self.status {
                VoiceConnectionStatus::Idle => None,
                VoiceConnectionStatus::Connecting { .. } => {
                    Some("Connecting to voice…".to_string())
                }
                VoiceConnectionStatus::Connected { room, .. } => {
                    let mute_state = if self.muted { " Microphone muted." } else { "" };
                    let deafen_state = if self.deafened {
                        " Audio deafened."
                    } else {
                        ""
                    };
                    Some(format!(
                        "Connected to voice room {room}.{mute_state}{deafen_state}"
                    ))
                }
                VoiceConnectionStatus::Disconnecting { .. } => Some("Leaving voice…".to_string()),
            },
        }
    }
}

impl Default for VoiceConnectionState {
    fn default() -> Self {
        Self {
            status: VoiceConnectionStatus::Idle,
            error: None,
            muted: false,
            deafened: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceConnectionStatus {
    Idle,
    Connecting { channel_id: Id },
    Connected { channel_id: Id, room: String },
    Disconnecting { channel_id: Id },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceConnectionError {
    pub channel_id: Option<Id>,
    pub message: String,
}

fn sort_voice_participants(participants: &mut [VoiceParticipant]) {
    participants.sort_by(|left, right| {
        left.username
            .cmp(&right.username)
            .then_with(|| left.user_id.cmp(&right.user_id))
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageActionVisibility {
    pub can_edit: bool,
    pub can_delete: bool,
    pub can_suppress_embeds: bool,
}

impl MessageActionVisibility {
    pub fn has_any_action(self) -> bool {
        self.can_edit || self.can_delete || self.can_suppress_embeds
    }
}

pub fn message_action_visibility(
    current_user_id: Id,
    message: &Message,
) -> MessageActionVisibility {
    let is_owner = message.user_id == current_user_id;
    let has_visible_embeds = !message.suppress_embeds && !message.embeds.is_empty();

    MessageActionVisibility {
        can_edit: is_owner,
        can_delete: is_owner,
        can_suppress_embeds: is_owner && has_visible_embeds,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageActionState {
    pub editing: Option<MessageEditState>,
    pub delete_status: MessageDeleteStatus,
    pub suppress_status: MessageSuppressStatus,
}

impl MessageActionState {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn clear_for_message(&mut self, message_id: Id) {
        if self
            .editing
            .as_ref()
            .is_some_and(|editing| editing.message_id == message_id)
        {
            self.editing = None;
        }

        if self.delete_status.is_for_message(message_id) {
            self.delete_status = MessageDeleteStatus::Idle;
        }

        self.clear_suppress_for_message(message_id);
    }

    pub fn clear_suppress_for_message(&mut self, message_id: Id) {
        if self.suppress_status.is_for_message(message_id) {
            self.suppress_status = MessageSuppressStatus::Idle;
        }
    }

    pub fn is_editing(&self, message_id: Id) -> bool {
        self.editing
            .as_ref()
            .is_some_and(|editing| editing.message_id == message_id)
    }

    pub fn is_deleting(&self, message_id: Id) -> bool {
        self.delete_status.is_deleting_message(message_id)
    }

    pub fn delete_message(&self, message_id: Id) -> Option<&str> {
        self.delete_status.message_for(message_id)
    }

    pub fn is_suppressing_embeds(&self, message_id: Id) -> bool {
        self.suppress_status.is_suppressing_message(message_id)
    }

    pub fn suppress_embeds_message(&self, message_id: Id) -> Option<&str> {
        self.suppress_status.message_for(message_id)
    }
}

impl Default for MessageActionState {
    fn default() -> Self {
        Self {
            editing: None,
            delete_status: MessageDeleteStatus::Idle,
            suppress_status: MessageSuppressStatus::Idle,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageEditState {
    pub message_id: Id,
    pub channel_id: Id,
    pub draft: String,
    pub status: MessageEditStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageEditStatus {
    Editing,
    Saving,
    Failed(String),
}

impl MessageEditStatus {
    pub fn is_saving(&self) -> bool {
        matches!(self, Self::Saving)
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message.as_str()),
            Self::Editing | Self::Saving => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageDeleteStatus {
    Idle,
    Deleting {
        message_id: Id,
        channel_id: Id,
    },
    Failed {
        message_id: Id,
        channel_id: Id,
        message: String,
    },
}

impl MessageDeleteStatus {
    pub fn is_for_message(&self, target_message_id: Id) -> bool {
        match self {
            Self::Deleting { message_id, .. } | Self::Failed { message_id, .. } => {
                *message_id == target_message_id
            }
            Self::Idle => false,
        }
    }

    pub fn is_deleting_message(&self, target_message_id: Id) -> bool {
        matches!(self, Self::Deleting { message_id, .. } if *message_id == target_message_id)
    }

    pub fn is_deleting(&self) -> bool {
        matches!(self, Self::Deleting { .. })
    }

    pub fn matches_deleting(&self, target_message_id: Id, target_channel_id: Id) -> bool {
        matches!(
            self,
            Self::Deleting {
                message_id,
                channel_id,
            } if *message_id == target_message_id && *channel_id == target_channel_id
        )
    }

    pub fn message_for(&self, target_message_id: Id) -> Option<&str> {
        match self {
            Self::Failed {
                message_id,
                message,
                ..
            } if *message_id == target_message_id => Some(message.as_str()),
            Self::Idle | Self::Deleting { .. } | Self::Failed { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageSuppressStatus {
    Idle,
    Suppressing {
        message_id: Id,
        channel_id: Id,
    },
    Failed {
        message_id: Id,
        channel_id: Id,
        message: String,
    },
}

impl MessageSuppressStatus {
    pub fn is_for_message(&self, target_message_id: Id) -> bool {
        match self {
            Self::Suppressing { message_id, .. } | Self::Failed { message_id, .. } => {
                *message_id == target_message_id
            }
            Self::Idle => false,
        }
    }

    pub fn is_suppressing_message(&self, target_message_id: Id) -> bool {
        matches!(self, Self::Suppressing { message_id, .. } if *message_id == target_message_id)
    }

    pub fn is_suppressing(&self) -> bool {
        matches!(self, Self::Suppressing { .. })
    }

    pub fn matches_suppressing(&self, target_message_id: Id, target_channel_id: Id) -> bool {
        matches!(
            self,
            Self::Suppressing {
                message_id,
                channel_id,
            } if *message_id == target_message_id && *channel_id == target_channel_id
        )
    }

    pub fn message_for(&self, target_message_id: Id) -> Option<&str> {
        match self {
            Self::Failed {
                message_id,
                message,
                ..
            } if *message_id == target_message_id => Some(message.as_str()),
            Self::Idle | Self::Suppressing { .. } | Self::Failed { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageHistoryState {
    NotLoaded,
    Loading {
        channel_id: Id,
    },
    Loaded {
        channel_id: Id,
        messages: Vec<Message>,
    },
    Failed {
        channel_id: Id,
        message: String,
    },
}

impl MessageHistoryState {
    pub fn is_for_channel(&self, channel_id: Id) -> bool {
        match self {
            Self::Loading { channel_id: id }
            | Self::Loaded { channel_id: id, .. }
            | Self::Failed { channel_id: id, .. } => *id == channel_id,
            Self::NotLoaded => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendMessageStatus {
    Idle,
    Sending,
    Failed(String),
}

impl SendMessageStatus {
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message.as_str()),
            Self::Idle | Self::Sending => None,
        }
    }
}
