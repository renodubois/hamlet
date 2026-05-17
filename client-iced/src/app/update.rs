use std::time::Duration;

use iced::{Subscription, Task, event, keyboard};

use crate::api::{ApiClient, ApiError, runtime_api};
use crate::auth::{
    AuthAction, AuthRequest, AuthSession, AuthStatus, AvatarUpdateStatus, ChannelListState,
    ChannelReorderState, CreateChannelStatus, LogoutRequest, LogoutStatus, MessageDeleteStatus,
    MessageEditState, MessageEditStatus, MessageHistoryState, MessageSuppressStatus,
    ProfileUpdateStatus, SendMessageStatus, ServerUrlStatus, SessionRestoreRequest,
    TYPING_SWEEP_MS, VoiceConnectionStatus, VoicePreferenceStatus,
};
use crate::avatar::{AvatarFetchRequest, fetch_avatar_image};
use crate::embeds::{EmbedImageFetchRequest, fetch_embed_image};
use crate::emoji::{EmojiPickerFocusTarget, EmojiPickerNavigation};
use crate::external_open::{
    ExternalLinkStatus, ExternalOpenError, ExternalOpenService, PlatformExternalOpen,
    validate_external_url,
};
use crate::protocol::{
    BroadcastEvent, Channel, ChannelKind, Id, Message, MessageEmbedsUpdatedEvent, User,
    UserTypingEvent, VoiceParticipant, VoiceToken,
};
use crate::realtime::{RealtimeConnectionState, RealtimeError, RealtimeEvent, ReconnectPolicy};
use crate::storage::{FileStorage, Preferences, Storage};
use crate::voice::{VoiceCommand, VoiceEvent, VoiceJoinRequest};

use super::effect::AppEffect;
use super::message::{AppMessage, ChannelMoveDirection};
use super::state::{AppState, BootStatus};
use super::widget_ids::{COMPOSER_INPUT_ID, EMOJI_SEARCH_INPUT_ID};

const DISPLAY_NAME_MAX_LEN: usize = 64;

pub fn boot() -> (AppState, Vec<AppEffect>) {
    (AppState::booting(), vec![AppEffect::LoadPreferences])
}

pub fn boot_runtime() -> (AppState, Task<AppMessage>) {
    let (state, effects) = boot();

    (state, effects_to_task(effects))
}

pub fn reduce(state: &mut AppState, message: AppMessage) -> Vec<AppEffect> {
    match message {
        AppMessage::PreferencesLoaded(Ok(preferences)) => {
            apply_loaded_preferences(state, preferences)
        }
        AppMessage::PreferencesLoaded(Err(message)) => {
            state.boot_status = BootStatus::Ready;
            state.signed_out.server_url_status = ServerUrlStatus::Failed(format!(
                "Could not load saved preferences: {message}. Using the default server URL."
            ));
            Vec::new()
        }
        AppMessage::ServerUrlEdited(server_url) => {
            state.signed_out.server_url = server_url;
            state.signed_out.server_url_status = ServerUrlStatus::Dirty;
            state.signed_out.clear_auth_feedback();
            Vec::new()
        }
        AppMessage::SaveServerUrlRequested => {
            match Preferences::with_server_url_session_token_and_voice(
                state.signed_out.server_url.clone(),
                None,
                state.signed_out.voice_preferences.clone(),
            ) {
                Ok(preferences) => {
                    state.signed_out.server_url = preferences.server_url.clone();
                    state.signed_out.server_url_status = ServerUrlStatus::Saving;
                    vec![AppEffect::SavePreferences(preferences)]
                }
                Err(error) => {
                    state.signed_out.server_url_status = ServerUrlStatus::Failed(error.to_string());
                    Vec::new()
                }
            }
        }
        AppMessage::ServerUrlSaved(Ok(preferences)) => {
            state.signed_out.server_url = preferences.server_url;
            state.signed_out.voice_preferences = preferences.voice;
            state.signed_out.server_url_status = ServerUrlStatus::Saved;
            Vec::new()
        }
        AppMessage::ServerUrlSaved(Err(message)) => {
            state.signed_out.server_url_status = ServerUrlStatus::Failed(message);
            Vec::new()
        }
        AppMessage::UsernameEdited(username) => {
            state.signed_out.username = username;
            state.signed_out.clear_auth_feedback();
            Vec::new()
        }
        AppMessage::PasswordEdited(password) => {
            state.signed_out.password = password;
            state.signed_out.clear_auth_feedback();
            Vec::new()
        }
        AppMessage::LoginPressed => begin_auth(state, AuthAction::Login),
        AppMessage::RegisterPressed => begin_auth(state, AuthAction::Register),
        AppMessage::UseDevCredentials => use_dev_credentials(state),
        AppMessage::AuthCompleted(Ok(session)) => complete_auth(state, session),
        AppMessage::AuthCompleted(Err(error)) => {
            state.signed_out.auth_status = AuthStatus::Failed(error.user_message());
            Vec::new()
        }
        AppMessage::SessionRestoreCompleted(Ok(session)) => {
            complete_session_restore(state, session)
        }
        AppMessage::SessionRestoreCompleted(Err(error)) => fail_session_restore(state, error),
        AppMessage::LogoutPressed => begin_logout(state),
        AppMessage::LogoutCompleted(result) => complete_logout(state, result),
        AppMessage::OpenSettingsPressed => open_settings(state),
        AppMessage::CloseSettingsPressed => close_settings(state),
        AppMessage::ProfileDisplayNameEdited(display_name) => {
            edit_profile_display_name(state, display_name)
        }
        AppMessage::SaveDisplayNamePressed => begin_profile_update(state),
        AppMessage::ClearDisplayNamePressed => begin_profile_clear(state),
        AppMessage::ProfileUpdated(result) => complete_profile_update(state, result),
        AppMessage::SelectAvatarPressed => begin_avatar_file_selection(state),
        AppMessage::AvatarFileSelected(result) => complete_avatar_file_selection(state, result),
        AppMessage::DeleteAvatarPressed => begin_avatar_delete(state),
        AppMessage::AvatarUploaded(result) => {
            complete_avatar_update(state, result, AvatarAction::Upload)
        }
        AppMessage::AvatarDeleted(result) => {
            complete_avatar_update(state, result, AvatarAction::Delete)
        }
        AppMessage::AvatarImageLoaded { url, result } => {
            complete_avatar_image_load(state, url, result)
        }
        AppMessage::EmbedImageLoaded { url, result } => {
            complete_embed_image_load(state, url, result)
        }
        AppMessage::ChannelsLoaded(result) => complete_channels_load(state, result),
        AppMessage::ChannelSelected(channel_id) => select_channel(state, channel_id),
        AppMessage::AddChannelPressed => open_create_channel(state),
        AppMessage::CancelCreateChannelPressed => close_create_channel(state),
        AppMessage::CreateChannelNameEdited(name) => edit_create_channel_name(state, name),
        AppMessage::CreateChannelKindSelected(kind) => select_create_channel_kind(state, kind),
        AppMessage::CreateChannelPressed => begin_create_channel(state),
        AppMessage::ChannelCreated(result) => complete_create_channel(state, result),
        AppMessage::MoveChannelRequested {
            channel_id,
            direction,
        } => begin_channel_move(state, channel_id, direction),
        AppMessage::ChannelReorderCompleted(result) => complete_channel_reorder(state, result),
        AppMessage::RetryChannelsPressed => retry_channels(state),
        AppMessage::MessageHistoryLoaded { channel_id, result } => {
            complete_message_history_load(state, channel_id, result)
        }
        AppMessage::RetryMessageHistoryPressed => retry_message_history(state),
        AppMessage::DraftEdited(draft) => edit_draft(state, draft),
        AppMessage::TypingPingPosted(_result) => Vec::new(),
        AppMessage::TypingTimerTick => advance_typing_timers(state),
        AppMessage::ToggleEmojiPickerPressed => toggle_emoji_picker(state),
        AppMessage::CloseEmojiPickerPressed => close_emoji_picker(state),
        AppMessage::EmojiSearchEdited(query) => edit_emoji_search(state, query),
        AppMessage::EmojiPickerNavigate(navigation) => navigate_emoji_picker(state, navigation),
        AppMessage::EmojiPickerSelectFocused => select_focused_emoji(state),
        AppMessage::EmojiSelected(emoji) => insert_selected_emoji(state, &emoji),
        AppMessage::SendMessagePressed => begin_send_message(state),
        AppMessage::MessageSent { channel_id, result } => {
            complete_send_message(state, channel_id, result)
        }
        AppMessage::EditMessagePressed(message_id) => begin_message_edit(state, message_id),
        AppMessage::EditMessageDraftEdited(draft) => edit_message_draft(state, draft),
        AppMessage::SaveMessageEditPressed => begin_message_update(state),
        AppMessage::CancelMessageEditPressed => cancel_message_edit(state),
        AppMessage::MessageEdited {
            message_id,
            channel_id,
            result,
        } => complete_message_update(state, message_id, channel_id, result),
        AppMessage::DeleteMessagePressed(message_id) => begin_message_delete(state, message_id),
        AppMessage::MessageDeleted {
            message_id,
            channel_id,
            result,
        } => complete_message_delete(state, message_id, channel_id, result),
        AppMessage::SuppressEmbedsPressed(message_id) => begin_suppress_embeds(state, message_id),
        AppMessage::EmbedsSuppressed {
            message_id,
            channel_id,
            result,
        } => complete_suppress_embeds(state, message_id, channel_id, result),
        AppMessage::OpenExternalUrlRequested(url) => begin_external_url_open(state, url),
        AppMessage::ExternalUrlOpened { url, result } => {
            complete_external_url_open(state, url, result)
        }
        AppMessage::VoiceParticipantsLoaded(result) => {
            complete_voice_participants_load(state, result)
        }
        AppMessage::VoiceJoinPressed(channel_id) => begin_voice_join(state, channel_id),
        AppMessage::VoiceLeavePressed => begin_voice_leave(state),
        AppMessage::VoiceMutePressed => begin_voice_mute(state),
        AppMessage::VoiceUnmutePressed => begin_voice_unmute(state),
        AppMessage::VoiceDeafenPressed => begin_voice_deafen(state),
        AppMessage::VoiceUndeafenPressed => begin_voice_undeafen(state),
        AppMessage::VoiceMicrophoneDeviceEdited(device_id) => {
            edit_voice_microphone_device(state, device_id)
        }
        AppMessage::VoiceOutputDeviceEdited(device_id) => {
            edit_voice_output_device(state, device_id)
        }
        AppMessage::SaveVoicePreferencesPressed => begin_voice_preferences_save(state),
        AppMessage::VoicePreferencesSaved(result) => complete_voice_preferences_save(state, result),
        AppMessage::VoiceSpeakingPosted(_result) => Vec::new(),
        AppMessage::VoiceTokenLoaded { channel_id, result } => {
            complete_voice_token_load(state, channel_id, result)
        }
        AppMessage::VoiceWorkerEvent(event) => apply_voice_worker_event(state, event),
        AppMessage::RealtimeStarted(result) => complete_realtime_start(state, result),
        AppMessage::RealtimeStopped(result) => complete_realtime_stop(state, result),
        AppMessage::RealtimeEventsReceived(events) => apply_realtime_events(state, events),
        AppMessage::RealtimeReconnectDue => reconnect_realtime(state),
    }
}

pub fn update_runtime(state: &mut AppState, message: AppMessage) -> Task<AppMessage> {
    let effects = reduce(state, message.clone());
    let focus_task = emoji_focus_task(&message, state);

    Task::batch([effects_to_task(effects), focus_task])
}

pub fn subscription_runtime(state: &AppState) -> Subscription<AppMessage> {
    let Some(signed_in) = &state.signed_in else {
        return Subscription::none();
    };

    let realtime = match signed_in.realtime_status {
        RealtimeConnectionState::Connecting | RealtimeConnectionState::Connected => {
            Subscription::run_with(
                signed_in.authenticated_request(),
                crate::realtime::event_source_stream,
            )
            .map(|event| AppMessage::RealtimeEventsReceived(vec![event]))
        }
        RealtimeConnectionState::Disconnected
        | RealtimeConnectionState::BackingOff { .. }
        | RealtimeConnectionState::AuthExpired => Subscription::none(),
    };
    let voice = Subscription::run_with(
        signed_in.authenticated_request(),
        crate::voice::worker_stream,
    )
    .map(AppMessage::VoiceWorkerEvent);
    let emoji_picker_keyboard = if signed_in.emoji_picker.is_open {
        event::listen_with(emoji_picker_keyboard_message)
    } else {
        Subscription::none()
    };
    let typing_timer = iced::time::every(Duration::from_millis(TYPING_SWEEP_MS))
        .map(|_| AppMessage::TypingTimerTick);

    Subscription::batch([realtime, voice, emoji_picker_keyboard, typing_timer])
}

pub fn effects_to_task(effects: Vec<AppEffect>) -> Task<AppMessage> {
    Task::batch(effects.into_iter().map(effect_to_task))
}

fn emoji_focus_task(message: &AppMessage, state: &AppState) -> Task<AppMessage> {
    if !matches!(
        message,
        AppMessage::ToggleEmojiPickerPressed
            | AppMessage::CloseEmojiPickerPressed
            | AppMessage::EmojiPickerSelectFocused
            | AppMessage::EmojiSelected(_)
    ) {
        return Task::none();
    }

    let Some(signed_in) = &state.signed_in else {
        return Task::none();
    };

    match signed_in.emoji_picker.focus_target {
        EmojiPickerFocusTarget::Search if signed_in.emoji_picker.is_open => {
            iced::widget::operation::focus(EMOJI_SEARCH_INPUT_ID)
        }
        EmojiPickerFocusTarget::Composer => Task::batch([
            iced::widget::operation::focus(COMPOSER_INPUT_ID),
            iced::widget::operation::move_cursor_to_end(COMPOSER_INPUT_ID),
        ]),
        EmojiPickerFocusTarget::Search => Task::none(),
    }
}

fn emoji_picker_keyboard_message(
    event: iced::Event,
    _status: event::Status,
    _window: iced::window::Id,
) -> Option<AppMessage> {
    let iced::Event::Keyboard(keyboard::Event::KeyPressed { modified_key, .. }) = event else {
        return None;
    };

    match modified_key.as_ref() {
        keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
            Some(AppMessage::EmojiPickerNavigate(EmojiPickerNavigation::Next))
        }
        keyboard::Key::Named(keyboard::key::Named::ArrowUp) => Some(
            AppMessage::EmojiPickerNavigate(EmojiPickerNavigation::Previous),
        ),
        keyboard::Key::Named(keyboard::key::Named::Home) => Some(AppMessage::EmojiPickerNavigate(
            EmojiPickerNavigation::First,
        )),
        keyboard::Key::Named(keyboard::key::Named::End) => {
            Some(AppMessage::EmojiPickerNavigate(EmojiPickerNavigation::Last))
        }
        keyboard::Key::Named(keyboard::key::Named::Enter) => {
            Some(AppMessage::EmojiPickerSelectFocused)
        }
        keyboard::Key::Named(keyboard::key::Named::Escape) => {
            Some(AppMessage::CloseEmojiPickerPressed)
        }
        _ => None,
    }
}

fn apply_loaded_preferences(state: &mut AppState, preferences: Preferences) -> Vec<AppEffect> {
    if let Some(session_token) = preferences.session_token.clone() {
        state.begin_session_restore(&preferences);
        vec![AppEffect::RestoreSession(SessionRestoreRequest {
            server_url: preferences.server_url,
            session_token,
        })]
    } else {
        state.apply_preferences(&preferences);
        Vec::new()
    }
}

fn begin_auth(state: &mut AppState, action: AuthAction) -> Vec<AppEffect> {
    if state.signed_out.auth_status.is_submitting() {
        return Vec::new();
    }

    let preferences = match Preferences::with_server_url(state.signed_out.server_url.clone()) {
        Ok(preferences) => preferences,
        Err(error) => {
            let message = error.to_string();
            state.signed_out.server_url_status = ServerUrlStatus::Failed(message.clone());
            state.signed_out.auth_status = AuthStatus::Failed(format!(
                "Enter a valid Hamlet server URL before continuing. {message}"
            ));
            return Vec::new();
        }
    };

    let username = state.signed_out.username.trim().to_string();
    let password = state.signed_out.password.clone();

    if username.is_empty() || password.is_empty() {
        state.signed_out.auth_status =
            AuthStatus::Failed("Username and password are required.".to_string());
        return Vec::new();
    }

    state.signed_out.server_url = preferences.server_url.clone();
    state.signed_out.server_url_status = ServerUrlStatus::Clean;
    state.signed_out.auth_status = AuthStatus::Submitting(action);
    state.signed_out.notice = None;

    vec![AppEffect::Authenticate(AuthRequest {
        action,
        server_url: preferences.server_url,
        username,
        password,
        email: None,
    })]
}

fn complete_auth(state: &mut AppState, session: AuthSession) -> Vec<AppEffect> {
    let server_url = state.signed_out.server_url.clone();
    let voice_preferences = state.signed_out.voice_preferences.clone();
    let preferences = Preferences::with_server_url_session_token_and_voice(
        server_url.clone(),
        session.session_token.clone(),
        voice_preferences.clone(),
    )
    .unwrap_or_else(|_| Preferences::default());

    state.sign_in(session, server_url, voice_preferences);

    let mut effects = vec![AppEffect::SavePreferences(preferences)];
    if let Some(signed_in) = state.signed_in.as_mut() {
        effects.extend(visible_avatar_image_effects(signed_in));
        effects.push(AppEffect::LoadChannels(signed_in.authenticated_request()));
        effects.push(AppEffect::StartRealtime(signed_in.authenticated_request()));
    }

    effects
}

fn complete_session_restore(state: &mut AppState, session: AuthSession) -> Vec<AppEffect> {
    let server_url = state.signed_out.server_url.clone();
    let voice_preferences = state.signed_out.voice_preferences.clone();

    state.sign_in(session, server_url, voice_preferences);

    state
        .signed_in
        .as_mut()
        .map(|signed_in| {
            let mut effects = visible_avatar_image_effects(signed_in);
            effects.push(AppEffect::LoadChannels(signed_in.authenticated_request()));
            effects.push(AppEffect::StartRealtime(signed_in.authenticated_request()));
            effects
        })
        .unwrap_or_default()
}

fn fail_session_restore(state: &mut AppState, error: ApiError) -> Vec<AppEffect> {
    let preferences = Preferences::with_server_url_session_token_and_voice(
        state.signed_out.server_url.clone(),
        None,
        state.signed_out.voice_preferences.clone(),
    )
    .unwrap_or_else(|_| Preferences::default());
    state.return_to_signed_out(
        &preferences,
        Some(format!(
            "Could not restore your saved session. {}",
            error.user_message()
        )),
    );

    vec![AppEffect::SavePreferences(preferences)]
}

fn begin_logout(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.logout_status == LogoutStatus::LoggingOut {
        return Vec::new();
    }

    signed_in.logout_status = LogoutStatus::LoggingOut;
    signed_in.voice_presence.clear();
    let should_stop_voice = signed_in.voice_connection.has_active_connection();
    signed_in.voice_connection.clear();

    let mut effects = vec![AppEffect::StopRealtime];
    if should_stop_voice {
        effects.push(AppEffect::SendVoiceCommand(VoiceCommand::Shutdown));
    }
    effects.push(AppEffect::Logout(LogoutRequest {
        server_url: signed_in.server_url.clone(),
        session_token: signed_in.session_token.clone(),
    }));

    effects
}

fn complete_logout(state: &mut AppState, result: Result<(), ApiError>) -> Vec<AppEffect> {
    let server_url = state
        .signed_in
        .as_ref()
        .map(|signed_in| signed_in.server_url.clone())
        .unwrap_or_else(|| state.signed_out.server_url.clone());
    let voice_preferences = state
        .signed_in
        .as_ref()
        .map(|signed_in| signed_in.voice_settings.preferences())
        .unwrap_or_else(|| state.signed_out.voice_preferences.clone());
    let preferences =
        Preferences::with_server_url_session_token_and_voice(server_url, None, voice_preferences)
            .unwrap_or_else(|_| Preferences::default());
    let notice = match result {
        Ok(()) => Some("Logged out.".to_string()),
        Err(error) => Some(format!(
            "Logged out locally. The server logout did not complete: {}",
            error.user_message()
        )),
    };

    state.return_to_signed_out(&preferences, notice);

    vec![AppEffect::SavePreferences(preferences)]
}

fn open_settings(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if !signed_in.profile_settings.is_open {
        signed_in.profile_settings.sync_from_user(&signed_in.user);
        signed_in.profile_settings.status = ProfileUpdateStatus::Idle;
        signed_in.profile_settings.avatar_status = AvatarUpdateStatus::Idle;
        signed_in.voice_settings.status = VoicePreferenceStatus::Idle;
    }
    signed_in.profile_settings.is_open = true;

    Vec::new()
}

fn close_settings(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.profile_settings.is_open = false;

    if !signed_in.profile_settings.status.is_saving() {
        signed_in.profile_settings.status = ProfileUpdateStatus::Idle;
    }
    if !signed_in.profile_settings.avatar_status.is_busy() {
        signed_in.profile_settings.avatar_status = AvatarUpdateStatus::Idle;
    }
    if !signed_in.voice_settings.status.is_saving() {
        signed_in.voice_settings.status = VoicePreferenceStatus::Idle;
    }

    Vec::new()
}

fn edit_profile_display_name(state: &mut AppState, display_name: String) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.profile_settings.display_name_input = display_name;
    signed_in.profile_settings.clear_feedback();

    Vec::new()
}

fn begin_profile_update(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.profile_settings.status.is_saving() {
        return Vec::new();
    }

    let trimmed = signed_in.profile_settings.display_name_input.trim();
    if trimmed.chars().count() > DISPLAY_NAME_MAX_LEN {
        signed_in.profile_settings.status =
            ProfileUpdateStatus::Failed(display_name_validation_message());
        return Vec::new();
    }

    let display_name = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    };
    signed_in.profile_settings.status = ProfileUpdateStatus::Saving;

    vec![AppEffect::UpdateProfile(
        signed_in.profile_update_request(display_name),
    )]
}

fn begin_profile_clear(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.profile_settings.status.is_saving() {
        return Vec::new();
    }

    signed_in.profile_settings.display_name_input.clear();
    signed_in.profile_settings.status = ProfileUpdateStatus::Saving;

    vec![AppEffect::UpdateProfile(
        signed_in.profile_update_request(None),
    )]
}

fn complete_profile_update(state: &mut AppState, result: Result<User, ApiError>) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    match result {
        Ok(user) => {
            refresh_visible_messages_for_user(&mut signed_in.message_history, &user);
            refresh_visible_voice_participants_for_user(&mut signed_in.voice_presence, &user);
            signed_in.user = user;
            signed_in.profile_settings.sync_from_user(&signed_in.user);
            signed_in.profile_settings.status = ProfileUpdateStatus::Saved;
            visible_avatar_image_effects(signed_in)
        }
        Err(error) => {
            signed_in.profile_settings.status =
                ProfileUpdateStatus::Failed(profile_update_error_message(error));
            Vec::new()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AvatarAction {
    Upload,
    Delete,
}

fn begin_avatar_file_selection(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.profile_settings.avatar_status.is_busy() {
        return Vec::new();
    }

    signed_in.profile_settings.avatar_status = AvatarUpdateStatus::Selecting;

    vec![AppEffect::PickAvatarFile]
}

fn complete_avatar_file_selection(
    state: &mut AppState,
    result: Result<Option<std::path::PathBuf>, String>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    match result {
        Ok(Some(path)) => {
            signed_in.profile_settings.avatar_status = AvatarUpdateStatus::Uploading;
            vec![AppEffect::UploadAvatar(
                signed_in.avatar_upload_request(path),
            )]
        }
        Ok(None) => {
            signed_in.profile_settings.avatar_status = AvatarUpdateStatus::Idle;
            Vec::new()
        }
        Err(message) => {
            signed_in.profile_settings.avatar_status =
                AvatarUpdateStatus::Failed(format!("Could not select an avatar image. {message}"));
            Vec::new()
        }
    }
}

fn begin_avatar_delete(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.profile_settings.avatar_status.is_busy() || signed_in.user.avatar_url.is_none() {
        return Vec::new();
    }

    signed_in.profile_settings.avatar_status = AvatarUpdateStatus::Deleting;

    vec![AppEffect::DeleteAvatar(signed_in.avatar_delete_request())]
}

fn complete_avatar_update(
    state: &mut AppState,
    result: Result<User, ApiError>,
    action: AvatarAction,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    match result {
        Ok(user) => {
            refresh_visible_messages_for_user(&mut signed_in.message_history, &user);
            refresh_visible_voice_participants_for_user(&mut signed_in.voice_presence, &user);
            signed_in.user = user;
            signed_in.profile_settings.sync_from_user(&signed_in.user);
            signed_in.profile_settings.avatar_status = AvatarUpdateStatus::Saved;
            visible_avatar_image_effects(signed_in)
        }
        Err(error) => {
            let action = match action {
                AvatarAction::Upload => "upload avatar",
                AvatarAction::Delete => "remove avatar",
            };
            signed_in.profile_settings.avatar_status =
                AvatarUpdateStatus::Failed(format!("Could not {action}. {}", error.user_message()));
            Vec::new()
        }
    }
}

fn complete_avatar_image_load(
    state: &mut AppState,
    url: String,
    result: Result<Vec<u8>, crate::avatar::AvatarImageError>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.avatar_images.complete_load(url, result);

    Vec::new()
}

fn complete_embed_image_load(
    state: &mut AppState,
    url: String,
    result: Result<Vec<u8>, crate::embeds::EmbedImageError>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.embed_images.complete_load(url, result);

    Vec::new()
}

fn display_name_validation_message() -> String {
    format!("Display name must be {DISPLAY_NAME_MAX_LEN} characters or fewer.")
}

fn profile_update_error_message(error: ApiError) -> String {
    match error {
        ApiError::InvalidRequest(_) => display_name_validation_message(),
        other => format!("Could not update display name. {}", other.user_message()),
    }
}

fn refresh_visible_messages_for_user(history: &mut MessageHistoryState, user: &User) {
    let MessageHistoryState::Loaded { messages, .. } = history else {
        return;
    };

    for message in messages
        .iter_mut()
        .filter(|message| message.user_id == user.id)
    {
        message.username = user.username.clone();
        message.display_name = user.display_name.clone();
        message.avatar_url = user.avatar_url.clone();
    }
}

fn refresh_visible_voice_participants_for_user(
    voice_presence: &mut crate::auth::VoicePresenceState,
    user: &User,
) {
    for participant in voice_presence
        .participants_by_channel
        .values_mut()
        .flat_map(|participants| participants.iter_mut())
        .filter(|participant| participant.user_id == user.id)
    {
        participant.username = user.username.clone();
        participant.avatar_url = user.avatar_url.clone();
    }
}

fn visible_message_image_effects(signed_in: &mut crate::auth::SignedInState) -> Vec<AppEffect> {
    let mut effects = visible_avatar_image_effects(signed_in);
    effects.extend(visible_embed_image_effects(signed_in));
    effects
}

fn visible_avatar_image_effects(signed_in: &mut crate::auth::SignedInState) -> Vec<AppEffect> {
    let server_url = signed_in.server_url.clone();
    let mut urls = Vec::new();

    if let Some(url) = &signed_in.user.avatar_url {
        urls.push(url.clone());
    }

    if let MessageHistoryState::Loaded { messages, .. } = &signed_in.message_history {
        urls.extend(
            messages
                .iter()
                .filter_map(|message| message.avatar_url.clone()),
        );
    }

    urls.extend(
        signed_in
            .voice_presence
            .participants_by_channel
            .values()
            .flat_map(|participants| participants.iter())
            .filter_map(|participant| participant.avatar_url.clone()),
    );

    urls.into_iter()
        .filter_map(|url| signed_in.avatar_images.begin_load(&server_url, Some(&url)))
        .map(AppEffect::LoadAvatarImage)
        .collect()
}

fn visible_embed_image_effects(signed_in: &mut crate::auth::SignedInState) -> Vec<AppEffect> {
    let server_url = signed_in.server_url.clone();
    let mut urls = Vec::new();

    if let MessageHistoryState::Loaded { messages, .. } = &signed_in.message_history {
        urls.extend(
            messages
                .iter()
                .filter(|message| !message.suppress_embeds)
                .flat_map(|message| message.embeds.iter())
                .filter_map(|embed| embed.image_url.clone()),
        );
    }

    urls.into_iter()
        .filter_map(|url| signed_in.embed_images.begin_load(&server_url, Some(&url)))
        .map(AppEffect::LoadEmbedImage)
        .collect()
}

fn edit_voice_microphone_device(state: &mut AppState, device_id: String) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.voice_settings.microphone_device_id_input = device_id;
    signed_in.voice_settings.clear_feedback();

    Vec::new()
}

fn edit_voice_output_device(state: &mut AppState, device_id: String) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.voice_settings.output_device_id_input = device_id;
    signed_in.voice_settings.clear_feedback();

    Vec::new()
}

fn begin_voice_preferences_save(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.voice_settings.status.is_saving() {
        return Vec::new();
    }

    let preferences = Preferences::with_server_url_session_token_and_voice(
        signed_in.server_url.clone(),
        signed_in.session_token.clone(),
        signed_in.voice_settings.preferences(),
    );

    match preferences {
        Ok(preferences) => {
            signed_in.voice_settings.status = VoicePreferenceStatus::Saving;
            vec![AppEffect::SaveVoicePreferences(preferences)]
        }
        Err(error) => {
            signed_in.voice_settings.status = VoicePreferenceStatus::Failed(error.to_string());
            Vec::new()
        }
    }
}

fn complete_voice_preferences_save(
    state: &mut AppState,
    result: Result<Preferences, String>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    match result {
        Ok(preferences) => {
            signed_in
                .voice_settings
                .sync_from_preferences(&preferences.voice);
            signed_in.voice_settings.status = VoicePreferenceStatus::Saved;
            state.signed_out.voice_preferences = preferences.voice;
        }
        Err(message) => {
            signed_in.voice_settings.status = VoicePreferenceStatus::Failed(message);
        }
    }

    Vec::new()
}

fn complete_channels_load(
    state: &mut AppState,
    result: Result<Vec<Channel>, ApiError>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    match result {
        Ok(channels) => {
            let selected_channel_id = signed_in
                .selected_channel_id
                .filter(|id| channels.iter().any(|channel| channel.id == *id))
                .or_else(|| first_text_channel_id(&channels));
            let text_channel_ids = text_channel_ids(&channels);
            let voice_channel_ids = voice_channel_ids(&channels);
            signed_in.channels = ChannelListState::Loaded(channels);
            signed_in.channel_reorder = ChannelReorderState::Idle;
            signed_in.selected_channel_id = selected_channel_id;
            signed_in.message_actions.clear();
            signed_in.typing.retain_channels(&text_channel_ids);
            signed_in.voice_presence.begin_loading(&voice_channel_ids);

            let mut effects = Vec::new();
            if let Some(channel_id) = signed_in.selected_text_channel_id() {
                signed_in.message_history = MessageHistoryState::Loading { channel_id };
                effects.push(AppEffect::LoadMessageHistory(
                    signed_in.message_history_request(channel_id),
                ));
            } else {
                signed_in.message_history = MessageHistoryState::NotLoaded;
            }
            if !voice_channel_ids.is_empty() {
                effects.push(AppEffect::LoadVoiceParticipants(
                    signed_in.voice_participants_request(voice_channel_ids),
                ));
            }

            effects
        }
        Err(error) => {
            signed_in.channels = ChannelListState::Failed(error.user_message());
            signed_in.channel_reorder = ChannelReorderState::Idle;
            signed_in.selected_channel_id = None;
            signed_in.message_history = MessageHistoryState::NotLoaded;
            signed_in.message_actions.clear();
            signed_in.typing.clear();
            signed_in.voice_presence.clear();
            Vec::new()
        }
    }
}

fn select_channel(state: &mut AppState, channel_id: Id) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };
    let Some(channel) = signed_in
        .loaded_channels()
        .iter()
        .find(|channel| channel.id == channel_id)
        .cloned()
    else {
        return Vec::new();
    };

    signed_in.selected_channel_id = Some(channel.id);
    signed_in.message_actions.clear();

    if channel.kind == ChannelKind::Text {
        signed_in.message_history = MessageHistoryState::Loading {
            channel_id: channel.id,
        };
        vec![AppEffect::LoadMessageHistory(
            signed_in.message_history_request(channel.id),
        )]
    } else {
        signed_in.message_history = MessageHistoryState::NotLoaded;
        Vec::new()
    }
}

fn open_create_channel(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.create_channel.is_open = true;

    Vec::new()
}

fn close_create_channel(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.create_channel.status == CreateChannelStatus::Creating {
        return Vec::new();
    }

    signed_in.create_channel.is_open = false;
    signed_in.create_channel.name.clear();
    signed_in.create_channel.status = CreateChannelStatus::Idle;

    Vec::new()
}

fn edit_create_channel_name(state: &mut AppState, name: String) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.create_channel.is_open = true;
    signed_in.create_channel.name = name;
    signed_in.create_channel.clear_failure();

    Vec::new()
}

fn select_create_channel_kind(state: &mut AppState, kind: ChannelKind) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.create_channel.is_open = true;
    signed_in.create_channel.kind = kind;
    signed_in.create_channel.clear_failure();

    Vec::new()
}

fn begin_create_channel(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.create_channel.status == CreateChannelStatus::Creating {
        return Vec::new();
    }

    let name = signed_in.create_channel.name.trim().to_string();
    if name.is_empty() {
        signed_in.create_channel.status =
            CreateChannelStatus::Failed("Channel name is required.".to_string());
        return Vec::new();
    }

    let kind = signed_in.create_channel.kind;
    signed_in.create_channel.status = CreateChannelStatus::Creating;

    vec![AppEffect::CreateChannel(
        signed_in.create_channel_request(name, kind),
    )]
}

fn complete_create_channel(
    state: &mut AppState,
    result: Result<Channel, ApiError>,
) -> Vec<AppEffect> {
    match result {
        Ok(channel) => {
            let channel_id = channel.id;
            let reload_request = {
                let Some(signed_in) = state.signed_in.as_mut() else {
                    return Vec::new();
                };

                signed_in.create_channel.name.clear();
                signed_in.create_channel.is_open = false;
                signed_in.create_channel.status = CreateChannelStatus::Idle;

                if matches!(&signed_in.channels, ChannelListState::Loaded(_)) {
                    upsert_channel(signed_in, channel);
                    None
                } else {
                    signed_in.channels = ChannelListState::Loading;
                    signed_in.selected_channel_id = Some(channel_id);
                    signed_in.message_history = MessageHistoryState::NotLoaded;
                    Some(signed_in.authenticated_request())
                }
            };

            if let Some(request) = reload_request {
                vec![AppEffect::LoadChannels(request)]
            } else {
                select_channel(state, channel_id)
            }
        }
        Err(error) => {
            let Some(signed_in) = state.signed_in.as_mut() else {
                return Vec::new();
            };

            signed_in.create_channel.status =
                CreateChannelStatus::Failed(create_channel_error_message(error));
            Vec::new()
        }
    }
}

fn create_channel_error_message(error: ApiError) -> String {
    match error {
        ApiError::InvalidRequest(_) => {
            "Invalid channel name. Enter a non-empty name up to 128 characters.".to_string()
        }
        other => other.user_message(),
    }
}

fn begin_channel_move(
    state: &mut AppState,
    channel_id: Id,
    direction: ChannelMoveDirection,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.channel_reorder.is_committing() {
        return Vec::new();
    }

    let ChannelListState::Loaded(channels) = &mut signed_in.channels else {
        return Vec::new();
    };
    let Some(current_index) = channels.iter().position(|channel| channel.id == channel_id) else {
        return Vec::new();
    };
    let Some(target_index) = channel_move_target(current_index, channels.len(), direction) else {
        return Vec::new();
    };

    let previous_channels = channels.clone();
    channels.swap(current_index, target_index);
    renumber_channel_positions(channels);
    let ids = channels.iter().map(|channel| channel.id).collect();
    signed_in.channel_reorder = ChannelReorderState::Committing { previous_channels };

    vec![AppEffect::ReorderChannels(
        signed_in.reorder_channels_request(ids),
    )]
}

fn complete_channel_reorder(
    state: &mut AppState,
    result: Result<Vec<Channel>, ApiError>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    match result {
        Ok(channels) => apply_reordered_channels(signed_in, channels),
        Err(error) => {
            if let ChannelReorderState::Committing { previous_channels } =
                &signed_in.channel_reorder
            {
                signed_in.channels = ChannelListState::Loaded(previous_channels.clone());
            }
            signed_in.channel_reorder = ChannelReorderState::Failed(format!(
                "Could not reorder channels. {}",
                channel_reorder_error_message(error)
            ));
        }
    }

    Vec::new()
}

fn channel_reorder_error_message(error: ApiError) -> String {
    match error {
        ApiError::InvalidRequest(_) => {
            "Refresh the channel list and try moving the channel again.".to_string()
        }
        other => other.user_message(),
    }
}

fn channel_move_target(
    current_index: usize,
    len: usize,
    direction: ChannelMoveDirection,
) -> Option<usize> {
    match direction {
        ChannelMoveDirection::Up => current_index.checked_sub(1),
        ChannelMoveDirection::Down => {
            let target = current_index.saturating_add(1);

            (target < len).then_some(target)
        }
    }
}

fn retry_channels(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.channels = ChannelListState::Loading;
    signed_in.channel_reorder = ChannelReorderState::Idle;
    signed_in.message_history = MessageHistoryState::NotLoaded;
    signed_in.message_actions.clear();
    signed_in.voice_presence.clear();

    vec![AppEffect::LoadChannels(signed_in.authenticated_request())]
}

fn complete_message_history_load(
    state: &mut AppState,
    channel_id: Id,
    result: Result<Vec<Message>, ApiError>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.selected_text_channel_id() != Some(channel_id) {
        return Vec::new();
    }

    signed_in.message_history = match result {
        Ok(messages) => MessageHistoryState::Loaded {
            channel_id,
            messages,
        },
        Err(error) => MessageHistoryState::Failed {
            channel_id,
            message: error.user_message(),
        },
    };

    visible_message_image_effects(signed_in)
}

fn retry_message_history(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };
    let Some(channel_id) = signed_in.selected_text_channel_id() else {
        return Vec::new();
    };

    signed_in.message_history = MessageHistoryState::Loading { channel_id };
    signed_in.message_actions.clear();

    vec![AppEffect::LoadMessageHistory(
        signed_in.message_history_request(channel_id),
    )]
}

fn first_text_channel_id(channels: &[Channel]) -> Option<Id> {
    channels
        .iter()
        .find(|channel| channel.kind == ChannelKind::Text)
        .map(|channel| channel.id)
}

fn text_channel_ids(channels: &[Channel]) -> Vec<Id> {
    channels
        .iter()
        .filter(|channel| channel.kind == ChannelKind::Text)
        .map(|channel| channel.id)
        .collect()
}

fn voice_channel_ids(channels: &[Channel]) -> Vec<Id> {
    channels
        .iter()
        .filter(|channel| channel.kind == ChannelKind::Voice)
        .map(|channel| channel.id)
        .collect()
}

fn upsert_channel(signed_in: &mut crate::auth::SignedInState, channel: Channel) {
    let is_voice_channel = channel.kind == ChannelKind::Voice;
    let channel_id = channel.id;

    if let ChannelListState::Loaded(channels) = &mut signed_in.channels {
        upsert_channel_list(channels, channel);
        let text_channel_ids = text_channel_ids(channels);
        signed_in.typing.retain_channels(&text_channel_ids);
    }

    if is_voice_channel {
        signed_in
            .voice_presence
            .participants_by_channel
            .entry(channel_id)
            .or_default();
    }
}

fn apply_reordered_channels(signed_in: &mut crate::auth::SignedInState, channels: Vec<Channel>) {
    let channels = sorted_channels(channels);
    let text_channel_ids = text_channel_ids(&channels);
    signed_in.typing.retain_channels(&text_channel_ids);
    signed_in.channels = ChannelListState::Loaded(channels);
    signed_in.channel_reorder = ChannelReorderState::Idle;
}

fn sorted_channels(mut channels: Vec<Channel>) -> Vec<Channel> {
    channels.sort_by_key(|channel| (channel.position, channel.id));
    channels
}

fn renumber_channel_positions(channels: &mut [Channel]) {
    for (position, channel) in channels.iter_mut().enumerate() {
        channel.position = position as Id;
    }
}

fn upsert_channel_list(channels: &mut Vec<Channel>, channel: Channel) {
    if let Some(existing) = channels
        .iter_mut()
        .find(|existing| existing.id == channel.id)
    {
        *existing = channel;
    } else {
        channels.push(channel);
    }

    channels.sort_by_key(|channel| (channel.position, channel.id));
}

fn edit_draft(state: &mut AppState, draft: String) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.draft = draft;
    signed_in.emoji_picker.focus_target = EmojiPickerFocusTarget::Composer;
    if matches!(signed_in.send_status, SendMessageStatus::Failed(_)) {
        signed_in.send_status = SendMessageStatus::Idle;
    }

    typing_ping_effect_for_current_draft(signed_in)
}

fn advance_typing_timers(state: &mut AppState) -> Vec<AppEffect> {
    if let Some(signed_in) = state.signed_in.as_mut() {
        signed_in.typing.tick();
    }

    Vec::new()
}

fn typing_ping_effect_for_current_draft(
    signed_in: &mut crate::auth::SignedInState,
) -> Vec<AppEffect> {
    if signed_in.draft.trim().is_empty() || !signed_in.typing.can_send_ping() {
        return Vec::new();
    }

    let Some(channel_id) = signed_in.selected_text_channel_id() else {
        return Vec::new();
    };

    signed_in.typing.mark_ping_sent();

    vec![AppEffect::PostTyping(
        signed_in.typing_ping_request(channel_id),
    )]
}

fn toggle_emoji_picker(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.emoji_picker.is_open {
        signed_in.emoji_picker.close();
    } else {
        signed_in.emoji_picker.open();
    }

    Vec::new()
}

fn close_emoji_picker(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.emoji_picker.close();

    Vec::new()
}

fn edit_emoji_search(state: &mut AppState, query: String) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.emoji_picker.is_open {
        signed_in.emoji_picker.edit_query(query);
    }

    Vec::new()
}

fn navigate_emoji_picker(
    state: &mut AppState,
    navigation: EmojiPickerNavigation,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.emoji_picker.is_open {
        signed_in.emoji_picker.navigate(navigation);
    }

    Vec::new()
}

fn select_focused_emoji(state: &mut AppState) -> Vec<AppEffect> {
    let Some(emoji) = state
        .signed_in
        .as_ref()
        .filter(|signed_in| signed_in.emoji_picker.is_open)
        .and_then(|signed_in| signed_in.emoji_picker.selected_choice())
        .map(|choice| choice.symbol.to_string())
    else {
        return Vec::new();
    };

    insert_selected_emoji(state, &emoji)
}

fn insert_selected_emoji(state: &mut AppState, emoji: &str) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if !signed_in.emoji_picker.is_open {
        return Vec::new();
    }

    signed_in.draft.push_str(emoji);
    signed_in.emoji_picker.close();
    if matches!(signed_in.send_status, SendMessageStatus::Failed(_)) {
        signed_in.send_status = SendMessageStatus::Idle;
    }

    typing_ping_effect_for_current_draft(signed_in)
}

fn begin_send_message(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.send_status == SendMessageStatus::Sending {
        return Vec::new();
    }

    let Some(channel_id) = signed_in.selected_text_channel_id() else {
        return Vec::new();
    };
    let text = signed_in.draft.trim().to_string();

    if text.is_empty() {
        return Vec::new();
    }

    signed_in.send_status = SendMessageStatus::Sending;
    signed_in.typing.reset_outgoing();

    vec![AppEffect::SendMessage(
        signed_in.send_message_request(channel_id, text),
    )]
}

fn complete_send_message(
    state: &mut AppState,
    channel_id: Id,
    result: Result<Message, ApiError>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    match result {
        Ok(message) => {
            signed_in.draft.clear();
            signed_in.send_status = SendMessageStatus::Idle;
            append_message_to_history(signed_in, channel_id, message);
            visible_message_image_effects(signed_in)
        }
        Err(error) => {
            signed_in.send_status = SendMessageStatus::Failed(error.user_message());
            Vec::new()
        }
    }
}

fn begin_message_edit(state: &mut AppState, message_id: Id) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };
    let Some(message) = visible_message(signed_in, message_id).cloned() else {
        return Vec::new();
    };

    if !signed_in.can_manage_message(&message) {
        return Vec::new();
    }

    signed_in.message_actions.editing = Some(MessageEditState {
        message_id: message.id,
        channel_id: message.channel_id,
        draft: message.text,
        status: MessageEditStatus::Editing,
    });

    Vec::new()
}

fn edit_message_draft(state: &mut AppState, draft: String) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };
    let Some(editing) = signed_in.message_actions.editing.as_mut() else {
        return Vec::new();
    };

    editing.draft = draft;
    if matches!(editing.status, MessageEditStatus::Failed(_)) {
        editing.status = MessageEditStatus::Editing;
    }

    Vec::new()
}

fn begin_message_update(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    let Some((message_id, channel_id, text)) = pending_edit_request_parts(signed_in) else {
        return Vec::new();
    };

    let is_unchanged = visible_message(signed_in, message_id)
        .is_some_and(|message| message.channel_id == channel_id && message.text == text);

    if is_unchanged {
        signed_in.message_actions.editing = None;
        return Vec::new();
    }

    if let Some(editing) = signed_in.message_actions.editing.as_mut() {
        editing.status = MessageEditStatus::Saving;
    }

    vec![AppEffect::EditMessage(
        signed_in.edit_message_request(message_id, channel_id, text),
    )]
}

fn pending_edit_request_parts(
    signed_in: &mut crate::auth::SignedInState,
) -> Option<(Id, Id, String)> {
    let editing = signed_in.message_actions.editing.as_mut()?;

    if editing.status.is_saving() {
        return None;
    }

    let text = editing.draft.trim().to_string();
    if text.is_empty() {
        editing.status = MessageEditStatus::Failed("Message text is required.".to_string());
        return None;
    }

    Some((editing.message_id, editing.channel_id, text))
}

fn cancel_message_edit(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.message_actions.editing = None;

    Vec::new()
}

fn complete_message_update(
    state: &mut AppState,
    message_id: Id,
    channel_id: Id,
    result: Result<Message, ApiError>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };
    let matches_current_edit = signed_in
        .message_actions
        .editing
        .as_ref()
        .is_some_and(|editing| {
            editing.message_id == message_id && editing.channel_id == channel_id
        });

    match result {
        Ok(message) => {
            replace_message_in_history(signed_in, message);
            if matches_current_edit {
                signed_in.message_actions.editing = None;
            }
            visible_message_image_effects(signed_in)
        }
        Err(error) => {
            if matches_current_edit
                && let Some(editing) = signed_in.message_actions.editing.as_mut()
            {
                editing.status = MessageEditStatus::Failed(format!(
                    "Could not edit message. {}",
                    error.user_message()
                ));
            }
            Vec::new()
        }
    }
}

fn begin_message_delete(state: &mut AppState, message_id: Id) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.message_actions.delete_status.is_deleting() {
        return Vec::new();
    }

    let Some(message) = visible_message(signed_in, message_id).cloned() else {
        return Vec::new();
    };

    if !signed_in.can_manage_message(&message) {
        return Vec::new();
    }

    signed_in.message_actions.delete_status = MessageDeleteStatus::Deleting {
        message_id: message.id,
        channel_id: message.channel_id,
    };

    vec![AppEffect::DeleteMessage(
        signed_in.delete_message_request(message.id, message.channel_id),
    )]
}

fn complete_message_delete(
    state: &mut AppState,
    message_id: Id,
    channel_id: Id,
    result: Result<(), ApiError>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };
    let matches_current_delete = signed_in
        .message_actions
        .delete_status
        .matches_deleting(message_id, channel_id);

    match result {
        Ok(()) => {
            remove_message_from_history(signed_in, channel_id, message_id);
            signed_in.message_actions.clear_for_message(message_id);
        }
        Err(error) => {
            if matches_current_delete {
                signed_in.message_actions.delete_status = MessageDeleteStatus::Failed {
                    message_id,
                    channel_id,
                    message: format!("Could not delete message. {}", error.user_message()),
                };
            }
        }
    }

    Vec::new()
}

fn begin_suppress_embeds(state: &mut AppState, message_id: Id) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.message_actions.suppress_status.is_suppressing() {
        return Vec::new();
    }

    let Some(message) = visible_message(signed_in, message_id).cloned() else {
        return Vec::new();
    };

    if !signed_in
        .message_action_visibility(&message)
        .can_suppress_embeds
    {
        return Vec::new();
    }

    signed_in.message_actions.suppress_status = MessageSuppressStatus::Suppressing {
        message_id: message.id,
        channel_id: message.channel_id,
    };

    vec![AppEffect::SuppressMessageEmbeds(
        signed_in.suppress_embeds_request(message.id, message.channel_id, true),
    )]
}

fn complete_suppress_embeds(
    state: &mut AppState,
    message_id: Id,
    channel_id: Id,
    result: Result<MessageEmbedsUpdatedEvent, ApiError>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };
    let matches_current_suppress = signed_in
        .message_actions
        .suppress_status
        .matches_suppressing(message_id, channel_id);

    match result {
        Ok(update) => {
            apply_message_embeds_update(signed_in, update);
            if matches_current_suppress {
                signed_in
                    .message_actions
                    .clear_suppress_for_message(message_id);
            }
            visible_message_image_effects(signed_in)
        }
        Err(error) => {
            if matches_current_suppress {
                signed_in.message_actions.suppress_status = MessageSuppressStatus::Failed {
                    message_id,
                    channel_id,
                    message: format!("Could not suppress embeds. {}", error.user_message()),
                };
            }
            Vec::new()
        }
    }
}

fn begin_external_url_open(state: &mut AppState, url: String) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    let validated = match validate_external_url(&url) {
        Ok(validated) => validated,
        Err(error) => {
            signed_in.external_link_status = ExternalLinkStatus::Failed(error.user_message(&url));
            return Vec::new();
        }
    };
    let url = validated.as_str().to_string();

    signed_in.external_link_status = ExternalLinkStatus::Opening { url: url.clone() };

    vec![AppEffect::OpenExternalUrl(url)]
}

fn complete_external_url_open(
    state: &mut AppState,
    url: String,
    result: Result<(), ExternalOpenError>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.external_link_status = match result {
        Ok(()) => ExternalLinkStatus::Idle,
        Err(error) => ExternalLinkStatus::Failed(error.user_message(&url)),
    };

    Vec::new()
}

fn append_message_to_history(
    signed_in: &mut crate::auth::SignedInState,
    channel_id: Id,
    message: Message,
) {
    if signed_in.selected_text_channel_id() != Some(channel_id) {
        return;
    }

    match &mut signed_in.message_history {
        MessageHistoryState::Loaded {
            channel_id: history_channel_id,
            messages,
        } if *history_channel_id == channel_id => {
            if let Some(existing) = messages
                .iter_mut()
                .find(|existing| existing.id == message.id)
            {
                *existing = message;
            } else {
                messages.push(message);
            }
        }
        MessageHistoryState::NotLoaded
        | MessageHistoryState::Loading { .. }
        | MessageHistoryState::Loaded { .. }
        | MessageHistoryState::Failed { .. } => {}
    }
}

fn replace_message_in_history(signed_in: &mut crate::auth::SignedInState, message: Message) {
    if signed_in.selected_text_channel_id() != Some(message.channel_id) {
        return;
    }

    let MessageHistoryState::Loaded {
        channel_id,
        messages,
    } = &mut signed_in.message_history
    else {
        return;
    };

    if *channel_id != message.channel_id {
        return;
    }

    if let Some(existing) = messages
        .iter_mut()
        .find(|existing| existing.id == message.id)
    {
        *existing = message;
    }
}

fn remove_message_from_history(
    signed_in: &mut crate::auth::SignedInState,
    channel_id: Id,
    message_id: Id,
) {
    if signed_in.selected_text_channel_id() != Some(channel_id) {
        return;
    }

    let MessageHistoryState::Loaded {
        channel_id: history_channel_id,
        messages,
    } = &mut signed_in.message_history
    else {
        return;
    };

    if *history_channel_id != channel_id {
        return;
    }

    messages.retain(|message| message.id != message_id);
}

fn apply_message_embeds_update(
    signed_in: &mut crate::auth::SignedInState,
    update: MessageEmbedsUpdatedEvent,
) {
    if signed_in.selected_text_channel_id() != Some(update.channel_id) {
        return;
    }

    let MessageHistoryState::Loaded {
        channel_id: history_channel_id,
        messages,
    } = &mut signed_in.message_history
    else {
        return;
    };

    if *history_channel_id != update.channel_id {
        return;
    }

    if let Some(existing) = messages
        .iter_mut()
        .find(|existing| existing.id == update.id)
    {
        existing.suppress_embeds = update.suppress_embeds;
        existing.embeds = update.embeds;
        signed_in
            .message_actions
            .clear_suppress_for_message(existing.id);
    }
}

fn visible_message(signed_in: &crate::auth::SignedInState, message_id: Id) -> Option<&Message> {
    let MessageHistoryState::Loaded { messages, .. } = &signed_in.message_history else {
        return None;
    };

    messages.iter().find(|message| message.id == message_id)
}

fn complete_voice_participants_load(
    state: &mut AppState,
    result: Result<Vec<VoiceParticipant>, ApiError>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    match result {
        Ok(participants) => {
            let channel_ids = voice_channel_ids(signed_in.loaded_channels());
            signed_in
                .voice_presence
                .apply_snapshot(&channel_ids, participants);
            visible_avatar_image_effects(signed_in)
        }
        Err(error) => {
            signed_in.voice_presence.fail(error.user_message());
            Vec::new()
        }
    }
}

fn begin_voice_join(state: &mut AppState, channel_id: Id) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };
    let Some(channel) = signed_in
        .loaded_channels()
        .iter()
        .find(|channel| channel.id == channel_id)
    else {
        return Vec::new();
    };

    if channel.kind != ChannelKind::Voice {
        return Vec::new();
    }
    if signed_in.voice_connection.is_connecting_to(channel_id)
        || signed_in.voice_connection.is_connected_to(channel_id)
    {
        return Vec::new();
    }

    let current_voice_channel_id = signed_in.voice_connection.target_channel_id();
    let should_leave_current =
        current_voice_channel_id.is_some_and(|current_id| current_id != channel_id);
    if should_leave_current && let Some(current_id) = current_voice_channel_id {
        signed_in
            .voice_presence
            .clear_speaking_for_channel(current_id);
    }
    signed_in.voice_connection.begin_connecting(channel_id);

    let mut effects = Vec::new();
    if should_leave_current {
        effects.push(AppEffect::SendVoiceCommand(VoiceCommand::Leave));
    }
    effects.push(AppEffect::LoadVoiceToken(
        signed_in.voice_token_request(channel_id),
    ));

    effects
}

fn begin_voice_leave(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    match signed_in.voice_connection.status {
        VoiceConnectionStatus::Idle => {
            signed_in.voice_connection.clear();
            Vec::new()
        }
        VoiceConnectionStatus::Connecting { channel_id } => {
            signed_in
                .voice_presence
                .clear_speaking_for_channel(channel_id);
            signed_in.voice_connection.clear();
            vec![AppEffect::SendVoiceCommand(VoiceCommand::Leave)]
        }
        VoiceConnectionStatus::Connected { channel_id, .. } => {
            signed_in
                .voice_presence
                .clear_speaking_for_channel(channel_id);
            signed_in.voice_connection.begin_disconnecting();
            vec![AppEffect::SendVoiceCommand(VoiceCommand::Leave)]
        }
        VoiceConnectionStatus::Disconnecting { .. } => Vec::new(),
    }
}

fn begin_voice_mute(state: &mut AppState) -> Vec<AppEffect> {
    begin_voice_control(state, true, VoiceCommand::Mute)
}

fn begin_voice_unmute(state: &mut AppState) -> Vec<AppEffect> {
    begin_voice_control(state, false, VoiceCommand::Unmute)
}

fn begin_voice_deafen(state: &mut AppState) -> Vec<AppEffect> {
    begin_voice_deafen_control(state, true, VoiceCommand::Deafen)
}

fn begin_voice_undeafen(state: &mut AppState) -> Vec<AppEffect> {
    begin_voice_deafen_control(state, false, VoiceCommand::Undeafen)
}

fn begin_voice_control(state: &mut AppState, muted: bool, command: VoiceCommand) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.voice_connection.connected_channel_id().is_none()
        || signed_in.voice_connection.muted == muted
    {
        return Vec::new();
    }

    signed_in.voice_connection.set_muted(muted);
    vec![AppEffect::SendVoiceCommand(command)]
}

fn begin_voice_deafen_control(
    state: &mut AppState,
    deafened: bool,
    command: VoiceCommand,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if signed_in.voice_connection.connected_channel_id().is_none()
        || signed_in.voice_connection.deafened == deafened
    {
        return Vec::new();
    }

    signed_in.voice_connection.set_deafened(deafened);
    vec![AppEffect::SendVoiceCommand(command)]
}

fn complete_voice_token_load(
    state: &mut AppState,
    channel_id: Id,
    result: Result<VoiceToken, ApiError>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if !signed_in.voice_connection.is_connecting_to(channel_id) {
        return Vec::new();
    }

    match result {
        Ok(token) => vec![AppEffect::SendVoiceCommand(VoiceCommand::Join(
            VoiceJoinRequest::from_token_with_device_preferences(
                channel_id,
                token,
                signed_in.voice_settings.preferences(),
            ),
        ))],
        Err(error) => {
            signed_in
                .voice_connection
                .fail(Some(channel_id), voice_token_error_message(error));
            Vec::new()
        }
    }
}

fn apply_voice_worker_event(state: &mut AppState, event: VoiceEvent) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };
    let mut effects = Vec::new();

    match event {
        VoiceEvent::CommandAccepted => {}
        VoiceEvent::Connecting { channel_id } => {
            if signed_in
                .voice_connection
                .target_channel_id()
                .is_none_or(|target_id| target_id == channel_id)
                && !matches!(signed_in.voice_connection.status, VoiceConnectionStatus::Connecting { channel_id: id } if id == channel_id)
            {
                signed_in.voice_connection.begin_connecting(channel_id);
            }
        }
        VoiceEvent::Connected { channel_id, room } => {
            if signed_in.voice_connection.is_connecting_to(channel_id)
                || signed_in.voice_connection.is_connected_to(channel_id)
            {
                signed_in.voice_connection.set_connected(channel_id, room);
            }
        }
        VoiceEvent::Muted { channel_id } => {
            if signed_in.voice_connection.is_connected_to(channel_id) {
                signed_in.voice_connection.set_muted(true);
            }
        }
        VoiceEvent::Unmuted { channel_id } => {
            if signed_in.voice_connection.is_connected_to(channel_id) {
                signed_in.voice_connection.set_muted(false);
            }
        }
        VoiceEvent::Deafened { channel_id } => {
            if signed_in.voice_connection.is_connected_to(channel_id) {
                signed_in.voice_connection.set_deafened(true);
            }
        }
        VoiceEvent::Undeafened { channel_id } => {
            if signed_in.voice_connection.is_connected_to(channel_id) {
                signed_in.voice_connection.set_deafened(false);
            }
        }
        VoiceEvent::SpeakingChanged {
            channel_id,
            user_id,
            speaking,
        } => {
            signed_in
                .voice_presence
                .set_speaking(channel_id, user_id, speaking);
            if user_id == signed_in.user.id
                && signed_in.voice_connection.target_channel_id() == Some(channel_id)
            {
                effects.push(AppEffect::PostVoiceSpeaking(
                    signed_in.voice_speaking_request(channel_id, speaking),
                ));
            }
        }
        VoiceEvent::Disconnected { channel_id, reason } => {
            let matches_current = channel_id.is_none()
                || channel_id == signed_in.voice_connection.target_channel_id();
            if matches_current {
                if let Some(channel_id) =
                    channel_id.or_else(|| signed_in.voice_connection.target_channel_id())
                {
                    signed_in
                        .voice_presence
                        .clear_speaking_for_channel(channel_id);
                }
                signed_in.voice_connection.set_idle();
                if let Some(reason) = reason.filter(|reason| !reason.trim().is_empty()) {
                    signed_in.voice_connection.error = Some(crate::auth::VoiceConnectionError {
                        channel_id,
                        message: reason,
                    });
                }
            }
        }
        VoiceEvent::Reconnecting { channel_id } => {
            if signed_in.voice_connection.is_connected_to(channel_id) {
                signed_in.voice_connection.error = None;
            }
        }
        VoiceEvent::Reconnected { channel_id } => {
            if signed_in.voice_connection.target_channel_id() == Some(channel_id) {
                signed_in.voice_connection.error = None;
            }
        }
        VoiceEvent::Error(error) => {
            let channel_id = error.channel_id;
            let matches_current = channel_id.is_none()
                || channel_id == signed_in.voice_connection.target_channel_id();
            if matches_current {
                if let Some(channel_id) =
                    channel_id.or_else(|| signed_in.voice_connection.target_channel_id())
                {
                    signed_in
                        .voice_presence
                        .clear_speaking_for_channel(channel_id);
                }
                signed_in
                    .voice_connection
                    .fail(channel_id, error.user_message());
            }
        }
    }

    effects
}

fn voice_token_error_message(error: ApiError) -> String {
    match error {
        ApiError::Unauthorized => "Your session expired. Please log in again.".to_string(),
        ApiError::Server { status: 503, .. } => {
            "Voice is not configured on this Hamlet server. Start the LiveKit dev stack and retry."
                .to_string()
        }
        other => format!("Could not get a voice token. {}", other.user_message()),
    }
}

fn complete_realtime_start(
    state: &mut AppState,
    result: Result<(), RealtimeError>,
) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    signed_in.realtime_status = match result {
        Ok(()) => RealtimeConnectionState::Connected,
        Err(_) => reconnect_state(1),
    };

    Vec::new()
}

fn complete_realtime_stop(
    state: &mut AppState,
    _result: Result<(), RealtimeError>,
) -> Vec<AppEffect> {
    if let Some(signed_in) = state.signed_in.as_mut() {
        signed_in.realtime_status = RealtimeConnectionState::Disconnected;
    }

    Vec::new()
}

fn apply_realtime_events(state: &mut AppState, events: Vec<RealtimeEvent>) -> Vec<AppEffect> {
    let mut effects = Vec::new();

    for event in events {
        match event {
            RealtimeEvent::Connected => {
                if let Some(signed_in) = state.signed_in.as_mut() {
                    signed_in.realtime_status = RealtimeConnectionState::Connected;
                }
            }
            RealtimeEvent::Disconnected | RealtimeEvent::Malformed(_) => {
                if let Some(signed_in) = state.signed_in.as_mut() {
                    let attempt = match signed_in.realtime_status {
                        RealtimeConnectionState::BackingOff { attempt, .. } => attempt + 1,
                        RealtimeConnectionState::Disconnected
                        | RealtimeConnectionState::Connecting
                        | RealtimeConnectionState::Connected
                        | RealtimeConnectionState::AuthExpired => 1,
                    };
                    signed_in.realtime_status = reconnect_state(attempt);
                }
            }
            RealtimeEvent::Broadcast(BroadcastEvent::Message(message)) => {
                if let Some(signed_in) = state.signed_in.as_mut() {
                    let channel_id = message.channel_id;
                    let user_id = message.user_id;
                    append_message_to_history(signed_in, channel_id, message);
                    signed_in.typing.clear_user(channel_id, user_id);
                }
            }
            RealtimeEvent::Broadcast(BroadcastEvent::MessageUpdated(message)) => {
                if let Some(signed_in) = state.signed_in.as_mut() {
                    replace_message_in_history(signed_in, message);
                }
            }
            RealtimeEvent::Broadcast(BroadcastEvent::MessageDeleted(deleted)) => {
                if let Some(signed_in) = state.signed_in.as_mut() {
                    remove_message_from_history(signed_in, deleted.channel_id, deleted.id);
                    signed_in.message_actions.clear_for_message(deleted.id);
                }
            }
            RealtimeEvent::Broadcast(BroadcastEvent::MessageEmbedsUpdated(update)) => {
                if let Some(signed_in) = state.signed_in.as_mut() {
                    apply_message_embeds_update(signed_in, update);
                }
            }
            RealtimeEvent::Broadcast(BroadcastEvent::ChannelCreated(channel)) => {
                if let Some(signed_in) = state.signed_in.as_mut() {
                    upsert_channel(signed_in, channel);
                }
            }
            RealtimeEvent::Broadcast(BroadcastEvent::ChannelsReordered(channels)) => {
                if let Some(signed_in) = state.signed_in.as_mut() {
                    apply_reordered_channels(signed_in, channels);
                }
            }
            RealtimeEvent::Broadcast(BroadcastEvent::VoiceParticipantJoined(participant)) => {
                if let Some(signed_in) = state.signed_in.as_mut() {
                    signed_in.voice_presence.upsert_participant(participant);
                }
            }
            RealtimeEvent::Broadcast(BroadcastEvent::VoiceParticipantLeft(participant)) => {
                if let Some(signed_in) = state.signed_in.as_mut() {
                    signed_in
                        .voice_presence
                        .remove_participant(participant.channel_id, participant.user_id);
                }
            }
            RealtimeEvent::Broadcast(BroadcastEvent::VoiceParticipantSpeakingChanged(speaking)) => {
                if let Some(signed_in) = state.signed_in.as_mut() {
                    signed_in.voice_presence.set_speaking(
                        speaking.channel_id,
                        speaking.user_id,
                        speaking.speaking,
                    );
                }
            }
            RealtimeEvent::Broadcast(BroadcastEvent::UserTyping(typing)) => {
                if let Some(signed_in) = state.signed_in.as_mut() {
                    apply_user_typing_event(signed_in, typing);
                }
            }
            RealtimeEvent::AuthExpired => {
                effects.extend(expire_session_from_realtime(state));
            }
        }
    }

    if let Some(signed_in) = state.signed_in.as_mut() {
        effects.extend(visible_message_image_effects(signed_in));
    }

    effects
}

fn apply_user_typing_event(signed_in: &mut crate::auth::SignedInState, event: UserTypingEvent) {
    if event.user_id == signed_in.user.id || !signed_in.is_text_channel(event.channel_id) {
        return;
    }

    signed_in.typing.note_user_typing(event);
}

fn reconnect_realtime(state: &mut AppState) -> Vec<AppEffect> {
    let Some(signed_in) = state.signed_in.as_mut() else {
        return Vec::new();
    };

    if matches!(
        signed_in.realtime_status,
        RealtimeConnectionState::BackingOff { .. }
    ) {
        signed_in.realtime_status = RealtimeConnectionState::Connecting;
        vec![AppEffect::StartRealtime(signed_in.authenticated_request())]
    } else {
        Vec::new()
    }
}

fn expire_session_from_realtime(state: &mut AppState) -> Vec<AppEffect> {
    let server_url = state
        .signed_in
        .as_ref()
        .map(|signed_in| signed_in.server_url.clone())
        .unwrap_or_else(|| state.signed_out.server_url.clone());
    let voice_preferences = state
        .signed_in
        .as_ref()
        .map(|signed_in| signed_in.voice_settings.preferences())
        .unwrap_or_else(|| state.signed_out.voice_preferences.clone());
    let preferences =
        Preferences::with_server_url_session_token_and_voice(server_url, None, voice_preferences)
            .unwrap_or_else(|_| Preferences::default());

    let should_stop_voice = state
        .signed_in
        .as_ref()
        .is_some_and(|signed_in| signed_in.voice_connection.has_active_connection());

    if let Some(signed_in) = state.signed_in.as_mut() {
        signed_in.realtime_status = RealtimeConnectionState::AuthExpired;
        signed_in.voice_connection.clear();
        signed_in.voice_presence.clear();
    }

    state.return_to_signed_out(
        &preferences,
        Some("Your session expired. Please log in again.".to_string()),
    );

    let mut effects = vec![AppEffect::StopRealtime];
    if should_stop_voice {
        effects.push(AppEffect::SendVoiceCommand(VoiceCommand::Shutdown));
    }
    effects.push(AppEffect::SavePreferences(preferences));

    effects
}

fn reconnect_state(attempt: u32) -> RealtimeConnectionState {
    RealtimeConnectionState::BackingOff {
        attempt,
        delay_ms: ReconnectPolicy::default().delay_for_attempt(attempt),
    }
}

fn use_dev_credentials(state: &mut AppState) -> Vec<AppEffect> {
    #[cfg(debug_assertions)]
    {
        state.signed_out.username = "baipas".to_string();
        state.signed_out.password = "password".to_string();
        begin_auth(state, AuthAction::Login)
    }

    #[cfg(not(debug_assertions))]
    {
        let _ = state;
        Vec::new()
    }
}

fn effect_to_task(effect: AppEffect) -> Task<AppMessage> {
    match effect {
        AppEffect::LoadPreferences => {
            Task::perform(load_preferences(), AppMessage::PreferencesLoaded)
        }
        AppEffect::SavePreferences(preferences) => {
            Task::perform(save_preferences(preferences), AppMessage::ServerUrlSaved)
        }
        AppEffect::Authenticate(request) => {
            Task::perform(authenticate(request), AppMessage::AuthCompleted)
        }
        AppEffect::RestoreSession(request) => Task::perform(
            restore_session(request),
            AppMessage::SessionRestoreCompleted,
        ),
        AppEffect::Logout(request) => Task::perform(logout(request), AppMessage::LogoutCompleted),
        AppEffect::LoadChannels(request) => {
            Task::perform(load_channels(request), AppMessage::ChannelsLoaded)
        }
        AppEffect::CreateChannel(request) => {
            Task::perform(create_channel(request), AppMessage::ChannelCreated)
        }
        AppEffect::ReorderChannels(request) => Task::perform(
            reorder_channels(request),
            AppMessage::ChannelReorderCompleted,
        ),
        AppEffect::UpdateProfile(request) => {
            Task::perform(update_profile(request), AppMessage::ProfileUpdated)
        }
        AppEffect::PickAvatarFile => {
            Task::perform(pick_avatar_file(), AppMessage::AvatarFileSelected)
        }
        AppEffect::UploadAvatar(request) => {
            Task::perform(upload_avatar(request), AppMessage::AvatarUploaded)
        }
        AppEffect::DeleteAvatar(request) => {
            Task::perform(delete_avatar(request), AppMessage::AvatarDeleted)
        }
        AppEffect::LoadAvatarImage(request) => {
            Task::perform(load_avatar_image(request), |(url, result)| {
                AppMessage::AvatarImageLoaded { url, result }
            })
        }
        AppEffect::LoadEmbedImage(request) => {
            Task::perform(load_embed_image(request), |(url, result)| {
                AppMessage::EmbedImageLoaded { url, result }
            })
        }
        AppEffect::LoadMessageHistory(request) => {
            Task::perform(load_message_history(request), |(channel_id, result)| {
                AppMessage::MessageHistoryLoaded { channel_id, result }
            })
        }
        AppEffect::SendMessage(request) => {
            Task::perform(send_message(request), |(channel_id, result)| {
                AppMessage::MessageSent { channel_id, result }
            })
        }
        AppEffect::PostTyping(request) => {
            Task::perform(post_typing(request), AppMessage::TypingPingPosted)
        }
        AppEffect::EditMessage(request) => {
            Task::perform(edit_message(request), |(message_id, channel_id, result)| {
                AppMessage::MessageEdited {
                    message_id,
                    channel_id,
                    result,
                }
            })
        }
        AppEffect::DeleteMessage(request) => Task::perform(
            delete_message(request),
            |(message_id, channel_id, result)| AppMessage::MessageDeleted {
                message_id,
                channel_id,
                result,
            },
        ),
        AppEffect::SuppressMessageEmbeds(request) => Task::perform(
            suppress_message_embeds(request),
            |(message_id, channel_id, result)| AppMessage::EmbedsSuppressed {
                message_id,
                channel_id,
                result,
            },
        ),
        AppEffect::LoadVoiceParticipants(request) => Task::perform(
            load_voice_participants(request),
            AppMessage::VoiceParticipantsLoaded,
        ),
        AppEffect::LoadVoiceToken(request) => {
            Task::perform(load_voice_token(request), |(channel_id, result)| {
                AppMessage::VoiceTokenLoaded { channel_id, result }
            })
        }
        AppEffect::SaveVoicePreferences(preferences) => Task::perform(
            save_preferences(preferences),
            AppMessage::VoicePreferencesSaved,
        ),
        AppEffect::PostVoiceSpeaking(request) => Task::perform(
            post_voice_speaking(request),
            AppMessage::VoiceSpeakingPosted,
        ),
        AppEffect::OpenExternalUrl(url) => {
            Task::perform(open_external_url(url), |(url, result)| {
                AppMessage::ExternalUrlOpened { url, result }
            })
        }
        AppEffect::SendVoiceCommand(command) => Task::perform(
            crate::voice::send_runtime_command(command),
            AppMessage::VoiceWorkerEvent,
        ),
        AppEffect::StartRealtime(request) => {
            Task::perform(start_realtime(request), AppMessage::RealtimeStarted)
        }
        AppEffect::StopRealtime => Task::perform(stop_realtime(), AppMessage::RealtimeStopped),
    }
}

// Iced executes Task futures on Tokio. The runtime transport still uses
// reqwest::blocking, whose client owns a Tokio runtime internally and panics if
// it is dropped from an async worker. Keep those calls and drops on Tokio's
// blocking pool instead.
async fn run_blocking_api<T>(
    operation: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .unwrap_or_else(|error| {
            Err(ApiError::TransportSetup(format!(
                "background task failed: {error}"
            )))
        })
}

async fn run_blocking_string<T>(
    operation: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .unwrap_or_else(|error| Err(format!("background task failed: {error}")))
}

async fn load_preferences() -> Result<Preferences, String> {
    run_blocking_string(|| {
        let storage = FileStorage::new().map_err(|error| error.to_string())?;

        storage
            .load_preferences()
            .map_err(|error| error.to_string())
    })
    .await
}

async fn save_preferences(preferences: Preferences) -> Result<Preferences, String> {
    run_blocking_string(move || {
        let storage = FileStorage::new().map_err(|error| error.to_string())?;

        storage
            .save_preferences(&preferences)
            .map(|()| preferences)
            .map_err(|error| error.to_string())
    })
    .await
}

async fn authenticate(request: AuthRequest) -> Result<AuthSession, ApiError> {
    run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url.clone())?;

        match request.action {
            AuthAction::Login => api.login(request.username, request.password),
            AuthAction::Register => api.register(request.username, request.password, request.email),
        }
    })
    .await
}

async fn restore_session(request: SessionRestoreRequest) -> Result<AuthSession, ApiError> {
    run_blocking_api(move || {
        let api = runtime_api()?;
        let session_token = request.session_token;
        api.set_base_url(request.server_url)?;
        api.set_session_token(Some(session_token.clone()))?;
        api.get_me()
            .map(|user| AuthSession::new(user, Some(session_token)))
    })
    .await
}

async fn logout(request: LogoutRequest) -> Result<(), ApiError> {
    run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.logout()
    })
    .await
}

async fn load_channels(
    request: crate::auth::AuthenticatedRequest,
) -> Result<Vec<Channel>, ApiError> {
    run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.list_channels()
    })
    .await
}

async fn create_channel(request: crate::auth::CreateChannelRequest) -> Result<Channel, ApiError> {
    run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.create_channel(request.name, request.kind)
    })
    .await
}

async fn reorder_channels(
    request: crate::auth::ReorderChannelsRequest,
) -> Result<Vec<Channel>, ApiError> {
    run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.reorder_channels(request.ids)
    })
    .await
}

async fn update_profile(request: crate::auth::ProfileUpdateRequest) -> Result<User, ApiError> {
    run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.update_profile(request.display_name)
    })
    .await
}

async fn pick_avatar_file() -> Result<Option<std::path::PathBuf>, String> {
    run_blocking_string(|| {
        Ok(rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .pick_file())
    })
    .await
}

async fn upload_avatar(request: crate::auth::AvatarUploadRequest) -> Result<User, ApiError> {
    run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.upload_avatar(request.path)
    })
    .await
}

async fn delete_avatar(request: crate::auth::AvatarDeleteRequest) -> Result<User, ApiError> {
    run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.delete_avatar()
    })
    .await
}

async fn load_avatar_image(
    request: AvatarFetchRequest,
) -> (String, Result<Vec<u8>, crate::avatar::AvatarImageError>) {
    let url = request.url.clone();

    tokio::task::spawn_blocking(move || fetch_avatar_image(request.url))
        .await
        .unwrap_or_else(|error| {
            (
                url.clone(),
                Err(crate::avatar::AvatarImageError::Unreachable {
                    url,
                    message: format!("avatar image task failed: {error}"),
                }),
            )
        })
}

async fn load_embed_image(
    request: EmbedImageFetchRequest,
) -> (String, Result<Vec<u8>, crate::embeds::EmbedImageError>) {
    let url = request.url.clone();

    tokio::task::spawn_blocking(move || fetch_embed_image(request.url))
        .await
        .unwrap_or_else(|error| {
            (
                url.clone(),
                Err(crate::embeds::EmbedImageError::Unreachable {
                    url,
                    message: format!("embed image task failed: {error}"),
                }),
            )
        })
}

async fn load_message_history(
    request: crate::auth::MessageHistoryRequest,
) -> (Id, Result<Vec<Message>, ApiError>) {
    let channel_id = request.channel_id;
    let result = run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.get_messages(channel_id)
    })
    .await;

    (channel_id, result)
}

async fn send_message(request: crate::auth::SendMessageRequest) -> (Id, Result<Message, ApiError>) {
    let channel_id = request.channel_id;
    let result = run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.send_message(channel_id, request.text)
    })
    .await;

    (channel_id, result)
}

async fn post_typing(request: crate::auth::TypingPingRequest) -> Result<(), ApiError> {
    run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.post_typing(request.channel_id)
    })
    .await
}

async fn edit_message(
    request: crate::auth::EditMessageRequest,
) -> (Id, Id, Result<Message, ApiError>) {
    let message_id = request.message_id;
    let channel_id = request.channel_id;
    let result = run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.edit_message(message_id, request.text)
    })
    .await;

    (message_id, channel_id, result)
}

async fn delete_message(
    request: crate::auth::DeleteMessageRequest,
) -> (Id, Id, Result<(), ApiError>) {
    let message_id = request.message_id;
    let channel_id = request.channel_id;
    let result = run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.delete_message(message_id)
    })
    .await;

    (message_id, channel_id, result)
}

async fn suppress_message_embeds(
    request: crate::auth::SuppressMessageEmbedsRequest,
) -> (Id, Id, Result<MessageEmbedsUpdatedEvent, ApiError>) {
    let message_id = request.message_id;
    let channel_id = request.channel_id;
    let result = run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.suppress_message_embeds(message_id, request.suppress)
    })
    .await;

    (message_id, channel_id, result)
}

async fn load_voice_participants(
    request: crate::auth::VoiceParticipantsRequest,
) -> Result<Vec<VoiceParticipant>, ApiError> {
    run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;

        let mut participants = Vec::new();
        for channel_id in request.channel_ids {
            participants.extend(api.list_voice_participants(channel_id)?);
        }

        Ok(participants)
    })
    .await
}

async fn load_voice_token(
    request: crate::auth::VoiceTokenRequest,
) -> (Id, Result<VoiceToken, ApiError>) {
    let channel_id = request.channel_id;
    let result = run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.get_voice_token(channel_id)
    })
    .await;

    (channel_id, result)
}

async fn post_voice_speaking(request: crate::auth::VoiceSpeakingRequest) -> Result<(), ApiError> {
    run_blocking_api(move || {
        let api = runtime_api()?;
        api.set_base_url(request.server_url)?;
        api.set_session_token(request.session_token)?;
        api.post_voice_speaking(request.channel_id, request.speaking)
    })
    .await
}

async fn open_external_url(url: String) -> (String, Result<(), ExternalOpenError>) {
    let result = tokio::task::spawn_blocking({
        let url = url.clone();
        move || PlatformExternalOpen.open_external_url(&url)
    })
    .await
    .unwrap_or_else(|error| Err(ExternalOpenError::Platform(error.to_string())));

    (url, result)
}

async fn start_realtime(_request: crate::auth::AuthenticatedRequest) -> Result<(), RealtimeError> {
    Ok(())
}

async fn stop_realtime() -> Result<(), RealtimeError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;

    #[test]
    fn authenticate_runs_blocking_http_transport_off_async_runtime()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let server_url = format!("http://{}", listener.local_addr()?);
        drop(listener);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        let result = runtime.block_on(authenticate(AuthRequest {
            action: AuthAction::Login,
            server_url: server_url.clone(),
            username: "alice".to_string(),
            password: "secret".to_string(),
            email: None,
        }));

        assert!(matches!(
            result,
            Err(ApiError::Unreachable { server_url: url, .. }) if url == server_url
        ));

        Ok(())
    }
}
