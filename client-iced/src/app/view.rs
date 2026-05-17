use iced::widget::{
    button, column, container, image, rich_text, row, scrollable, span, text, text_input,
};
use iced::{Color, ContentFit, Element, Fill, Font, border, font};

use crate::auth::{
    AuthStatus, AvatarUpdateStatus, ChannelListState, ChannelReorderState, CreateChannelState,
    CreateChannelStatus, LogoutStatus, MessageActionState, MessageActionVisibility,
    MessageHistoryState, SendMessageStatus, ServerUrlStatus, TypingState, VoiceConnectionState,
    VoicePresenceState, message_action_visibility,
};
use crate::avatar::{AvatarImageCache, fallback_avatar};
use crate::design;
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

    let server_url_input = text_input("http://localhost:3030", &signed_out.server_url)
        .id(SERVER_URL_INPUT_ID)
        .on_input(AppMessage::ServerUrlEdited)
        .padding(12)
        .width(Fill)
        .style(design::text_input_style::field);
    let mut server_group = column![
        text("Server").size(18).color(design::color::TEXT),
        text("Choose the Hamlet server this desktop client should connect to.")
            .color(design::color::TEXT_MUTED),
        text("Server URL").color(design::color::TEXT_MUTED),
        server_url_input,
        save_server_url_button(&signed_out.server_url_status),
    ]
    .spacing(design::spacing::SM);

    if let Some(message) = signed_out.server_url_status.message() {
        server_group = server_group.push(text(message).color(design::color::TEXT_MUTED));
    }

    let account_group = column![
        text("Account").size(18).color(design::color::TEXT),
        text("Sign in with your username and password, or register a new account.")
            .color(design::color::TEXT_MUTED),
        text("Username").color(design::color::TEXT_MUTED),
        text_input("username", &signed_out.username)
            .id(USERNAME_INPUT_ID)
            .on_input(AppMessage::UsernameEdited)
            .padding(12)
            .width(Fill)
            .style(design::text_input_style::field),
        text("Password").color(design::color::TEXT_MUTED),
        text_input("password", &signed_out.password)
            .id(PASSWORD_INPUT_ID)
            .secure(true)
            .on_input(AppMessage::PasswordEdited)
            .padding(12)
            .width(Fill)
            .style(design::text_input_style::field),
    ]
    .spacing(design::spacing::SM);

    let mut actions = column![
        row![
            login_button(&signed_out.auth_status),
            register_button(&signed_out.auth_status),
        ]
        .spacing(design::spacing::SM),
    ]
    .spacing(design::spacing::SM);

    if let Some(label) = signed_out.auth_status.submitting_label() {
        actions = actions.push(text(label).color(design::color::TEXT_MUTED));
    }

    #[cfg(debug_assertions)]
    {
        let dev_button =
            if signed_out.auth_status.is_submitting() || state.is_restoring_session() {
                button("Use dev credentials (debug)")
            } else {
                button("Use dev credentials (debug)").on_press(AppMessage::UseDevCredentials)
            }
            .style(design::button_style::secondary);

        actions = actions.push(dev_button);
    }

    if let Some(message) = signed_out.auth_status.message() {
        actions = actions.push(text(message).color(design::color::TEXT_MUTED));
    }

    if let Some(notice) = &signed_out.notice {
        actions = actions.push(text(notice).color(design::color::TEXT_MUTED));
    }

    let mut status = column![].spacing(design::spacing::SM);
    if state.is_loading_preferences() {
        status = status.push(text("Loading saved preferences…").color(design::color::TEXT_MUTED));
    }
    if state.is_restoring_session() {
        status =
            status.push(text("Restoring your saved session…").color(design::color::TEXT_MUTED));
    }

    let panel = column![
        column![
            text("Hamlet").size(44).color(design::color::TEXT),
            text("Native Iced client")
                .size(18)
                .color(design::color::ACCENT),
            text("A focused desktop home for your Hamlet conversations.")
                .color(design::color::TEXT_MUTED),
        ]
        .spacing(design::spacing::XS),
        status,
        container(server_group)
            .padding(design::spacing::LG)
            .style(design::container_style::field_group),
        container(account_group)
            .padding(design::spacing::LG)
            .style(design::container_style::field_group),
        actions,
    ]
    .spacing(design::spacing::LG)
    .width(460);

    container(
        container(panel)
            .padding(design::spacing::XL)
            .style(design::container_style::hero_panel),
    )
    .padding(design::spacing::XXL)
    .center_x(Fill)
    .center_y(Fill)
    .style(design::container_style::app_background)
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
        signed_in,
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
        ChannelListState::Failed(message) => container(
            column![
                text("Could not load channels."),
                text(message),
                button("Retry channels").on_press(AppMessage::RetryChannelsPressed),
            ]
            .spacing(8),
        )
        .padding(design::spacing::LG)
        .width(Fill)
        .height(Fill)
        .style(design::container_style::main_surface)
        .into(),
        ChannelListState::Loading => container(text("Loading channels…"))
            .padding(design::spacing::LG)
            .width(Fill)
            .height(Fill)
            .style(design::container_style::main_surface)
            .into(),
        ChannelListState::NotLoaded => container(text("Channels are not loaded yet."))
            .padding(design::spacing::LG)
            .width(Fill)
            .height(Fill)
            .style(design::container_style::main_surface)
            .into(),
    };

    let main_content = if signed_in.profile_settings.is_open {
        column![profile_settings_view(signed_in), main]
            .spacing(design::spacing::LG)
            .height(Fill)
    } else {
        column![main].height(Fill)
    };

    let shell = row![
        container(sidebar)
            .padding(design::spacing::LG)
            .width(280)
            .height(Fill)
            .style(design::container_style::sidebar),
        container(main_content)
            .padding(design::spacing::LG)
            .width(Fill)
            .height(Fill)
            .style(design::container_style::main_shell),
    ]
    .height(Fill)
    .width(Fill);

    container(shell)
        .width(Fill)
        .height(Fill)
        .style(design::container_style::app_background)
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
    signed_in: &'a crate::auth::SignedInState,
    voice: SidebarVoiceContext<'a>,
) -> iced::widget::Column<'a, AppMessage> {
    let mut channel_nav = column![text("Channels").size(16).color(design::color::TEXT_MUTED)]
        .spacing(design::spacing::SM)
        .width(Fill);

    if let Some(message) = channel_reorder.message() {
        channel_nav = channel_nav.push(text(message));
    }
    match channels {
        ChannelListState::Loaded(channels) => {
            let disable_reorder = channel_reorder.is_committing();
            for (index, channel) in channels.iter().enumerate() {
                channel_nav = channel_nav.push(channel_entry(
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
            channel_nav = channel_nav.push(text("Loading…"));
        }
        ChannelListState::Failed(message) => {
            channel_nav = channel_nav
                .push(text("Failed to load channels."))
                .push(text(message))
                .push(button("Retry").on_press(AppMessage::RetryChannelsPressed));
        }
        ChannelListState::NotLoaded => {
            channel_nav = channel_nav.push(text("Not loaded."));
        }
    }

    column![
        column![
            text("Hamlet workspace").size(24),
            text("Native Iced client")
                .size(13)
                .color(design::color::TEXT_MUTED),
        ]
        .spacing(design::spacing::XS),
        channel_nav,
        iced::widget::Space::new().height(Fill),
        create_channel_footer(create_channel),
        user_sidebar_footer(signed_in, voice.avatars),
    ]
    .spacing(design::spacing::SM)
    .width(Fill)
    .height(Fill)
}

fn create_channel_footer<'a>(
    create_channel: &'a CreateChannelState,
) -> iced::widget::Container<'a, AppMessage> {
    let content = if create_channel.is_open {
        create_channel_form(create_channel)
    } else {
        column![button("Add channel").on_press(AppMessage::AddChannelPressed)].width(Fill)
    };

    container(content)
        .padding(design::spacing::SM)
        .width(Fill)
        .style(design::container_style::field_group)
}

fn user_sidebar_footer<'a>(
    signed_in: &'a crate::auth::SignedInState,
    avatars: AvatarViewContext<'a>,
) -> iced::widget::Container<'a, AppMessage> {
    let settings_button = if signed_in.profile_settings.is_open {
        button("Close settings").on_press(AppMessage::CloseSettingsPressed)
    } else {
        button("Settings").on_press(AppMessage::OpenSettingsPressed)
    };
    let logout_button = match signed_in.logout_status {
        LogoutStatus::Idle => button("Log out").on_press(AppMessage::LogoutPressed),
        LogoutStatus::LoggingOut => button("Logging out…"),
    };

    container(
        column![
            current_user_sidebar_row(&signed_in.user, avatars),
            text(format!("Signed in as {}", signed_in.display_name()))
                .size(12)
                .color(design::color::TEXT_MUTED),
            row![settings_button, logout_button].spacing(design::spacing::SM),
        ]
        .spacing(design::spacing::SM),
    )
    .padding(design::spacing::SM)
    .width(Fill)
    .style(design::container_style::field_group)
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
            text(format!("@{}", user.username))
                .size(12)
                .color(design::color::TEXT_MUTED),
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
    let cancel_button = match create_channel.status {
        CreateChannelStatus::Creating => button("Cancel"),
        CreateChannelStatus::Idle | CreateChannelStatus::Failed(_) => {
            button("Cancel").on_press(AppMessage::CancelCreateChannelPressed)
        }
    };
    let mut form = column![
        text("Create channel").size(16),
        text_input("channel-name", &create_channel.name)
            .id(CREATE_CHANNEL_NAME_INPUT_ID)
            .on_input(AppMessage::CreateChannelNameEdited)
            .padding(10)
            .width(Fill),
        row![text_button, voice_button].spacing(6),
        row![submit_button, cancel_button].spacing(6),
    ]
    .spacing(6)
    .width(Fill);

    if let Some(message) = create_channel.status.message() {
        form = form.push(text(message).size(12).color(design::color::TEXT_MUTED));
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
    .spacing(2)
}

fn move_channel_button<'a>(
    channel: &'a Channel,
    direction: ChannelMoveDirection,
    disabled: bool,
) -> iced::widget::Button<'a, AppMessage> {
    let label = match direction {
        ChannelMoveDirection::Up => "↑",
        ChannelMoveDirection::Down => "↓",
    };
    let button = button(text(label))
        .padding([2, 6])
        .width(28)
        .style(design::button_style::reorder_control);

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
    let selected = Some(channel.id) == selected_channel_id;
    let prefix = match channel.kind {
        ChannelKind::Text => "#",
        ChannelKind::Voice => "🔊",
    };
    let selected_marker = if selected { " •" } else { "" };
    let label = compact_channel_name(&channel.name);
    let button = button(text(format!("{prefix} {label}{selected_marker}")))
        .padding([5, 8])
        .on_press(AppMessage::ChannelSelected(channel.id));

    match channel.kind {
        ChannelKind::Text => button.style(design::button_style::channel_text(selected)),
        ChannelKind::Voice => button.style(design::button_style::channel_voice(selected)),
    }
}

fn compact_channel_name(name: &str) -> String {
    const MAX_CHARS: usize = 24;

    let mut chars = name.chars();
    let shortened: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{shortened}…")
    } else {
        name.to_string()
    }
}

fn voice_participants_sidebar<'a>(
    channel: &'a Channel,
    voice: SidebarVoiceContext<'a>,
) -> iced::widget::Column<'a, AppMessage> {
    let participants = voice.presence.participants(channel.id);
    let mut list = iced::widget::Column::new().spacing(2).push(
        row![
            text("  ").width(14),
            voice_connection_button(channel, voice.connection)
        ]
        .spacing(2),
    );

    for participant in participants {
        let suffix = if voice.presence.is_speaking(channel.id, participant.user_id) {
            " (speaking)"
        } else {
            ""
        };
        list = list.push(
            row![
                text("  •").color(design::color::TEXT_MUTED),
                avatar_view(
                    voice.avatars.cache,
                    voice.avatars.server_url,
                    participant.user_id,
                    &participant.username,
                    None,
                    participant.avatar_url.as_deref(),
                    18,
                ),
                text(format!("{}{}", participant.username, suffix))
                    .size(13)
                    .color(design::color::TEXT_MUTED),
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

    let button = button.style(design::button_style::primary);

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
) -> Element<'a, AppMessage> {
    let Some(channel) =
        selected_channel_id.and_then(|id| channels.iter().find(|channel| channel.id == id))
    else {
        return container(text("No text channels are available yet."))
            .padding(design::spacing::LG)
            .width(Fill)
            .height(Fill)
            .style(design::container_style::main_surface)
            .into();
    };

    match channel.kind {
        ChannelKind::Text => text_channel_view(channel, text_channel).into(),
        ChannelKind::Voice => voice_channel_view(
            channel,
            voice_presence,
            voice_connection,
            avatar_images,
            server_url,
        )
        .into(),
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
    let action_card = voice_action_card(channel, voice_presence, voice_connection);
    let mut participant_list = column![text("Connected participants").size(18)]
        .spacing(design::spacing::SM)
        .width(Fill);

    if participants.is_empty() {
        participant_list = participant_list.push(text("No one is connected."));
    } else {
        for participant in participants {
            participant_list = participant_list.push(voice_participant_row(
                participant,
                voice_presence.is_speaking(channel.id, participant.user_id),
                avatar_images,
                server_url,
            ));
        }
    }

    column![
        container(
            column![
                text(format!("🔊 {}", channel.name)).size(24),
                text("Voice channel")
                    .size(13)
                    .color(design::color::MAIN_TEXT_MUTED),
            ]
            .spacing(design::spacing::XS),
        )
        .padding([design::spacing::MD, design::spacing::LG])
        .width(Fill)
        .style(design::container_style::main_surface),
        scrollable(
            column![
                container(action_card)
                    .padding(design::spacing::LG)
                    .width(Fill)
                    .style(design::container_style::main_surface),
                container(participant_list)
                    .padding(design::spacing::LG)
                    .width(Fill)
                    .style(design::container_style::main_surface),
            ]
            .spacing(design::spacing::MD)
        )
        .height(Fill)
        .width(Fill),
        container(voice_controls(channel, voice_connection))
            .padding(design::spacing::LG)
            .width(Fill)
            .style(design::container_style::main_surface),
    ]
    .spacing(design::spacing::MD)
    .height(Fill)
    .width(Fill)
}

fn voice_action_card<'a>(
    channel: &'a Channel,
    voice_presence: &'a VoicePresenceState,
    voice_connection: &'a VoiceConnectionState,
) -> iced::widget::Column<'a, AppMessage> {
    let mut card = column![
        text("LiveKit voice").size(18),
        text("Native LiveKit voice is available for this channel.")
            .color(design::color::MAIN_TEXT_MUTED),
        voice_connection_button(channel, voice_connection),
    ]
    .spacing(design::spacing::SM)
    .width(Fill);

    if let Some(message) = voice_connection_message_for_channel(channel.id, voice_connection) {
        card = card.push(text(message).color(design::color::MAIN_TEXT_MUTED));
    }
    if let Some(message) = voice_presence.message() {
        card = card.push(text(message).color(design::color::MAIN_TEXT_MUTED));
    }
    if voice_connection
        .error
        .as_ref()
        .is_some_and(|error| error.channel_id.is_none() || error.channel_id == Some(channel.id))
    {
        card = card.push(
            button("Retry voice connection").on_press(AppMessage::VoiceJoinPressed(channel.id)),
        );
    }

    card
}

fn voice_connection_message_for_channel(
    channel_id: Id,
    voice_connection: &VoiceConnectionState,
) -> Option<String> {
    if let Some(error) = &voice_connection.error
        && (error.channel_id.is_none() || error.channel_id == Some(channel_id))
    {
        return Some(format!("Voice connection error: {}", error.message));
    }

    if voice_connection.target_channel_id() == Some(channel_id) {
        return voice_connection.message();
    }

    if voice_connection.has_active_connection() {
        return Some(
            "You are connected to another voice channel. Use Switch voice to move here."
                .to_string(),
        );
    }

    None
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
    }
    .style(design::button_style::secondary);
    let deafen_button = if voice_connection.is_connected_to(channel.id) {
        button(deafen_label).on_press(deafen_message)
    } else {
        button(deafen_label)
    }
    .style(design::button_style::secondary);

    row![text("Voice controls").size(18), mute_button, deafen_button]
        .spacing(design::spacing::SM)
        .align_y(iced::Alignment::Center)
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
    let mut message_body = column![].spacing(design::spacing::MD);

    if let Some(message) = text_channel.external_link_status.message() {
        message_body = message_body.push(text(message).color(design::color::MAIN_TEXT_MUTED));
    }

    match text_channel.history {
        MessageHistoryState::NotLoaded => {
            message_body = message_body.push(text("No message history loaded."));
        }
        MessageHistoryState::Loading { .. } => {
            message_body = message_body.push(text("Loading message history…"));
        }
        MessageHistoryState::Failed { message, .. } => {
            message_body = message_body
                .push(text("Could not load message history."))
                .push(text(message))
                .push(button("Retry messages").on_press(AppMessage::RetryMessageHistoryPressed));
        }
        MessageHistoryState::Loaded { messages, .. } => {
            if messages.is_empty() {
                message_body = message_body.push(text("No messages yet."));
            } else {
                for message in messages {
                    message_body = message_body.push(message_row(
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
        message_body = message_body.push(text(message).color(design::color::MAIN_TEXT_MUTED));
    }

    let mut composer = column![message_composer(
        text_channel.draft,
        text_channel.emoji_picker,
        text_channel.send_status,
    )]
    .spacing(design::spacing::SM);

    if let Some(message) = text_channel.send_status.message() {
        composer = composer.push(text(message).color(design::color::MAIN_TEXT_MUTED));
    }

    column![
        container(
            column![
                text(format!("# {}", channel.name)).size(24),
                text("Text channel")
                    .size(13)
                    .color(design::color::MAIN_TEXT_MUTED),
            ]
            .spacing(design::spacing::XS),
        )
        .padding([design::spacing::MD, design::spacing::LG])
        .width(Fill)
        .style(design::container_style::main_surface),
        scrollable(
            container(message_body)
                .padding(design::spacing::LG)
                .width(Fill)
        )
        .height(Fill)
        .width(Fill),
        container(composer)
            .padding([design::spacing::MD, design::spacing::LG])
            .width(Fill)
            .style(design::container_style::composer_bar),
    ]
    .spacing(design::spacing::SM)
    .height(Fill)
    .width(Fill)
}

fn message_composer<'a>(
    draft: &'a str,
    emoji_picker: &'a EmojiPickerState,
    send_status: &'a SendMessageStatus,
) -> iced::widget::Column<'a, AppMessage> {
    let input = text_input("Message", draft)
        .id(COMPOSER_INPUT_ID)
        .on_input(AppMessage::DraftEdited)
        .padding(12)
        .width(Fill)
        .style(design::text_input_style::composer);
    let emoji_button_label = if emoji_picker.is_open { "✕" } else { "😊" };
    let emoji_button = button(text(emoji_button_label).size(18))
        .on_press(AppMessage::ToggleEmojiPickerPressed)
        .padding(10)
        .width(44)
        .style(design::button_style::composer_secondary);
    let send_button = match send_status {
        SendMessageStatus::Sending => button("Sending…"),
        SendMessageStatus::Idle | SendMessageStatus::Failed(_) => {
            button("Send").on_press(AppMessage::SendMessagePressed)
        }
    }
    .padding(10)
    .style(design::button_style::primary);
    let controls = row![input, emoji_button, send_button]
        .spacing(design::spacing::SM)
        .width(Fill);
    let mut composer = column![].spacing(design::spacing::SM).width(Fill);

    if emoji_picker.is_open {
        composer = composer.push(
            container(emoji_picker_view(emoji_picker))
                .padding(design::spacing::MD)
                .width(Fill)
                .style(design::container_style::emoji_picker),
        );
    }

    composer.push(controls)
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
        40,
    );
    let visibility = message_action_visibility(current_user_id, message);
    let author_label = text(author)
        .font(Font {
            weight: font::Weight::Bold,
            ..Font::default()
        })
        .color(design::color::MAIN_TEXT);

    if let Some(editing) = actions
        .editing
        .as_ref()
        .filter(|editing| editing.message_id == message.id)
    {
        let input = text_input("Edit message", &editing.draft)
            .on_input(AppMessage::EditMessageDraftEdited)
            .padding(8)
            .width(Fill)
            .style(design::text_input_style::composer);
        let save_button = if editing.status.is_saving() {
            button("Saving…")
        } else {
            button("Save edit").on_press(AppMessage::SaveMessageEditPressed)
        }
        .style(design::button_style::primary);
        let cancel_button = button("Cancel")
            .on_press(AppMessage::CancelMessageEditPressed)
            .style(design::button_style::subtle_link);
        let mut content = column![
            author_label,
            row![input, save_button, cancel_button]
                .spacing(design::spacing::SM)
                .width(Fill),
        ]
        .spacing(design::spacing::SM)
        .width(Fill);

        if let Some(message) = editing.status.message() {
            content = content.push(text(message).color(design::color::MAIN_TEXT_MUTED));
        }

        return container(
            row![avatar, content]
                .spacing(design::spacing::MD)
                .width(Fill),
        )
        .padding([design::spacing::SM, design::spacing::MD])
        .width(Fill)
        .style(design::container_style::message_row)
        .into();
    }

    let mut meta = row![author_label].spacing(design::spacing::XS);
    if visibility.has_any_action() {
        let edit_button = if actions.is_deleting(message.id) {
            button("Edit")
        } else {
            button("Edit").on_press(AppMessage::EditMessagePressed(message.id))
        }
        .padding([2, 4])
        .style(design::button_style::subtle_link);
        let delete_button = if actions.is_deleting(message.id) {
            button("Deleting…")
        } else {
            button("Delete").on_press(AppMessage::DeleteMessagePressed(message.id))
        }
        .padding([2, 4])
        .style(design::button_style::subtle_link);

        meta = meta.push(text("·").color(design::color::MAIN_TEXT_MUTED));
        meta = meta.push(edit_button).push(delete_button);
    }

    let mut content = column![meta, message_text(message)]
        .spacing(design::spacing::XS)
        .width(Fill);

    if !message.suppress_embeds && !message.embeds.is_empty() {
        content = content.push(message_embeds(
            message,
            visibility,
            actions,
            embed_images,
            server_url,
        ));
    }

    if let Some(message) = actions.delete_message(message.id) {
        content = content.push(text(message).color(design::color::MAIN_TEXT_MUTED));
    }

    if let Some(message) = actions.suppress_embeds_message(message.id) {
        content = content.push(text(message).color(design::color::MAIN_TEXT_MUTED));
    }

    container(
        row![avatar, content]
            .spacing(design::spacing::MD)
            .width(Fill),
    )
    .padding([design::spacing::SM, design::spacing::MD])
    .width(Fill)
    .style(design::container_style::message_row)
    .into()
}

fn message_embeds<'a>(
    message: &'a Message,
    visibility: MessageActionVisibility,
    actions: &'a MessageActionState,
    embed_images: &'a EmbedImageCache,
    server_url: &'a str,
) -> iced::widget::Column<'a, AppMessage> {
    let mut embeds = iced::widget::Column::new()
        .spacing(design::spacing::SM)
        .width(Fill);
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
        text(embed_site_label(embed))
            .size(12)
            .color(design::color::MAIN_TEXT_MUTED),
        button(text(title))
            .on_press(AppMessage::OpenExternalUrlRequested(embed.url.clone()))
            .padding([2, 0])
            .style(design::button_style::subtle_link),
    ]
    .spacing(design::spacing::XS)
    .width(Fill);

    if mode == EmbedRenderMode::ExternalOpenCard {
        body = body
            .push(text(
                "This preview requires embedded web content. Open it externally to view it.",
            ))
            .push(
                button("Open externally")
                    .on_press(AppMessage::OpenExternalUrlRequested(embed.url.clone()))
                    .style(design::button_style::subtle_link),
            );
    }

    if let Some(description) = embed
        .description
        .as_deref()
        .filter(|description| !description.trim().is_empty())
    {
        body = body.push(
            text(description)
                .color(design::color::MAIN_TEXT)
                .width(Fill),
        );
    }

    if let Some(preview) = embed_preview_image(embed, embed_images, server_url) {
        body = body.push(preview);
    }

    if mode == EmbedRenderMode::NativeImagePreview {
        body = body.push(
            button("Open image source")
                .on_press(AppMessage::OpenExternalUrlRequested(embed.url.clone()))
                .style(design::button_style::subtle_link),
        );
    }

    let mut card = column![body].spacing(4);

    if can_suppress {
        let suppress_button = if is_suppressing {
            button("Suppressing embeds…")
        } else {
            button("Suppress embeds")
                .on_press(AppMessage::SuppressEmbedsPressed(message_id))
                .style(design::button_style::subtle_link)
        };
        card = card.push(suppress_button);
    }

    container(card)
        .padding(design::spacing::MD)
        .width(480)
        .style(design::container_style::embed_card)
        .into()
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
                .width(456)
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

    Some(text(label).color(design::color::MAIN_TEXT_MUTED).into())
}

fn message_author(message: &Message) -> &str {
    message
        .display_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&message.username)
}

fn message_text<'a>(message: &'a Message) -> Element<'a, AppMessage> {
    let segments = parse_message_text(&message.text);
    let has_links = segments
        .iter()
        .any(|segment| matches!(segment, MessageTextSegment::Link { .. }));

    if !has_links {
        return text(&message.text)
            .color(design::color::MAIN_TEXT)
            .width(Fill)
            .into();
    }

    let mut spans: Vec<iced::widget::text::Span<'static, String>> = Vec::new();

    for segment in segments {
        match segment {
            MessageTextSegment::Text(text) => spans.push(span(text)),
            MessageTextSegment::Link { text, url } => spans.push(
                span(text)
                    .link(url)
                    .underline(true)
                    .color(design::color::LINK),
            ),
        }
    }

    rich_text(spans)
        .width(Fill)
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
    let button = if status.is_submitting() {
        button("Log in")
    } else {
        button("Log in").on_press(AppMessage::LoginPressed)
    };

    button.style(design::button_style::primary)
}

fn register_button<'a>(status: &'a AuthStatus) -> iced::widget::Button<'a, AppMessage> {
    let button = if status.is_submitting() {
        button("Register")
    } else {
        button("Register").on_press(AppMessage::RegisterPressed)
    };

    button.style(design::button_style::secondary)
}

fn save_server_url_button<'a>(status: &'a ServerUrlStatus) -> iced::widget::Button<'a, AppMessage> {
    let button = match status {
        ServerUrlStatus::Saving => button("Saving server URL…"),
        _ => button("Save server URL").on_press(AppMessage::SaveServerUrlRequested),
    };

    button.style(design::button_style::secondary)
}
