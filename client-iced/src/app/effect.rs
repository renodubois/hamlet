use crate::auth::{
    AuthRequest, AuthenticatedRequest, AvatarDeleteRequest, AvatarUploadRequest,
    CreateChannelRequest, DeleteMessageRequest, EditMessageRequest, LogoutRequest,
    MessageHistoryRequest, ProfileUpdateRequest, ReorderChannelsRequest, SendMessageRequest,
    SessionRestoreRequest, SuppressMessageEmbedsRequest, TypingPingRequest,
    VoiceParticipantsRequest, VoiceSpeakingRequest, VoiceTokenRequest,
};
use crate::avatar::AvatarFetchRequest;
use crate::embeds::EmbedImageFetchRequest;
use crate::storage::Preferences;
use crate::voice::VoiceCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEffect {
    LoadPreferences,
    SavePreferences(Preferences),
    Authenticate(AuthRequest),
    RestoreSession(SessionRestoreRequest),
    Logout(LogoutRequest),
    LoadChannels(AuthenticatedRequest),
    CreateChannel(CreateChannelRequest),
    ReorderChannels(ReorderChannelsRequest),
    UpdateProfile(ProfileUpdateRequest),
    PickAvatarFile,
    UploadAvatar(AvatarUploadRequest),
    DeleteAvatar(AvatarDeleteRequest),
    LoadAvatarImage(AvatarFetchRequest),
    LoadEmbedImage(EmbedImageFetchRequest),
    LoadMessageHistory(MessageHistoryRequest),
    SendMessage(SendMessageRequest),
    PostTyping(TypingPingRequest),
    EditMessage(EditMessageRequest),
    DeleteMessage(DeleteMessageRequest),
    SuppressMessageEmbeds(SuppressMessageEmbedsRequest),
    LoadVoiceParticipants(VoiceParticipantsRequest),
    LoadVoiceToken(VoiceTokenRequest),
    SaveVoicePreferences(Preferences),
    PostVoiceSpeaking(VoiceSpeakingRequest),
    OpenExternalUrl(String),
    SendVoiceCommand(VoiceCommand),
    StartRealtime(AuthenticatedRequest),
    StopRealtime,
}
