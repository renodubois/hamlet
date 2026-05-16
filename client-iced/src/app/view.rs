use iced::widget::{button, column, container, image, rich_text, row, span, text, text_input};
use iced::{Color, ContentFit, Element, Fill, border};

use crate::auth::{
    AuthStatus, AvatarUpdateStatus, ChannelListState, ChannelReorderState, CreateChannelState,
    CreateChannelStatus, LogoutStatus, MessageActionState, MessageActionVisibility,
    MessageHistoryState, SendMessageStatus, ServerUrlStatus, TypingState, VoiceConnectionState,
    VoicePresenceState, message_action_visibility,
};
use crate::avatar::{AvatarImageCache, fallback_avatar};
use crate::embeds::{
    EmbedImageCache, EmbedImageStatus, EmbedRenderMode, embed_render_mode, embed_site_label,
};
use crate::emoji::{EmojiPickerState, MAX_VISIBLE_CHOICES};
use crate::external_open::{ExternalLinkStatus, MessageTextSegment, parse_message_text};
use crate::protocol::{Channel, ChannelKind, Embed, Id, Message, User, VoiceParticipant};

use super::message::{AppMessage, ChannelMoveDirection};
use super::route::Route;
use super::state::AppState;
use super::widget_ids::{
    COMPOSER_INPUT_ID, CREATE_CHANNEL_NAME_INPUT_ID, EMOJI_SEARCH_INPUT_ID, PASSWORD_INPUT_ID,
    SERVER_URL_INPUT_ID, USERNAME_INPUT_ID,
};

pub fn view(state: &AppState) -> Element<'_, AppMessage> {
    match state.route {
        Route::SignedOut => signed_out_view(state),
        Route::SignedIn => signed_in_view(state),
    }
}

fn signed_out_view(state: &AppState) -> Element<'_, AppMessage> {
    let signed_out = &state.signed_out;

    let mut content = column![
        text("Hamlet").size(40),
        text("Native Iced client").size(18),
        text("Connect to a Hamlet server to continue."),
    ]
    .spacing(12)
    .width(420);

    if state.is_loading_preferences() {
        content = content.push(text("Loading saved preferences…"));
    }

    if state.is_restoring_session() {
        content = content.push(text("Restoring your saved session…"));
    }

    content = content
        .push(text("Server URL"))
        .push(
            text_input("http://localhost:3030", &signed_out.server_url)
                .id(SERVER_URL_INPUT_ID)
                .on_input(AppMessage::ServerUrlEdited)
                .padding(10)
                .width(Fill),
        )
        .push(save_server_url_button(&signed_out.server_url_status));

    if let Some(message) = signed_out.server_url_status.message() {
        content = content.push(text(message));
    }

    content = content
        .push(text("Username"))
        .push(
            text_input("username", &signed_out.username)
                .id(USERNAME_INPUT_ID)
                .on_input(AppMessage::UsernameEdited)
                .padding(10)
                .width(Fill),
        )
        .push(text("Password"))
        .push(
            text_input("password", &signed_out.password)
                .id(PASSWORD_INPUT_ID)
                .secure(true)
                .on_input(AppMessage::PasswordEdited)
                .padding(10)
                .width(Fill),
        );

    if let Some(label) = signed_out.auth_status.submitting_label() {
        content = content.push(text(label));
    }

    content = content.push(
        row![
            login_button(&signed_out.auth_status),
            register_button(&signed_out.auth_status),
        ]
        .spacing(8),
    );

    #[cfg(debug_assertions)]
    {
        let dev_button = if signed_out.auth_status.is_submitting() || state.is_restoring_session() {
            button("Use dev credentials (debug)")
        } else {
            button("Use dev credentials (debug)").on_press(AppMessage::UseDevCredentials)
        };

        content = content.push(dev_button);
    }

    if let Some(message) = signed_out.auth_status.message() {
        content = content.push(text(message));
    }

    if let Some(notice) = &signed_out.notice {
        content = content.push(text(notice));
    }

    container(content)
        .padding(32)
        .center_x(Fill)
        .center_y(Fill)
        .into()
}

fn signed_in_view(state: &AppState) -> Element<'_, AppMessage> {
    let Some(signed_in) = &state.signed_in else {
        return container(text("Signed-in state is unavailable."))
            .padding(32)
            .center_x(Fill)
            .center_y(Fill)
            .into();
    };
    let display_name = signed_in.display_name();
    let settings_button = if signed_in.profile_settings.is_open {
        button("Close settings").on_press(AppMessage::CloseSettingsPressed)
    } else {
        button("Settings").on_press(AppMessage::OpenSettingsPressed)
    };
    let logout_button = match signed_in.logout_status {
        LogoutStatus::Idle => button("Log out").on_press(AppMessage::LogoutPressed),
        LogoutStatus::LoggingOut => button("Logging out…"),
    };

    let avatar_context = AvatarViewContext {
        cache: &signed_in.avatar_images,
        server_url: &signed_in.server_url,
    };
    let sidebar_voice = SidebarVoiceContext {
        presence: &signed_in.voice_presence,
        connection: &signed_in.voice_connection,
        avatars: avatar_context,
    };
    let sidebar = channel_sidebar(
        &signed_in.channels,
        signed_in.selected_channel_id,
        &signed_in.channel_reorder,
        &signed_in.create_channel,
        &signed_in.user,
        sidebar_voice,
    );
    let main = match &signed_in.channels {
        ChannelListState::Loaded(channels) => channel_main(
            channels,
            signed_in.selected_channel_id,
            TextChannelViewState {
                history: &signed_in.message_history,
                current_user_id: signed_in.user.id,
                message_actions: &signed_in.message_actions,
                draft: &signed_in.draft,
                emoji_picker: &signed_in.emoji_picker,
                send_status: &signed_in.send_status,
                typing: &signed_in.typing,
                external_link_status: &signed_in.external_link_status,
                avatar_images: &signed_in.avatar_images,
                embed_images: &signed_in.embed_images,
                server_url: &signed_in.server_url,
            },
            &signed_in.voice_presence,
            &signed_in.voice_connection,
            &signed_in.avatar_images,
            &signed_in.server_url,
        ),
        ChannelListState::Failed(message) => column![
            text("Could not load channels."),
            text(message),
            button("Retry channels").on_press(AppMessage::RetryChannelsPressed),
        ]
        .spacing(8),
        ChannelListState::Loading => column![text("Loading channels…")].spacing(8),
        ChannelListState::NotLoaded => column![text("Channels are not loaded yet.")].spacing(8),
    };

    let mut content = column![
        text("Hamlet").size(40),
        row![
            avatar_view(
                &signed_in.avatar_images,
                &signed_in.server_url,
                signed_in.user.id,
                &signed_in.user.username,
                signed_in.user.display_name.as_deref(),
                signed_in.user.avatar_url.as_deref(),
                40,
            ),
            text(format!("Signed in as {display_name}")),
            settings_button,
            logout_button,
        ]
        .spacing(8),
        text(format!("Realtime: {:?}", signed_in.realtime_status)),
        text(
            signed_in
                .voice_connection
                .message()
                .unwrap_or_else(|| "Voice: idle".to_string()),
        ),
    ]
    .spacing(12)
    .width(Fill);

    if signed_in.profile_settings.is_open {
        content = content.push(profile_settings_view(signed_in));
    }

    content = content.push(row![sidebar, main].spacing(24));

    container(content)
        .padding(32)
        .center_x(Fill)
        .center_y(Fill)
        .into()
}

fn profile_settings_view<'a>(
    signed_in: &'a crate::auth::SignedInState,
) -> iced::widget::Column<'a, AppMessage> {
    let profile = &signed_in.profile_settings;
    let save_button = if profile.status.is_saving() {
        button("Saving display name…")
    } else {
        button("Save display name").on_press(AppMessage::SaveDisplayNamePressed)
    };
    let clear_button = if profile.status.is_saving() {
        button("Clear display name")
    } else {
        button("Clear display name").on_press(AppMessage::ClearDisplayNamePressed)
    };
    let display_name_state = signed_in
        .user
        .display_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .map(|name| format!("Current display name: {name}"))
        .unwrap_or_else(|| "Current display name: not set (using username)".to_string());

    let voice = &signed_in.voice_settings;
    let save_voice_button = if voice.status.is_saving() {
        button("Saving voice preferences…")
    } else {
        button("Save voice preferences").on_press(AppMessage::SaveVoicePreferencesPressed)
    };

    let upload_avatar_button = if profile.avatar_status.is_busy() {
        button(avatar_busy_label(&profile.avatar_status))
    } else {
        button("Upload picture").on_press(AppMessage::SelectAvatarPressed)
    };
    let remove_avatar_button =
        if profile.avatar_status.is_busy() || signed_in.user.avatar_url.is_none() {
            button("Remove picture")
        } else {
            button("Remove picture").on_press(AppMessage::DeleteAvatarPressed)
        };

    let mut content = column![
        text("Profile settings").size(24),
        row![
            avatar_view(
                &signed_in.avatar_images,
                &signed_in.server_url,
                signed_in.user.id,
                &signed_in.user.username,
                signed_in.user.display_name.as_deref(),
                signed_in.user.avatar_url.as_deref(),
                96,
            ),
            column![
                text(format!("Username: {}", signed_in.user.username)),
                text(display_name_state),
                text(format!("Displayed as: {}", signed_in.display_name())),
                row![upload_avatar_button, remove_avatar_button].spacing(8),
            ]
            .spacing(4),
        ]
        .spacing(12),
        text("Display name"),
        text_input("Optional display name", &profile.display_name_input)
            .on_input(AppMessage::ProfileDisplayNameEdited)
            .padding(10)
            .width(Fill),
        row![save_button, clear_button].spacing(8),
        text("Voice preferences").size(18),
        text("Preferred microphone device ID (optional)"),
        text_input(
            "Use system default microphone",
            &voice.microphone_device_id_input
        )
        .on_input(AppMessage::VoiceMicrophoneDeviceEdited)
        .padding(10)
        .width(Fill),
        text("Preferred output device ID (optional)"),
        text_input("Use system default output", &voice.output_device_id_input)
            .on_input(AppMessage::VoiceOutputDeviceEdited)
            .padding(10)
            .width(Fill),
        save_voice_button,
        button("Close settings").on_press(AppMessage::CloseSettingsPressed),
    ]
    .spacing(6)
    .width(Fill);

    if let Some(message) = profile.status.message() {
        content = content.push(text(message));
    }
    if let Some(message) = profile.avatar_status.message() {
        content = content.push(text(message));
    }
    if let Some(message) = voice.status.message() {
        content = content.push(text(message));
    }

    content
}

#[derive(Debug, Clone, Copy)]
struct AvatarViewContext<'a> {
    cache: &'a AvatarImageCache,
    server_url: &'a str,
}

#[derive(Debug, Clone, Copy)]
struct SidebarVoiceContext<'a> {
    presence: &'a VoicePresenceState,
    connection: &'a VoiceConnectionState,
    avatars: AvatarViewContext<'a>,
}

fn channel_sidebar<'a>(
    channels: &'a ChannelListState,
    selected_channel_id: Option<i64>,
    channel_reorder: &'a ChannelReorderState,
    create_channel: &'a CreateChannelState,
    current_user: &'a User,
    voice: SidebarVoiceContext<'a>,
) -> iced::widget::Column<'a, AppMessage> {
    let mut sidebar = column![
        text("Channels").size(24),
        current_user_sidebar_row(current_user, voice.avatars),
    ]
    .spacing(6)
    .width(220);

    if let Some(message) = channel_reorder.message() {
        sidebar = sidebar.push(text(message));
    }
    if let Some(message) = voice.presence.message() {
        sidebar = sidebar.push(text(message));
    }
    if let Some(message) = voice.connection.message() {
        sidebar = sidebar.push(text(message));
    }

    sidebar = sidebar.push(create_channel_form(create_channel));

    match channels {
        ChannelListState::Loaded(channels) => {
            let disable_reorder = channel_reorder.is_committing();
            for (index, channel) in channels.iter().enumerate() {
                sidebar = sidebar.push(channel_entry(
                    channel,
                    selected_channel_id,
                    index,
                    channels.len(),
                    disable_reorder,
                    voice,
                ));
            }
        }
        ChannelListState::Loading => {
            sidebar = sidebar.push(text("Loading…"));
        }
        ChannelListState::Failed(message) => {
            sidebar = sidebar
                .push(text("Failed to load channels."))
                .push(text(message))
                .push(button("Retry").on_press(AppMessage::RetryChannelsPressed));
        }
        ChannelListState::NotLoaded => {
            sidebar = sidebar.push(text("Not loaded."));
        }
    }

    sidebar
}

fn current_user_sidebar_row<'a>(
    user: &'a User,
    avatars: AvatarViewContext<'a>,
) -> iced::widget::Row<'a, AppMessage> {
    row![
        avatar_view(
            avatars.cache,
            avatars.server_url,
            user.id,
            &user.username,
            user.display_name.as_deref(),
            user.avatar_url.as_deref(),
            32,
        ),
        column![
            text(user_display_name(user).to_string()).size(14),
            text(format!("@{}", user.username)).size(12),
        ]
        .spacing(1),
    ]
    .spacing(8)
}

fn user_display_name(user: &User) -> &str {
    user.display_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&user.username)
}

fn create_channel_form<'a>(
    create_channel: &'a CreateChannelState,
) -> iced::widget::Column<'a, AppMessage> {
    let text_button = channel_kind_button(create_channel, ChannelKind::Text, "Text");
    let voice_button = channel_kind_button(create_channel, ChannelKind::Voice, "Voice");
    let submit_button = match create_channel.status {
        CreateChannelStatus::Creating => button("Creating…"),
        CreateChannelStatus::Idle | CreateChannelStatus::Failed(_) => {
            button("Create").on_press(AppMessage::CreateChannelPressed)
        }
    };
    let mut form = column![
        text("Create channel").size(18),
        text_input("channel-name", &create_channel.name)
            .id(CREATE_CHANNEL_NAME_INPUT_ID)
            .on_input(AppMessage::CreateChannelNameEdited)
            .padding(10)
            .width(Fill),
        row![text_button, voice_button].spacing(6),
        submit_button,
    ]
    .spacing(6);

    if let Some(message) = create_channel.status.message() {
        form = form.push(text(message));
    }

    form
}

fn channel_kind_button<'a>(
    create_channel: &'a CreateChannelState,
    kind: ChannelKind,
    label: &'static str,
) -> iced::widget::Button<'a, AppMessage> {
    let selected = if create_channel.kind == kind {
        " •"
    } else {
        ""
    };
    let button = button(text(format!("{label}{selected}")));

    match create_channel.status {
        CreateChannelStatus::Creating => button,
        CreateChannelStatus::Idle | CreateChannelStatus::Failed(_) => {
            button.on_press(AppMessage::CreateChannelKindSelected(kind))
        }
    }
}

fn channel_entry<'a>(
    channel: &'a Channel,
    selected_channel_id: Option<i64>,
    index: usize,
    channel_count: usize,
    disable_reorder: bool,
    voice: SidebarVoiceContext<'a>,
) -> iced::widget::Column<'a, AppMessage> {
    let mut entry = column![channel_row(
        channel,
        selected_channel_id,
        index,
        channel_count,
        disable_reorder,
    )]
    .spacing(2);

    if channel.kind == ChannelKind::Voice {
        entry = entry.push(voice_participants_sidebar(channel, voice));
    }

    entry
}

fn channel_row<'a>(
    channel: &'a Channel,
    selected_channel_id: Option<i64>,
    index: usize,
    channel_count: usize,
    disable_reorder: bool,
) -> iced::widget::Row<'a, AppMessage> {
    row![
        channel_button(channel, selected_channel_id).width(Fill),
        move_channel_button(
            channel,
            ChannelMoveDirection::Up,
            disable_reorder || index == 0,
        ),
        move_channel_button(
            channel,
            ChannelMoveDirection::Down,
            disable_reorder || index.saturating_add(1) >= channel_count,
        ),
    ]
    .spacing(4)
}

fn move_channel_button<'a>(
    channel: &'a Channel,
    direction: ChannelMoveDirection,
    disabled: bool,
) -> iced::widget::Button<'a, AppMessage> {
    let label = match direction {
        ChannelMoveDirection::Up => "Move up",
        ChannelMoveDirection::Down => "Move down",
    };
    let button = button(text(label));

    if disabled {
        button
    } else {
        button.on_press(AppMessage::MoveChannelRequested {
            channel_id: channel.id,
            direction,
        })
    }
}

fn channel_button<'a>(
    channel: &'a Channel,
    selected_channel_id: Option<i64>,
) -> iced::widget::Button<'a, AppMessage> {
    let prefix = match channel.kind {
        ChannelKind::Text => "#",
        ChannelKind::Voice => "🔊",
    };
    let selected = if Some(channel.id) == selected_channel_id {
        " •"
    } else {
        ""
    };

    button(text(format!("{prefix} {}{selected}", channel.name)))
        .on_press(AppMessage::ChannelSelected(channel.id))
}

fn voice_participants_sidebar<'a>(
    channel: &'a Channel,
    voice: SidebarVoiceContext<'a>,
) -> iced::widget::Column<'a, AppMessage> {
    let participants = voice.presence.participants(channel.id);
    let mut list = iced::widget::Column::new()
        .spacing(1)
        .push(voice_connection_button(channel, voice.connection));

    if let Some(error) = voice
        .connection
        .error
        .as_ref()
        .filter(|error| error.channel_id == Some(channel.id))
    {
        list = list.push(text(format!("  {}", error.message)).size(14));
    }

    for participant in participants {
        let suffix = if voice.presence.is_speaking(channel.id, participant.user_id) {
            " (speaking)"
        } else {
            ""
        };
        list = list.push(
            row![
                text("  •"),
                avatar_view(
                    voice.avatars.cache,
                    voice.avatars.server_url,
                    participant.user_id,
                    &participant.username,
                    None,
                    participant.avatar_url.as_deref(),
                    18,
                ),
                text(format!("{}{}", participant.username, suffix)).size(14),
            ]
            .spacing(4),
        );
    }

    list
}

fn voice_connection_button<'a>(
    channel: &'a Channel,
    voice_connection: &'a VoiceConnectionState,
) -> iced::widget::Button<'a, AppMessage> {
    let button = if voice_connection.is_connected_to(channel.id) {
        button("Leave voice")
    } else if voice_connection.is_connecting_to(channel.id) {
        button("Connecting voice…")
    } else if voice_connection.is_disconnecting_from(channel.id) {
        button("Leaving voice…")
    } else if voice_connection.connected_channel_id().is_some()
        || voice_connection.target_channel_id().is_some()
    {
        button("Switch voice")
    } else {
        button("Join voice")
    };

    if voice_connection.is_connected_to(channel.id) {
        button.on_press(AppMessage::VoiceLeavePressed)
    } else if voice_connection.is_connecting_to(channel.id)
        || voice_connection.is_disconnecting_from(channel.id)
    {
        button
    } else {
        button.on_press(AppMessage::VoiceJoinPressed(channel.id))
    }
}

struct TextChannelViewState<'a> {
    history: &'a MessageHistoryState,
    current_user_id: Id,
    message_actions: &'a MessageActionState,
    draft: &'a str,
    emoji_picker: &'a EmojiPickerState,
    send_status: &'a SendMessageStatus,
    typing: &'a TypingState,
    external_link_status: &'a ExternalLinkStatus,
    avatar_images: &'a AvatarImageCache,
    embed_images: &'a EmbedImageCache,
    server_url: &'a str,
}

fn channel_main<'a>(
    channels: &'a [Channel],
    selected_channel_id: Option<i64>,
    text_channel: TextChannelViewState<'a>,
    voice_presence: &'a VoicePresenceState,
    voice_connection: &'a VoiceConnectionState,
    avatar_images: &'a AvatarImageCache,
    server_url: &'a str,
) -> iced::widget::Column<'a, AppMessage> {
    let Some(channel) =
        selected_channel_id.and_then(|id| channels.iter().find(|channel| channel.id == id))
    else {
        return column![text("No text channels are available yet.")].spacing(8);
    };

    match channel.kind {
        ChannelKind::Text => text_channel_view(channel, text_channel),
        ChannelKind::Voice => voice_channel_view(
            channel,
            voice_presence,
            voice_connection,
            avatar_images,
            server_url,
        ),
    }
}

fn voice_channel_view<'a>(
    channel: &'a Channel,
    voice_presence: &'a VoicePresenceState,
    voice_connection: &'a VoiceConnectionState,
    avatar_images: &'a AvatarImageCache,
    server_url: &'a str,
) -> iced::widget::Column<'a, AppMessage> {
    let participants = voice_presence.participants(channel.id);
    let mut content = column![
        text(format!("🔊 {}", channel.name)).size(24),
        text("Native LiveKit voice is available for this channel."),
        voice_connection_button(channel, voice_connection),
        voice_controls(channel, voice_connection),
    ]
    .spacing(8);

    if let Some(message) = voice_connection.message() {
        content = content.push(text(message));
    }
    if let Some(error) = voice_connection
        .error
        .as_ref()
        .filter(|error| error.channel_id == Some(channel.id))
    {
        content = content.push(button("Retry voice connection").on_press(
            AppMessage::VoiceJoinPressed(error.channel_id.unwrap_or(channel.id)),
        ));
    }

    if participants.is_empty() {
        content = content.push(text("No one is connected."));
    } else {
        content = content.push(text("Connected participants:"));
        for participant in participants {
            content = content.push(voice_participant_row(
                participant,
                voice_presence.is_speaking(channel.id, participant.user_id),
                avatar_images,
                server_url,
            ));
        }
    }

    content
}

fn voice_controls<'a>(
    channel: &'a Channel,
    voice_connection: &'a VoiceConnectionState,
) -> iced::widget::Row<'a, AppMessage> {
    let mute_label = if voice_connection.muted {
        "Unmute microphone"
    } else {
        "Mute microphone"
    };
    let deafen_label = if voice_connection.deafened {
        "Undeafen audio"
    } else {
        "Deafen audio"
    };
    let mute_message = if voice_connection.muted {
        AppMessage::VoiceUnmutePressed
    } else {
        AppMessage::VoiceMutePressed
    };
    let deafen_message = if voice_connection.deafened {
        AppMessage::VoiceUndeafenPressed
    } else {
        AppMessage::VoiceDeafenPressed
    };
    let mute_button = if voice_connection.is_connected_to(channel.id) {
        button(mute_label).on_press(mute_message)
    } else {
        button(mute_label)
    };
    let deafen_button = if voice_connection.is_connected_to(channel.id) {
        button(deafen_label).on_press(deafen_message)
    } else {
        button(deafen_label)
    };

    row![mute_button, deafen_button].spacing(8)
}

fn voice_participant_row<'a>(
    participant: &'a VoiceParticipant,
    speaking: bool,
    avatar_images: &'a AvatarImageCache,
    server_url: &'a str,
) -> iced::widget::Row<'a, AppMessage> {
    let suffix = if speaking { " (speaking)" } else { "" };

    row![
        text("•"),
        avatar_view(
            avatar_images,
            server_url,
            participant.user_id,
            &participant.username,
            None,
            participant.avatar_url.as_deref(),
            24,
        ),
        text(format!("{}{}", participant.username, suffix)),
    ]
    .spacing(6)
}

fn text_channel_view<'a>(
    channel: &'a Channel,
    text_channel: TextChannelViewState<'a>,
) -> iced::widget::Column<'a, AppMessage> {
    let mut content = column![text(format!("# {}", channel.name)).size(24)].spacing(8);

    if let Some(message) = text_channel.external_link_status.message() {
        content = content.push(text(message));
    }

    match text_channel.history {
        MessageHistoryState::NotLoaded => {
            content = content.push(text("No message history loaded."));
        }
        MessageHistoryState::Loading { .. } => {
            content = content.push(text("Loading message history…"));
        }
        MessageHistoryState::Failed { message, .. } => {
            content = content
                .push(text("Could not load message history."))
                .push(text(message))
                .push(button("Retry messages").on_press(AppMessage::RetryMessageHistoryPressed));
        }
        MessageHistoryState::Loaded { messages, .. } => {
            if messages.is_empty() {
                content = content.push(text("No messages yet."));
            } else {
                for message in messages {
                    content = content.push(message_row(
                        message,
                        text_channel.current_user_id,
                        text_channel.message_actions,
                        text_channel.avatar_images,
                        text_channel.embed_images,
                        text_channel.server_url,
                    ));
                }
            }
        }
    }

    if let Some(message) = text_channel.typing.indicator_message(channel.id) {
        content = content.push(text(message));
    }

    content = content.push(message_composer(
        text_channel.draft,
        text_channel.emoji_picker,
        text_channel.send_status,
    ));

    if let Some(message) = text_channel.send_status.message() {
        content = content.push(text(message));
    }

    content
}

fn message_composer<'a>(
    draft: &'a str,
    emoji_picker: &'a EmojiPickerState,
    send_status: &'a SendMessageStatus,
) -> iced::widget::Column<'a, AppMessage> {
    let input = text_input("Message", draft)
        .id(COMPOSER_INPUT_ID)
        .on_input(AppMessage::DraftEdited)
        .padding(10)
        .width(Fill);
    let emoji_button_label = if emoji_picker.is_open {
        "Close emoji"
    } else {
        "Emoji"
    };
    let emoji_button = button(emoji_button_label).on_press(AppMessage::ToggleEmojiPickerPressed);
    let send_button = match send_status {
        SendMessageStatus::Sending => button("Sending…"),
        SendMessageStatus::Idle | SendMessageStatus::Failed(_) => {
            button("Send").on_press(AppMessage::SendMessagePressed)
        }
    };
    let mut composer = column![row![input, emoji_button, send_button].spacing(8)].spacing(8);

    if emoji_picker.is_open {
        composer = composer.push(emoji_picker_view(emoji_picker));
    }

    composer
}

fn emoji_picker_view<'a>(
    emoji_picker: &'a EmojiPickerState,
) -> iced::widget::Column<'a, AppMessage> {
    let choices = emoji_picker.filtered_choices();
    let visible_count = choices.len().min(MAX_VISIBLE_CHOICES);
    let mut picker = column![
        row![
            text("Emoji picker").size(18),
            button("Close").on_press(AppMessage::CloseEmojiPickerPressed),
        ]
        .spacing(8),
        text_input("Search emoji", &emoji_picker.query)
            .id(EMOJI_SEARCH_INPUT_ID)
            .on_input(AppMessage::EmojiSearchEdited)
            .padding(8)
            .width(Fill),
        text("Use ↑/↓ to navigate, Enter to insert, Escape to close."),
    ]
    .spacing(6)
    .width(Fill);

    if choices.is_empty() {
        return picker.push(text("No emoji found."));
    }

    for (index, choice) in choices.iter().take(MAX_VISIBLE_CHOICES).enumerate() {
        let selection_marker = if index == emoji_picker.selected_index {
            "▶"
        } else {
            " "
        };
        picker = picker.push(
            button(text(format!(
                "{selection_marker} {} {}",
                choice.symbol, choice.name
            )))
            .on_press(AppMessage::EmojiSelected(choice.symbol.to_string())),
        );
    }

    if choices.len() > visible_count {
        picker = picker.push(text(format!(
            "Showing {visible_count} of {} matches. Keep typing to narrow the list.",
            choices.len()
        )));
    }

    picker
}

fn message_row<'a>(
    message: &'a Message,
    current_user_id: Id,
    actions: &'a MessageActionState,
    avatar_images: &'a AvatarImageCache,
    embed_images: &'a EmbedImageCache,
    server_url: &'a str,
) -> Element<'a, AppMessage> {
    let author = message_author(message);
    let avatar = avatar_view(
        avatar_images,
        server_url,
        message.user_id,
        &message.username,
        message.display_name.as_deref(),
        message.avatar_url.as_deref(),
        32,
    );

    if let Some(editing) = actions
        .editing
        .as_ref()
        .filter(|editing| editing.message_id == message.id)
    {
        let input = text_input("Edit message", &editing.draft)
            .on_input(AppMessage::EditMessageDraftEdited)
            .padding(8)
            .width(Fill);
        let save_button = if editing.status.is_saving() {
            button("Saving…")
        } else {
            button("Save edit").on_press(AppMessage::SaveMessageEditPressed)
        };
        let cancel_button = button("Cancel").on_press(AppMessage::CancelMessageEditPressed);
        let mut content = column![
            text(format!("{author}:")),
            row![input, save_button, cancel_button].spacing(8),
        ]
        .spacing(4);

        if let Some(message) = editing.status.message() {
            content = content.push(text(message));
        }

        return row![avatar, content].spacing(8).into();
    }

    let mut content = column![message_text(message, author)].spacing(4);
    let visibility = message_action_visibility(current_user_id, message);

    if !message.suppress_embeds && !message.embeds.is_empty() {
        content = content.push(message_embeds(
            message,
            visibility,
            actions,
            embed_images,
            server_url,
        ));
    }

    if visibility.has_any_action() {
        let edit_button = if actions.is_deleting(message.id) {
            button("Edit")
        } else {
            button("Edit").on_press(AppMessage::EditMessagePressed(message.id))
        };
        let delete_button = if actions.is_deleting(message.id) {
            button("Deleting…")
        } else {
            button("Delete").on_press(AppMessage::DeleteMessagePressed(message.id))
        };

        content = content.push(row![edit_button, delete_button].spacing(6));
    }

    if let Some(message) = actions.delete_message(message.id) {
        content = content.push(text(message));
    }

    if let Some(message) = actions.suppress_embeds_message(message.id) {
        content = content.push(text(message));
    }

    row![avatar, content].spacing(8).into()
}

fn message_embeds<'a>(
    message: &'a Message,
    visibility: MessageActionVisibility,
    actions: &'a MessageActionState,
    embed_images: &'a EmbedImageCache,
    server_url: &'a str,
) -> iced::widget::Column<'a, AppMessage> {
    let mut embeds = iced::widget::Column::new().spacing(4);
    let is_suppressing = actions.is_suppressing_embeds(message.id);

    for embed in &message.embeds {
        embeds = embeds.push(message_embed_card(
            embed,
            message.id,
            visibility.can_suppress_embeds,
            is_suppressing,
            embed_images,
            server_url,
        ));
    }

    embeds
}

fn message_embed_card<'a>(
    embed: &'a Embed,
    message_id: Id,
    can_suppress: bool,
    is_suppressing: bool,
    embed_images: &'a EmbedImageCache,
    server_url: &'a str,
) -> Element<'a, AppMessage> {
    let mode = embed_render_mode(embed);
    let title = embed
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(&embed.url);
    let mut body = column![
        text(embed_site_label(embed)).size(12),
        button(text(title)).on_press(AppMessage::OpenExternalUrlRequested(embed.url.clone())),
    ]
    .spacing(3)
    .width(420);

    if mode == EmbedRenderMode::ExternalOpenCard {
        body = body
            .push(text(
                "This preview requires embedded web content. Open it externally to view it.",
            ))
            .push(
                button("Open externally")
                    .on_press(AppMessage::OpenExternalUrlRequested(embed.url.clone())),
            );
    }

    if let Some(description) = embed
        .description
        .as_deref()
        .filter(|description| !description.trim().is_empty())
    {
        body = body.push(text(description));
    }

    if let Some(preview) = embed_preview_image(embed, embed_images, server_url) {
        body = body.push(preview);
    }

    if mode == EmbedRenderMode::NativeImagePreview {
        body = body.push(
            button("Open image source")
                .on_press(AppMessage::OpenExternalUrlRequested(embed.url.clone())),
        );
    }

    let mut card = column![body].spacing(4);

    if can_suppress {
        let suppress_button = if is_suppressing {
            button("Suppressing embeds…")
        } else {
            button("Suppress embeds").on_press(AppMessage::SuppressEmbedsPressed(message_id))
        };
        card = card.push(suppress_button);
    }

    container(card).padding(8).width(440).into()
}

fn embed_preview_image<'a>(
    embed: &'a Embed,
    embed_images: &'a EmbedImageCache,
    server_url: &'a str,
) -> Option<Element<'a, AppMessage>> {
    let image_url = embed
        .image_url
        .as_deref()
        .filter(|image_url| !image_url.trim().is_empty())?;

    if let Some(handle) = embed_images.handle_for(server_url, Some(image_url)) {
        return Some(
            image(handle.clone())
                .width(420)
                .height(240)
                .content_fit(ContentFit::Contain)
                .into(),
        );
    }

    let label = match embed_images.status_for_image_url(server_url, image_url) {
        Some(EmbedImageStatus::Loading) => "Loading image preview…",
        Some(EmbedImageStatus::Failed(_)) => "Image preview unavailable.",
        Some(EmbedImageStatus::Loaded { .. }) | None => "Image preview queued…",
    };

    Some(text(label).into())
}

fn message_author(message: &Message) -> &str {
    message
        .display_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&message.username)
}

fn message_text<'a>(message: &Message, author: &str) -> Element<'a, AppMessage> {
    let segments = parse_message_text(&message.text);
    let has_links = segments
        .iter()
        .any(|segment| matches!(segment, MessageTextSegment::Link { .. }));

    if !has_links {
        return text(format!("{author}: {}", message.text)).into();
    }

    let mut spans: Vec<iced::widget::text::Span<'static, String>> = Vec::new();
    spans.push(span(format!("{author}: ")));

    for segment in segments {
        match segment {
            MessageTextSegment::Text(text) => spans.push(span(text)),
            MessageTextSegment::Link { text, url } => spans.push(
                span(text)
                    .link(url)
                    .underline(true)
                    .color(Color::from_rgb8(88, 166, 255)),
            ),
        }
    }

    rich_text(spans)
        .on_link_click(AppMessage::OpenExternalUrlRequested)
        .into()
}

fn avatar_view<'a>(
    cache: &AvatarImageCache,
    server_url: &str,
    user_id: Id,
    username: &str,
    display_name: Option<&str>,
    avatar_url: Option<&str>,
    size: u16,
) -> Element<'a, AppMessage> {
    if let Some(handle) = cache.handle_for(server_url, avatar_url) {
        return image(handle.clone())
            .width(u32::from(size))
            .height(u32::from(size))
            .content_fit(ContentFit::Cover)
            .border_radius(border::radius(f32::from(size) / 2.0))
            .into();
    }

    let fallback = fallback_avatar(user_id, username, display_name);
    let background = Color::from_rgb8(
        fallback.background_rgb[0],
        fallback.background_rgb[1],
        fallback.background_rgb[2],
    );
    let foreground = Color::from_rgb8(
        fallback.foreground_rgb[0],
        fallback.foreground_rgb[1],
        fallback.foreground_rgb[2],
    );

    container(text(fallback.initials).size(u32::from((size / 2).max(10))))
        .center(u32::from(size))
        .style(move |_| container::Style {
            text_color: Some(foreground),
            background: Some(background.into()),
            border: border::rounded(f32::from(size) / 2.0),
            ..container::Style::default()
        })
        .into()
}

fn avatar_busy_label(status: &AvatarUpdateStatus) -> &'static str {
    match status {
        AvatarUpdateStatus::Selecting => "Opening…",
        AvatarUpdateStatus::Uploading => "Uploading…",
        AvatarUpdateStatus::Deleting => "Removing…",
        AvatarUpdateStatus::Idle | AvatarUpdateStatus::Saved | AvatarUpdateStatus::Failed(_) => {
            "Upload picture"
        }
    }
}

fn login_button<'a>(status: &'a AuthStatus) -> iced::widget::Button<'a, AppMessage> {
    if status.is_submitting() {
        button("Log in")
    } else {
        button("Log in").on_press(AppMessage::LoginPressed)
    }
}

fn register_button<'a>(status: &'a AuthStatus) -> iced::widget::Button<'a, AppMessage> {
    if status.is_submitting() {
        button("Register")
    } else {
        button("Register").on_press(AppMessage::RegisterPressed)
    }
}

fn save_server_url_button<'a>(status: &'a ServerUrlStatus) -> iced::widget::Button<'a, AppMessage> {
    match status {
        ServerUrlStatus::Saving => button("Saving server URL…"),
        _ => button("Save server URL").on_press(AppMessage::SaveServerUrlRequested),
    }
}
