#![allow(clippy::expect_used)]

use hamlet_client_iced::api::{ApiCall, ApiError};
use hamlet_client_iced::app::{AppEffect, AppMessage, BootStatus, ChannelMoveDirection, Route};
use hamlet_client_iced::auth::{
    AuthAction, AuthRequest, AuthSession, AuthStatus, AvatarDeleteRequest, AvatarUpdateStatus,
    AvatarUploadRequest, ChannelListState, ChannelReorderState, CreateChannelRequest,
    CreateChannelStatus, DeleteMessageRequest, EditMessageRequest, LogoutRequest, LogoutStatus,
    MessageDeleteStatus, MessageEditState, MessageEditStatus, MessageHistoryState,
    MessageSuppressStatus, ProfileUpdateRequest, ProfileUpdateStatus, ReorderChannelsRequest,
    SendMessageStatus, ServerUrlStatus, SessionRestoreRequest, SuppressMessageEmbedsRequest,
    TYPING_EXPIRY_MS, TYPING_PING_INTERVAL_MS, TYPING_SWEEP_MS, TypingPingRequest,
    VoiceConnectionStatus, VoiceParticipantsRequest, VoicePreferenceStatus, VoicePresenceStatus,
    VoiceSpeakingRequest, VoiceTokenRequest,
};
use hamlet_client_iced::emoji::{EmojiPickerFocusTarget, EmojiPickerNavigation, search_emoji};
use hamlet_client_iced::protocol::{
    BroadcastEvent, Channel, ChannelKind, Embed, Message, MessageDeletedEvent,
    MessageEmbedsUpdatedEvent, User, UserTypingEvent, VoiceParticipantSpeakingEvent,
};
use hamlet_client_iced::realtime::{RealtimeCall, RealtimeConnectionState, RealtimeEvent};
use hamlet_client_iced::storage::{DEFAULT_SERVER_URL, Preferences, VoiceDevicePreferences};
use hamlet_client_iced::test_support::fake_api::FakeApi;
use hamlet_client_iced::test_support::fake_storage::FakeStorage;
use hamlet_client_iced::test_support::fixtures::{
    dev_user, general_channel, message, voice_channel, voice_participant,
};
use hamlet_client_iced::test_support::harness::ReducerHarness;
use hamlet_client_iced::voice::{VoiceCommand, VoiceError, VoiceEvent, VoiceJoinRequest};

#[test]
fn boot_starts_in_signed_out_route_and_loads_preferences() {
    let harness = ReducerHarness::boot();

    assert_eq!(harness.state.boot_status, BootStatus::LoadingPreferences);
    assert!(harness.state.is_loading_preferences());
    assert_eq!(harness.pending_effects(), &[AppEffect::LoadPreferences]);
}

#[test]
fn loaded_preferences_update_the_login_server_url() {
    let storage = FakeStorage::new(
        Preferences::with_server_url("https://chat.example.test").unwrap_or_default(),
    );
    let mut harness = ReducerHarness::boot_with_storage(storage);

    harness.run_all_effects();

    assert_eq!(harness.state.boot_status, BootStatus::Ready);
    assert_eq!(
        harness.state.signed_out.server_url,
        "https://chat.example.test"
    );
    assert_eq!(
        harness.state.signed_out.server_url_status,
        ServerUrlStatus::Clean
    );
}

#[test]
fn editing_and_saving_server_url_persists_across_storage_reload() {
    let storage = FakeStorage::default();
    let mut harness = ReducerHarness::boot_with_storage(storage.clone());
    harness.run_all_effects();

    harness.dispatch(AppMessage::ServerUrlEdited(
        "http://127.0.0.1:4040/".to_string(),
    ));
    let effects = harness.dispatch(AppMessage::SaveServerUrlRequested);

    assert_eq!(
        effects,
        vec![AppEffect::SavePreferences(
            Preferences::with_server_url("http://127.0.0.1:4040").unwrap_or_default()
        )]
    );

    harness.run_all_effects();

    assert_eq!(
        harness.state.signed_out.server_url_status,
        ServerUrlStatus::Saved
    );
    assert_eq!(
        storage.saved_preferences().unwrap_or_default(),
        vec![Preferences::with_server_url("http://127.0.0.1:4040").unwrap_or_default()]
    );

    let mut restarted = ReducerHarness::boot_with_storage(storage);
    restarted.run_all_effects();

    assert_eq!(
        restarted.state.signed_out.server_url,
        "http://127.0.0.1:4040"
    );
}

#[test]
fn invalid_server_url_is_recoverable_without_saving() {
    let mut harness = ReducerHarness::boot();
    harness.run_all_effects();

    harness.dispatch(AppMessage::ServerUrlEdited("not a url".to_string()));
    let effects = harness.dispatch(AppMessage::SaveServerUrlRequested);

    assert!(effects.is_empty());
    assert!(matches!(
        harness.state.signed_out.server_url_status,
        ServerUrlStatus::Failed(_)
    ));
}

#[test]
fn preference_load_failure_still_shows_signed_out_shell() {
    let storage = FakeStorage::default();
    assert!(storage.fail_load("disk unavailable").is_ok());
    let mut harness = ReducerHarness::boot_with_storage(storage);

    harness.run_all_effects();

    assert_eq!(harness.state.boot_status, BootStatus::Ready);
    assert_eq!(
        harness.state.signed_out.server_url,
        hamlet_client_iced::storage::DEFAULT_SERVER_URL
    );
    assert!(matches!(
        harness.state.signed_out.server_url_status,
        ServerUrlStatus::Failed(_)
    ));
}

#[test]
fn login_success_reaches_signed_in_state_and_reuses_configured_server_url() {
    let api = FakeApi::default();
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    harness.run_all_effects();

    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    let effects = harness.dispatch(AppMessage::LoginPressed);

    assert_eq!(
        effects,
        vec![AppEffect::Authenticate(AuthRequest {
            action: AuthAction::Login,
            server_url: DEFAULT_SERVER_URL.to_string(),
            username: "baipas".to_string(),
            password: "password".to_string(),
            email: None,
        })]
    );
    assert_eq!(
        harness.state.signed_out.auth_status,
        AuthStatus::Submitting(AuthAction::Login)
    );

    harness.run_all_effects();

    assert_eq!(harness.state.route, Route::SignedIn);
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .map(|state| &state.user.username),
        Some(&"baipas".to_string())
    );
    assert_eq!(harness.state.signed_out.password, "");
    assert_eq!(
        api.calls().unwrap_or_default(),
        vec![
            ApiCall::SetBaseUrl(DEFAULT_SERVER_URL.to_string()),
            ApiCall::Login {
                username: "baipas".to_string()
            },
            ApiCall::SetBaseUrl(DEFAULT_SERVER_URL.to_string()),
            ApiCall::SetSessionToken { present: true },
            ApiCall::ListChannels,
            ApiCall::SetBaseUrl(DEFAULT_SERVER_URL.to_string()),
            ApiCall::SetSessionToken { present: true },
            ApiCall::GetMessages {
                channel_id: general_channel().id,
            },
            ApiCall::SetBaseUrl(DEFAULT_SERVER_URL.to_string()),
            ApiCall::SetSessionToken { present: true },
            ApiCall::ListVoiceParticipants {
                channel_id: voice_channel().id,
            },
        ]
    );
}

#[test]
fn register_success_reaches_signed_in_state() {
    let api = FakeApi::default();
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    harness.run_all_effects();

    let mut user = dev_user();
    user.username = "new-user".to_string();
    assert!(
        api.set_next_register_result(Ok(AuthSession::new(
            user.clone(),
            Some("registered-session".to_string())
        )))
        .is_ok()
    );

    harness.dispatch(AppMessage::UsernameEdited("new-user".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("secret".to_string()));
    let effects = harness.dispatch(AppMessage::RegisterPressed);

    assert_eq!(
        effects,
        vec![AppEffect::Authenticate(AuthRequest {
            action: AuthAction::Register,
            server_url: DEFAULT_SERVER_URL.to_string(),
            username: "new-user".to_string(),
            password: "secret".to_string(),
            email: None,
        })]
    );

    harness.run_all_effects();

    assert_eq!(harness.state.route, Route::SignedIn);
    assert_eq!(
        harness.state.signed_in.as_ref().map(|state| &state.user),
        Some(&user)
    );
    assert_eq!(
        api.calls().unwrap_or_default(),
        vec![
            ApiCall::SetBaseUrl(DEFAULT_SERVER_URL.to_string()),
            ApiCall::Register {
                username: "new-user".to_string()
            },
            ApiCall::SetBaseUrl(DEFAULT_SERVER_URL.to_string()),
            ApiCall::SetSessionToken { present: true },
            ApiCall::ListChannels,
            ApiCall::SetBaseUrl(DEFAULT_SERVER_URL.to_string()),
            ApiCall::SetSessionToken { present: true },
            ApiCall::GetMessages {
                channel_id: general_channel().id,
            },
            ApiCall::SetBaseUrl(DEFAULT_SERVER_URL.to_string()),
            ApiCall::SetSessionToken { present: true },
            ApiCall::ListVoiceParticipants {
                channel_id: voice_channel().id,
            },
        ]
    );
}

#[test]
fn auth_failures_remain_signed_out_with_clear_errors() {
    let cases = [
        (
            ApiError::InvalidCredentials,
            "Invalid username or password.",
        ),
        (ApiError::UsernameTaken, "That username is already taken."),
        (
            ApiError::InvalidRequest("invalid request".to_string()),
            "Invalid input. Username and password are required.",
        ),
        (
            ApiError::Unreachable {
                server_url: DEFAULT_SERVER_URL.to_string(),
                message: "connection refused".to_string(),
            },
            "Could not reach the Hamlet server at http://localhost:3030.",
        ),
    ];

    for (error, expected_message) in cases {
        let api = FakeApi::default();
        assert!(api.set_next_login_result(Err(error)).is_ok());
        let mut harness = ReducerHarness::boot_with_api(api);
        harness.run_all_effects();

        harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
        harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
        harness.dispatch(AppMessage::LoginPressed);
        harness.run_all_effects();

        assert_eq!(harness.state.route, Route::SignedOut);
        assert_eq!(
            harness.state.signed_out.auth_status,
            AuthStatus::Failed(expected_message.to_string())
        );
    }
}

#[test]
fn empty_credentials_and_invalid_server_url_do_not_submit_auth_effects() {
    let mut harness = ReducerHarness::boot();
    harness.run_all_effects();

    assert!(harness.dispatch(AppMessage::LoginPressed).is_empty());
    assert_eq!(
        harness.state.signed_out.auth_status,
        AuthStatus::Failed("Username and password are required.".to_string())
    );

    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::ServerUrlEdited("not a url".to_string()));

    assert!(harness.dispatch(AppMessage::LoginPressed).is_empty());
    assert!(matches!(
        harness.state.signed_out.server_url_status,
        ServerUrlStatus::Failed(_)
    ));
    assert!(matches!(
        harness.state.signed_out.auth_status,
        AuthStatus::Failed(_)
    ));
}

#[test]
fn duplicate_submit_while_auth_is_pending_is_ignored() {
    let mut harness = ReducerHarness::boot();
    harness.run_all_effects();

    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));

    assert_eq!(harness.dispatch(AppMessage::LoginPressed).len(), 1);
    assert!(harness.dispatch(AppMessage::LoginPressed).is_empty());
}

#[cfg(debug_assertions)]
#[test]
fn debug_dev_credentials_shortcut_submits_seeded_login() {
    let mut harness = ReducerHarness::boot();
    harness.run_all_effects();

    let effects = harness.dispatch(AppMessage::UseDevCredentials);

    assert_eq!(harness.state.signed_out.username, "baipas");
    assert_eq!(harness.state.signed_out.password, "password");
    assert_eq!(
        effects,
        vec![AppEffect::Authenticate(AuthRequest {
            action: AuthAction::Login,
            server_url: DEFAULT_SERVER_URL.to_string(),
            username: "baipas".to_string(),
            password: "password".to_string(),
            email: None,
        })]
    );
}

#[test]
fn startup_restores_valid_stored_session() {
    let preferences = Preferences::with_server_url_and_session_token(
        DEFAULT_SERVER_URL,
        Some("stored-session".to_string()),
    )
    .unwrap_or_default();
    let storage = FakeStorage::new(preferences);
    let api = FakeApi::default();
    let mut harness = ReducerHarness::boot_with_storage_and_api(storage, api.clone());

    assert!(harness.run_next_effect());

    assert_eq!(harness.state.boot_status, BootStatus::RestoringSession);
    assert_eq!(
        harness.pending_effects(),
        &[AppEffect::RestoreSession(SessionRestoreRequest {
            server_url: DEFAULT_SERVER_URL.to_string(),
            session_token: "stored-session".to_string(),
        })]
    );

    harness.run_all_effects();

    assert_eq!(harness.state.route, Route::SignedIn);
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .and_then(|state| state.session_token.as_deref()),
        Some("stored-session")
    );
    assert_eq!(
        api.calls().unwrap_or_default(),
        vec![
            ApiCall::SetBaseUrl(DEFAULT_SERVER_URL.to_string()),
            ApiCall::SetSessionToken { present: true },
            ApiCall::GetMe,
            ApiCall::SetBaseUrl(DEFAULT_SERVER_URL.to_string()),
            ApiCall::SetSessionToken { present: true },
            ApiCall::ListChannels,
            ApiCall::SetBaseUrl(DEFAULT_SERVER_URL.to_string()),
            ApiCall::SetSessionToken { present: true },
            ApiCall::GetMessages {
                channel_id: general_channel().id,
            },
            ApiCall::SetBaseUrl(DEFAULT_SERVER_URL.to_string()),
            ApiCall::SetSessionToken { present: true },
            ApiCall::ListVoiceParticipants {
                channel_id: voice_channel().id,
            },
        ]
    );
}

#[test]
fn startup_invalid_stored_session_returns_to_login_and_clears_session() {
    let preferences = Preferences::with_server_url_and_session_token(
        DEFAULT_SERVER_URL,
        Some("expired-session".to_string()),
    )
    .unwrap_or_default();
    let storage = FakeStorage::new(preferences);
    let api = FakeApi::default();
    assert!(api.set_next_me_result(Err(ApiError::Unauthorized)).is_ok());
    let mut harness = ReducerHarness::boot_with_storage_and_api(storage.clone(), api);

    harness.run_all_effects();

    assert_eq!(harness.state.route, Route::SignedOut);
    assert_eq!(harness.state.boot_status, BootStatus::Ready);
    assert!(
        harness
            .state
            .signed_out
            .notice
            .as_deref()
            .is_some_and(|notice| {
                notice.contains("Could not restore your saved session")
                    && notice.contains("Please log in again")
            })
    );
    assert_eq!(
        storage.saved_preferences().unwrap_or_default(),
        vec![Preferences::with_server_url(DEFAULT_SERVER_URL).unwrap_or_default()]
    );
}

#[test]
fn logout_calls_server_clears_signed_in_state_and_removes_saved_session() {
    let storage = FakeStorage::default();
    let api = FakeApi::default();
    let mut harness = ReducerHarness::boot_with_storage_and_api(storage.clone(), api.clone());
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();

    let effects = harness.dispatch(AppMessage::LogoutPressed);

    assert_eq!(
        effects,
        vec![
            AppEffect::StopRealtime,
            AppEffect::Logout(LogoutRequest {
                server_url: DEFAULT_SERVER_URL.to_string(),
                session_token: Some("fake-session-token".to_string()),
            })
        ]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .map(|state| &state.logout_status),
        Some(&LogoutStatus::LoggingOut)
    );

    harness.run_all_effects();

    assert_eq!(harness.state.route, Route::SignedOut);
    assert_eq!(
        harness.state.signed_out.notice.as_deref(),
        Some("Logged out.")
    );
    assert_eq!(
        storage
            .saved_preferences()
            .unwrap_or_default()
            .last()
            .cloned(),
        Some(Preferences::with_server_url(DEFAULT_SERVER_URL).unwrap_or_default())
    );
    assert!(api.calls().unwrap_or_default().contains(&ApiCall::Logout));
}

#[test]
fn logout_failure_still_clears_local_signed_in_state() {
    let storage = FakeStorage::default();
    let api = FakeApi::default();
    let mut harness = ReducerHarness::boot_with_storage_and_api(storage.clone(), api.clone());
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();
    assert!(
        api.fail_next(ApiError::Unreachable {
            server_url: DEFAULT_SERVER_URL.to_string(),
            message: "connection refused".to_string(),
        })
        .is_ok()
    );

    harness.dispatch(AppMessage::LogoutPressed);
    harness.run_all_effects();

    assert_eq!(harness.state.route, Route::SignedOut);
    assert!(
        harness
            .state
            .signed_out
            .notice
            .as_deref()
            .is_some_and(|notice| {
                notice.contains("Logged out locally")
                    && notice.contains("Could not reach the Hamlet server")
            })
    );
    assert_eq!(
        storage
            .saved_preferences()
            .unwrap_or_default()
            .last()
            .cloned(),
        Some(Preferences::with_server_url(DEFAULT_SERVER_URL).unwrap_or_default())
    );
}

#[test]
fn settings_open_close_syncs_current_profile_state() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);

    assert!(harness.dispatch(AppMessage::OpenSettingsPressed).is_empty());

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert!(signed_in.profile_settings.is_open);
    assert_eq!(signed_in.user.username, "baipas");
    assert_eq!(signed_in.profile_settings.display_name_input, "");
    assert_eq!(signed_in.profile_settings.status, ProfileUpdateStatus::Idle);

    harness.dispatch(AppMessage::ProfileDisplayNameEdited("Draft".to_string()));
    assert!(
        harness
            .dispatch(AppMessage::CloseSettingsPressed)
            .is_empty()
    );

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert!(!signed_in.profile_settings.is_open);
    assert_eq!(signed_in.profile_settings.status, ProfileUpdateStatus::Idle);

    harness.dispatch(AppMessage::OpenSettingsPressed);

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert!(signed_in.profile_settings.is_open);
    assert_eq!(signed_in.profile_settings.display_name_input, "");
}

#[test]
fn display_name_update_refreshes_profile_and_visible_messages() {
    let api = FakeApi::default();
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    sign_in(&mut harness);

    harness.dispatch(AppMessage::OpenSettingsPressed);
    harness.dispatch(AppMessage::ProfileDisplayNameEdited(
        "  Captain Hamlet  ".to_string(),
    ));
    let effects = harness.dispatch(AppMessage::SaveDisplayNamePressed);

    assert_eq!(
        effects,
        vec![AppEffect::UpdateProfile(ProfileUpdateRequest {
            server_url: DEFAULT_SERVER_URL.to_string(),
            session_token: Some("fake-session-token".to_string()),
            display_name: Some("Captain Hamlet".to_string()),
        })]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .profile_settings
            .status,
        ProfileUpdateStatus::Saving
    );

    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.user.display_name.as_deref(),
        Some("Captain Hamlet")
    );
    assert_eq!(signed_in.display_name(), "Captain Hamlet");
    assert_eq!(
        signed_in.profile_settings.display_name_input,
        "Captain Hamlet"
    );
    assert_eq!(
        signed_in.profile_settings.status,
        ProfileUpdateStatus::Saved
    );
    assert!(visible_messages(&harness).iter().any(|message| {
        message.user_id == signed_in.user.id
            && message.display_name.as_deref() == Some("Captain Hamlet")
    }));
    assert!(
        api.calls()
            .unwrap_or_default()
            .contains(&ApiCall::UpdateProfile {
                display_name: Some("Captain Hamlet".to_string()),
            })
    );
}

#[test]
fn clearing_display_name_falls_back_to_username_and_refreshes_messages() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);

    harness.dispatch(AppMessage::OpenSettingsPressed);
    harness.dispatch(AppMessage::ProfileDisplayNameEdited("Captain".to_string()));
    harness.dispatch(AppMessage::SaveDisplayNamePressed);
    harness.run_all_effects();

    let effects = harness.dispatch(AppMessage::ClearDisplayNamePressed);

    assert_eq!(
        effects,
        vec![AppEffect::UpdateProfile(ProfileUpdateRequest {
            server_url: DEFAULT_SERVER_URL.to_string(),
            session_token: Some("fake-session-token".to_string()),
            display_name: None,
        })]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .profile_settings
            .display_name_input,
        ""
    );

    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.user.display_name, None);
    assert_eq!(signed_in.display_name(), "baipas");
    assert_eq!(
        signed_in.profile_settings.status,
        ProfileUpdateStatus::Saved
    );
    assert!(
        visible_messages(&harness).iter().any(|message| {
            message.user_id == signed_in.user.id && message.display_name.is_none()
        })
    );
}

#[test]
fn display_name_validation_failure_does_not_submit_or_discard_input() {
    let api = FakeApi::default();
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    sign_in(&mut harness);
    let too_long = "x".repeat(65);

    harness.dispatch(AppMessage::OpenSettingsPressed);
    harness.dispatch(AppMessage::ProfileDisplayNameEdited(too_long.clone()));
    let effects = harness.dispatch(AppMessage::SaveDisplayNamePressed);

    assert!(effects.is_empty());
    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.profile_settings.display_name_input, too_long);
    assert_eq!(
        signed_in.profile_settings.status,
        ProfileUpdateStatus::Failed("Display name must be 64 characters or fewer.".to_string())
    );
    assert!(
        !api.calls()
            .unwrap_or_default()
            .iter()
            .any(|call| matches!(call, ApiCall::UpdateProfile { .. }))
    );
}

#[test]
fn display_name_transport_failure_keeps_settings_input_recoverable() {
    let api = FakeApi::default();
    assert!(
        api.set_next_update_profile_result(Err(ApiError::Unreachable {
            server_url: DEFAULT_SERVER_URL.to_string(),
            message: "connection refused".to_string(),
        }))
        .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);

    harness.dispatch(AppMessage::OpenSettingsPressed);
    harness.dispatch(AppMessage::ProfileDisplayNameEdited(
        "Still Here".to_string(),
    ));
    harness.dispatch(AppMessage::SaveDisplayNamePressed);
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.user.display_name, None);
    assert_eq!(signed_in.profile_settings.display_name_input, "Still Here");
    assert_eq!(
        signed_in.profile_settings.status,
        ProfileUpdateStatus::Failed(
            "Could not update display name. Could not reach the Hamlet server at http://localhost:3030."
                .to_string()
        )
    );
}

#[test]
fn avatar_file_selection_uploads_and_refreshes_profile_messages_and_cache() {
    let api = FakeApi::default();
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    sign_in(&mut harness);
    harness.dispatch(AppMessage::OpenSettingsPressed);

    let effects = harness.dispatch(AppMessage::SelectAvatarPressed);
    assert_eq!(effects, vec![AppEffect::PickAvatarFile]);
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .profile_settings
            .avatar_status,
        AvatarUpdateStatus::Selecting
    );

    assert!(harness.run_next_effect());
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .profile_settings
            .avatar_status,
        AvatarUpdateStatus::Idle
    );

    let path = std::path::PathBuf::from("/tmp/avatar.webp");
    let effects = harness.dispatch(AppMessage::AvatarFileSelected(Ok(Some(path.clone()))));
    assert_eq!(
        effects,
        vec![AppEffect::UploadAvatar(AvatarUploadRequest {
            server_url: DEFAULT_SERVER_URL.to_string(),
            session_token: Some("fake-session-token".to_string()),
            path: path.clone(),
        })]
    );

    harness.run_next_effect();
    assert!(matches!(
        harness.pending_effects().last(),
        Some(AppEffect::LoadAvatarImage(request))
            if request.url == "http://localhost:3030/uploads/avatars/1.webp?v=1"
    ));
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.user.avatar_url.as_deref(),
        Some("/uploads/avatars/1.webp?v=1")
    );
    assert_eq!(
        signed_in.profile_settings.avatar_status,
        AvatarUpdateStatus::Saved
    );
    assert!(visible_messages(&harness).iter().any(|message| {
        message.user_id == signed_in.user.id
            && message.avatar_url.as_deref() == Some("/uploads/avatars/1.webp?v=1")
    }));
    assert!(
        signed_in
            .avatar_images
            .handle_for(DEFAULT_SERVER_URL, Some("/uploads/avatars/1.webp?v=1"))
            .is_some()
    );
    assert!(
        api.calls()
            .unwrap_or_default()
            .contains(&ApiCall::UploadAvatar { path })
    );
}

#[test]
fn avatar_upload_failure_keeps_existing_profile_recoverable() {
    let api = FakeApi::default();
    assert!(
        api.set_next_avatar_upload_result(Err(ApiError::Unreachable {
            server_url: DEFAULT_SERVER_URL.to_string(),
            message: "connection refused".to_string(),
        }))
        .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);

    harness.dispatch(AppMessage::OpenSettingsPressed);
    harness.dispatch(AppMessage::AvatarFileSelected(Ok(Some(
        std::path::PathBuf::from("/tmp/avatar.webp"),
    ))));
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.user.avatar_url, None);
    assert!(matches!(
        &signed_in.profile_settings.avatar_status,
        AvatarUpdateStatus::Failed(message)
            if message.contains("Could not upload avatar")
                && message.contains("Could not reach the Hamlet server")
    ));
}

#[test]
fn avatar_delete_removes_avatar_and_refreshes_visible_messages() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    let user_with_avatar = user_with_avatar(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .user
            .clone(),
        Some("/uploads/avatars/42.webp?v=1"),
    );
    harness.dispatch(AppMessage::AvatarUploaded(Ok(user_with_avatar)));
    harness.run_all_effects();

    let effects = harness.dispatch(AppMessage::DeleteAvatarPressed);
    assert_eq!(
        effects,
        vec![AppEffect::DeleteAvatar(AvatarDeleteRequest {
            server_url: DEFAULT_SERVER_URL.to_string(),
            session_token: Some("fake-session-token".to_string()),
        })]
    );
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.user.avatar_url, None);
    assert_eq!(
        signed_in.profile_settings.avatar_status,
        AvatarUpdateStatus::Saved
    );
    assert!(
        visible_messages(&harness)
            .iter()
            .any(|message| message.user_id == signed_in.user.id && message.avatar_url.is_none())
    );
}

#[test]
fn avatar_delete_failure_keeps_current_avatar() {
    let api = FakeApi::default();
    assert!(
        api.set_next_avatar_delete_result(Err(ApiError::Server {
            status: 500,
            kind: None,
            message: "boom".to_string(),
        }))
        .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);
    let avatar_url = "/uploads/avatars/42.webp?v=1";
    let user_with_avatar = user_with_avatar(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .user
            .clone(),
        Some(avatar_url),
    );
    harness.dispatch(AppMessage::AvatarUploaded(Ok(user_with_avatar)));
    harness.run_all_effects();

    harness.dispatch(AppMessage::DeleteAvatarPressed);
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.user.avatar_url.as_deref(), Some(avatar_url));
    assert!(matches!(
        &signed_in.profile_settings.avatar_status,
        AvatarUpdateStatus::Failed(message)
            if message.contains("Could not remove avatar")
                && message.contains("Server returned 500")
    ));
}

#[test]
fn signed_in_shell_loads_ordered_channels_selects_first_text_and_history() {
    let api = FakeApi::default();
    let voice = voice_channel();
    let general = general_channel();
    let history = vec![message(501, general.id, "loaded history")];
    assert!(
        api.set_channels(vec![voice.clone(), general.clone()])
            .is_ok()
    );
    assert!(api.set_messages(general.id, history.clone()).is_ok());
    let mut harness = ReducerHarness::boot_with_api(api);
    harness.run_all_effects();

    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.channels,
        ChannelListState::Loaded(vec![voice.clone(), general.clone()])
    );
    assert_eq!(signed_in.selected_channel_id, Some(general.id));
    assert_eq!(
        signed_in.message_history,
        MessageHistoryState::Loaded {
            channel_id: general.id,
            messages: history,
        }
    );
}

#[test]
fn signed_in_shell_fetches_initial_voice_participants() {
    let api = FakeApi::default();
    let voice = voice_channel();
    let participants = vec![
        voice_participant(2, voice.id, "teo"),
        voice_participant(1, voice.id, "baipas"),
    ];
    assert!(api.set_voice_participants(voice.id, participants).is_ok());
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    sign_in(&mut harness);

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.voice_presence.status, VoicePresenceStatus::Loaded);
    assert_eq!(
        signed_in
            .voice_presence
            .participants(voice.id)
            .iter()
            .map(|participant| participant.username.as_str())
            .collect::<Vec<_>>(),
        vec!["baipas", "teo"]
    );
    assert!(
        api.calls()
            .unwrap_or_default()
            .contains(&ApiCall::ListVoiceParticipants {
                channel_id: voice.id,
            })
    );
}

#[test]
fn channels_loaded_starts_voice_presence_loading_effect() {
    let api = FakeApi::default();
    let voice = voice_channel();
    let mut harness = ReducerHarness::boot_with_api(api);
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);

    while !matches!(
        harness
            .state
            .signed_in
            .as_ref()
            .map(|state| &state.channels),
        Some(ChannelListState::Loaded(_))
    ) {
        assert!(harness.run_next_effect());
    }

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.voice_presence.status,
        VoicePresenceStatus::Loading
    );
    assert!(
        harness
            .pending_effects()
            .contains(&AppEffect::LoadVoiceParticipants(
                VoiceParticipantsRequest {
                    server_url: DEFAULT_SERVER_URL.to_string(),
                    session_token: Some("fake-session-token".to_string()),
                    channel_ids: vec![voice.id],
                }
            ))
    );
}

#[test]
fn voice_participant_fetch_failure_is_visible_without_blocking_channels() {
    let api = FakeApi::default();
    assert!(
        api.set_next_voice_participants_result(Err(ApiError::Unreachable {
            server_url: DEFAULT_SERVER_URL.to_string(),
            message: "connection refused".to_string(),
        }))
        .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert!(matches!(signed_in.channels, ChannelListState::Loaded(_)));
    assert!(matches!(
        &signed_in.voice_presence.status,
        VoicePresenceStatus::Failed(message)
            if message.contains("Could not reach the Hamlet server")
    ));
    assert!(
        signed_in
            .voice_presence
            .participants(voice_channel().id)
            .is_empty()
    );
}

#[test]
fn voice_participant_join_events_update_channel_presence() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    let voice = voice_channel();
    let participant = voice_participant(2, voice.id, "teo");

    let effects = harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::VoiceParticipantJoined(participant.clone())),
        RealtimeEvent::Broadcast(BroadcastEvent::VoiceParticipantJoined(participant)),
    ]));

    assert!(effects.is_empty());
    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.voice_presence.participants(voice.id).len(), 1);
    assert_eq!(
        signed_in.voice_presence.participants(voice.id)[0].username,
        "teo"
    );
}

#[test]
fn voice_participant_leave_events_remove_channel_presence() {
    let api = FakeApi::default();
    let voice = voice_channel();
    assert!(
        api.set_voice_participants(
            voice.id,
            vec![
                voice_participant(1, voice.id, "baipas"),
                voice_participant(2, voice.id, "teo"),
            ],
        )
        .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);

    let effects = harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::VoiceParticipantLeft(
            hamlet_client_iced::protocol::VoiceParticipantLeftEvent {
                channel_id: voice.id,
                user_id: 2,
            },
        )),
    ]));

    assert!(effects.is_empty());
    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.voice_presence.participants(voice.id).len(), 1);
    assert_eq!(
        signed_in.voice_presence.participants(voice.id)[0].username,
        "baipas"
    );
}

#[test]
fn logout_clears_voice_presence_locally() {
    let api = FakeApi::default();
    let voice = voice_channel();
    assert!(
        api.set_voice_participants(voice.id, vec![voice_participant(2, voice.id, "teo")])
            .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_presence
            .participants(voice.id)
            .len(),
        1
    );

    harness.dispatch(AppMessage::LogoutPressed);

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.voice_presence.status,
        VoicePresenceStatus::NotLoaded
    );
    assert!(signed_in.voice_presence.participants(voice.id).is_empty());

    harness.run_all_effects();
    assert!(harness.state.signed_in.is_none());
}

#[test]
fn selecting_voice_channel_does_not_open_message_history() {
    let api = FakeApi::default();
    let voice = voice_channel();
    let general = general_channel();
    assert!(
        api.set_channels(vec![general.clone(), voice.clone()])
            .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();

    let effects = harness.dispatch(AppMessage::ChannelSelected(voice.id));

    assert!(effects.is_empty());
    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.selected_channel_id, Some(voice.id));
    assert_eq!(signed_in.message_history, MessageHistoryState::NotLoaded);
}

#[test]
fn voice_join_requests_token_and_sends_worker_join_command() {
    let api = FakeApi::default();
    let voice = voice_channel();
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    sign_in(&mut harness);

    let effects = harness.dispatch(AppMessage::VoiceJoinPressed(voice.id));

    assert_eq!(
        effects,
        vec![AppEffect::LoadVoiceToken(VoiceTokenRequest {
            server_url: DEFAULT_SERVER_URL.to_string(),
            session_token: Some("fake-session-token".to_string()),
            channel_id: voice.id,
        })]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_connection
            .status,
        VoiceConnectionStatus::Connecting {
            channel_id: voice.id,
        }
    );

    harness.run_all_effects();

    assert!(
        api.calls()
            .unwrap_or_default()
            .contains(&ApiCall::GetVoiceToken {
                channel_id: voice.id,
            })
    );
    assert_eq!(
        harness.voice.commands().unwrap_or_default(),
        vec![VoiceCommand::Join(VoiceJoinRequest {
            channel_id: voice.id,
            url: "ws://localhost:7880".to_string(),
            token: "fake-livekit-token".to_string(),
            room: format!("channel-{}", voice.id),
            device_preferences: VoiceDevicePreferences::default(),
        })]
    );
}

#[test]
fn voice_connected_event_marks_channel_connected() {
    let voice = voice_channel();
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    harness.dispatch(AppMessage::VoiceJoinPressed(voice.id));
    harness.run_all_effects();

    let effects = harness.dispatch(AppMessage::VoiceWorkerEvent(VoiceEvent::Connected {
        channel_id: voice.id,
        room: format!("channel-{}", voice.id),
    }));

    assert!(effects.is_empty());
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_connection
            .status,
        VoiceConnectionStatus::Connected {
            channel_id: voice.id,
            room: format!("channel-{}", voice.id),
        }
    );
}

#[test]
fn voice_mute_unmute_and_deafen_controls_send_worker_commands() {
    let voice = voice_channel();
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    connect_voice(&mut harness, voice.id);

    assert_eq!(
        harness.dispatch(AppMessage::VoiceMutePressed),
        vec![AppEffect::SendVoiceCommand(VoiceCommand::Mute)]
    );
    assert!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_connection
            .muted
    );
    harness.run_all_effects();

    assert_eq!(
        harness.dispatch(AppMessage::VoiceUnmutePressed),
        vec![AppEffect::SendVoiceCommand(VoiceCommand::Unmute)]
    );
    assert!(
        !harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_connection
            .muted
    );
    harness.run_all_effects();

    assert_eq!(
        harness.dispatch(AppMessage::VoiceDeafenPressed),
        vec![AppEffect::SendVoiceCommand(VoiceCommand::Deafen)]
    );
    assert!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_connection
            .deafened
    );
    harness.run_all_effects();

    assert_eq!(
        harness.dispatch(AppMessage::VoiceUndeafenPressed),
        vec![AppEffect::SendVoiceCommand(VoiceCommand::Undeafen)]
    );
    assert!(
        !harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_connection
            .deafened
    );
    harness.run_all_effects();

    assert_eq!(
        harness.voice.commands().unwrap_or_default(),
        vec![
            VoiceCommand::Join(VoiceJoinRequest {
                channel_id: voice.id,
                url: "ws://localhost:7880".to_string(),
                token: "fake-livekit-token".to_string(),
                room: format!("channel-{}", voice.id),
                device_preferences: VoiceDevicePreferences::default(),
            }),
            VoiceCommand::Mute,
            VoiceCommand::Unmute,
            VoiceCommand::Deafen,
            VoiceCommand::Undeafen,
        ]
    );
}

#[test]
fn speaking_events_update_indicators_post_local_state_and_clear() {
    let voice = voice_channel();
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    connect_voice(&mut harness, voice.id);

    let effects = harness.dispatch(AppMessage::VoiceWorkerEvent(VoiceEvent::SpeakingChanged {
        channel_id: voice.id,
        user_id: 1,
        speaking: true,
    }));

    assert_eq!(
        effects,
        vec![AppEffect::PostVoiceSpeaking(VoiceSpeakingRequest {
            server_url: DEFAULT_SERVER_URL.to_string(),
            session_token: Some("fake-session-token".to_string()),
            channel_id: voice.id,
            speaking: true,
        })]
    );
    assert!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_presence
            .is_speaking(voice.id, 1)
    );
    harness.run_all_effects();
    assert!(
        harness
            .api
            .calls()
            .unwrap_or_default()
            .contains(&ApiCall::PostVoiceSpeaking {
                channel_id: voice.id,
                speaking: true,
            })
    );

    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::VoiceParticipantSpeakingChanged(
            VoiceParticipantSpeakingEvent {
                channel_id: voice.id,
                user_id: 2,
                speaking: true,
            },
        )),
    ]));
    assert!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_presence
            .is_speaking(voice.id, 2)
    );

    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::VoiceParticipantSpeakingChanged(
            VoiceParticipantSpeakingEvent {
                channel_id: voice.id,
                user_id: 2,
                speaking: false,
            },
        )),
    ]));
    assert!(
        !harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_presence
            .is_speaking(voice.id, 2)
    );

    harness.dispatch(AppMessage::VoiceWorkerEvent(VoiceEvent::Disconnected {
        channel_id: Some(voice.id),
        reason: None,
    }));
    assert!(
        !harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_presence
            .is_speaking(voice.id, 1)
    );
}

#[test]
fn saved_voice_device_preferences_are_reapplied_on_restart_and_join() {
    let voice = voice_channel();
    let storage = FakeStorage::default();
    let mut harness = ReducerHarness::boot_with_storage(storage.clone());
    sign_in(&mut harness);

    harness.dispatch(AppMessage::OpenSettingsPressed);
    harness.dispatch(AppMessage::VoiceMicrophoneDeviceEdited(
        " mic-device-id ".to_string(),
    ));
    harness.dispatch(AppMessage::VoiceOutputDeviceEdited(
        " output-device-id ".to_string(),
    ));
    let effects = harness.dispatch(AppMessage::SaveVoicePreferencesPressed);

    let expected_preferences = Preferences::with_server_url_session_token_and_voice(
        DEFAULT_SERVER_URL,
        Some("fake-session-token".to_string()),
        VoiceDevicePreferences::new(
            Some("mic-device-id".to_string()),
            Some("output-device-id".to_string()),
        ),
    )
    .unwrap_or_default();
    assert_eq!(
        effects,
        vec![AppEffect::SaveVoicePreferences(
            expected_preferences.clone()
        )]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_settings
            .status,
        VoicePreferenceStatus::Saving
    );

    harness.run_all_effects();
    assert_eq!(
        storage
            .saved_preferences()
            .unwrap_or_default()
            .last()
            .cloned(),
        Some(expected_preferences.clone())
    );

    let mut restarted = ReducerHarness::boot_with_storage(storage);
    restarted.run_all_effects();
    let signed_in = restarted.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.voice_settings.microphone_device_id_input,
        "mic-device-id"
    );
    assert_eq!(
        signed_in.voice_settings.output_device_id_input,
        "output-device-id"
    );

    restarted.dispatch(AppMessage::VoiceJoinPressed(voice.id));
    restarted.run_all_effects();

    assert_eq!(
        restarted.voice.commands().unwrap_or_default(),
        vec![VoiceCommand::Join(VoiceJoinRequest {
            channel_id: voice.id,
            url: "ws://localhost:7880".to_string(),
            token: "fake-livekit-token".to_string(),
            room: format!("channel-{}", voice.id),
            device_preferences: expected_preferences.voice,
        })]
    );
}

#[test]
fn microphone_permission_failure_is_user_facing_and_recoverable() {
    let voice = voice_channel();
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    connect_voice(&mut harness, voice.id);

    harness.dispatch(AppMessage::VoiceWorkerEvent(VoiceEvent::Error(
        VoiceError::microphone_permission(voice.id, "permission denied"),
    )));

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.voice_connection.status,
        VoiceConnectionStatus::Idle
    );
    assert!(
        signed_in
            .voice_connection
            .error
            .as_ref()
            .is_some_and(|error| error.message.contains("Allow microphone access")
                && error.message.contains("retry joining"))
    );

    assert_eq!(
        harness
            .dispatch(AppMessage::VoiceJoinPressed(voice.id))
            .len(),
        1
    );
}

#[test]
fn voice_leave_command_disconnects_and_clears_local_state() {
    let voice = voice_channel();
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    connect_voice(&mut harness, voice.id);

    let effects = harness.dispatch(AppMessage::VoiceLeavePressed);

    assert_eq!(
        effects,
        vec![AppEffect::SendVoiceCommand(VoiceCommand::Leave)]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_connection
            .status,
        VoiceConnectionStatus::Disconnecting {
            channel_id: voice.id,
        }
    );

    harness.run_all_effects();
    harness.dispatch(AppMessage::VoiceWorkerEvent(VoiceEvent::Disconnected {
        channel_id: Some(voice.id),
        reason: None,
    }));

    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_connection
            .status,
        VoiceConnectionStatus::Idle
    );
}

#[test]
fn voice_switch_disconnects_previous_room_and_joins_requested_room() {
    let api = FakeApi::default();
    let general = general_channel();
    let first = voice_channel();
    let second = channel(12, "stage", 2, ChannelKind::Voice);
    assert!(
        api.set_channels(vec![general, first.clone(), second.clone()])
            .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);
    connect_voice(&mut harness, first.id);

    let effects = harness.dispatch(AppMessage::VoiceJoinPressed(second.id));

    assert_eq!(
        effects,
        vec![
            AppEffect::SendVoiceCommand(VoiceCommand::Leave),
            AppEffect::LoadVoiceToken(VoiceTokenRequest {
                server_url: DEFAULT_SERVER_URL.to_string(),
                session_token: Some("fake-session-token".to_string()),
                channel_id: second.id,
            }),
        ]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_connection
            .status,
        VoiceConnectionStatus::Connecting {
            channel_id: second.id,
        }
    );

    harness.run_all_effects();

    assert_eq!(
        harness.voice.commands().unwrap_or_default(),
        vec![
            VoiceCommand::Join(VoiceJoinRequest {
                channel_id: first.id,
                url: "ws://localhost:7880".to_string(),
                token: "fake-livekit-token".to_string(),
                room: format!("channel-{}", first.id),
                device_preferences: VoiceDevicePreferences::default(),
            }),
            VoiceCommand::Leave,
            VoiceCommand::Join(VoiceJoinRequest {
                channel_id: second.id,
                url: "ws://localhost:7880".to_string(),
                token: "fake-livekit-token".to_string(),
                room: format!("channel-{}", second.id),
                device_preferences: VoiceDevicePreferences::default(),
            }),
        ]
    );

    harness.dispatch(AppMessage::VoiceWorkerEvent(VoiceEvent::Connected {
        channel_id: second.id,
        room: format!("channel-{}", second.id),
    }));

    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_connection
            .status,
        VoiceConnectionStatus::Connected {
            channel_id: second.id,
            room: format!("channel-{}", second.id),
        }
    );
}

#[test]
fn voice_worker_error_is_visible_and_retryable() {
    let voice = voice_channel();
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    connect_voice(&mut harness, voice.id);

    harness.dispatch(AppMessage::VoiceWorkerEvent(VoiceEvent::Error(
        VoiceError::audio(voice.id, "microphone permission denied"),
    )));

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.voice_connection.status,
        VoiceConnectionStatus::Idle
    );
    assert!(
        signed_in
            .voice_connection
            .error
            .as_ref()
            .is_some_and(|error| error.message.contains("microphone permission denied"))
    );

    let effects = harness.dispatch(AppMessage::VoiceJoinPressed(voice.id));

    assert_eq!(effects.len(), 1);
    assert!(matches!(effects[0], AppEffect::LoadVoiceToken(_)));
    assert!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_connection
            .error
            .is_none()
    );
}

#[test]
fn voice_token_error_is_visible_without_worker_command() {
    let api = FakeApi::default();
    let voice = voice_channel();
    assert!(
        api.set_next_voice_token_result(Err(ApiError::Server {
            status: 503,
            kind: Some("service_unavailable".to_string()),
            message: "voice unavailable".to_string(),
        }))
        .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);

    harness.dispatch(AppMessage::VoiceJoinPressed(voice.id));
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.voice_connection.status,
        VoiceConnectionStatus::Idle
    );
    assert!(
        signed_in
            .voice_connection
            .error
            .as_ref()
            .is_some_and(|error| error.message.contains("LiveKit dev stack"))
    );
    assert!(harness.voice.commands().unwrap_or_default().is_empty());
}

#[test]
fn logout_cleans_up_active_voice_connection() {
    let voice = voice_channel();
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    connect_voice(&mut harness, voice.id);

    let effects = harness.dispatch(AppMessage::LogoutPressed);

    assert_eq!(
        effects,
        vec![
            AppEffect::StopRealtime,
            AppEffect::SendVoiceCommand(VoiceCommand::Shutdown),
            AppEffect::Logout(LogoutRequest {
                server_url: DEFAULT_SERVER_URL.to_string(),
                session_token: Some("fake-session-token".to_string()),
            }),
        ]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .voice_connection
            .status,
        VoiceConnectionStatus::Idle
    );
}

#[test]
fn session_expiration_cleans_up_active_voice_connection() {
    let storage = FakeStorage::default();
    let mut harness = ReducerHarness::boot_with_storage(storage);
    sign_in(&mut harness);
    connect_voice(&mut harness, voice_channel().id);

    let effects = harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::AuthExpired,
    ]));

    assert_eq!(
        effects,
        vec![
            AppEffect::StopRealtime,
            AppEffect::SendVoiceCommand(VoiceCommand::Shutdown),
            AppEffect::SavePreferences(
                Preferences::with_server_url(DEFAULT_SERVER_URL).unwrap_or_default()
            ),
        ]
    );
    assert_eq!(harness.state.route, Route::SignedOut);
}

#[test]
fn selecting_text_channel_fetches_that_channel_history() {
    let api = FakeApi::default();
    let general = general_channel();
    let mut second = general_channel();
    second.id = 12;
    second.name = "random".to_string();
    second.position = 1;
    second.kind = ChannelKind::Text;
    let second_history = vec![message(777, second.id, "second channel")];
    assert!(
        api.set_channels(vec![general.clone(), second.clone()])
            .is_ok()
    );
    assert!(api.set_messages(second.id, second_history.clone()).is_ok());
    let mut harness = ReducerHarness::boot_with_api(api);
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();

    let effects = harness.dispatch(AppMessage::ChannelSelected(second.id));

    assert_eq!(
        effects,
        vec![AppEffect::LoadMessageHistory(
            harness
                .state
                .signed_in
                .as_ref()
                .expect("signed in")
                .message_history_request(second.id)
        )]
    );

    harness.run_all_effects();

    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .message_history,
        MessageHistoryState::Loaded {
            channel_id: second.id,
            messages: second_history,
        }
    );
}

#[test]
fn channel_load_failure_is_visible_and_retryable() {
    let api = FakeApi::default();
    assert!(
        api.set_next_channels_result(Err(ApiError::Unreachable {
            server_url: DEFAULT_SERVER_URL.to_string(),
            message: "connection refused".to_string(),
        }))
        .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();

    assert!(matches!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .channels,
        ChannelListState::Failed(_)
    ));

    let effects = harness.dispatch(AppMessage::RetryChannelsPressed);

    assert_eq!(effects.len(), 1);
    harness.run_all_effects();
    assert!(matches!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .channels,
        ChannelListState::Loaded(_)
    ));
}

#[test]
fn creating_text_channel_adds_it_in_server_order_and_opens_history() {
    let api = FakeApi::default();
    let general = general_channel();
    let mut voice = voice_channel();
    voice.position = 2;
    let created = channel(12, "native-text", 1, ChannelKind::Text);
    assert!(
        api.set_channels(vec![general.clone(), voice.clone()])
            .is_ok()
    );
    assert!(
        api.set_next_create_channel_result(Ok(created.clone()))
            .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    sign_in(&mut harness);

    harness.dispatch(AppMessage::CreateChannelNameEdited(
        "  native-text  ".to_string(),
    ));
    let effects = harness.dispatch(AppMessage::CreateChannelPressed);

    assert_eq!(
        effects,
        vec![AppEffect::CreateChannel(CreateChannelRequest {
            server_url: DEFAULT_SERVER_URL.to_string(),
            session_token: Some("fake-session-token".to_string()),
            name: "native-text".to_string(),
            kind: ChannelKind::Text,
        })]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .create_channel
            .status,
        CreateChannelStatus::Creating
    );

    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.create_channel.name, "");
    assert_eq!(signed_in.create_channel.status, CreateChannelStatus::Idle);
    assert_eq!(
        loaded_channel_ids(&harness),
        vec![general.id, created.id, voice.id]
    );
    assert_eq!(signed_in.selected_channel_id, Some(created.id));
    assert_eq!(
        signed_in.message_history,
        MessageHistoryState::Loaded {
            channel_id: created.id,
            messages: Vec::new(),
        }
    );
    assert!(
        api.calls()
            .unwrap_or_default()
            .contains(&ApiCall::CreateChannel {
                name: "native-text".to_string(),
                kind: ChannelKind::Text,
            })
    );
}

#[test]
fn creating_voice_channel_selects_voice_without_loading_messages() {
    let api = FakeApi::default();
    let created = channel(12, "native-voice", 2, ChannelKind::Voice);
    assert!(
        api.set_next_create_channel_result(Ok(created.clone()))
            .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    sign_in(&mut harness);

    harness.dispatch(AppMessage::CreateChannelKindSelected(ChannelKind::Voice));
    harness.dispatch(AppMessage::CreateChannelNameEdited(
        "native-voice".to_string(),
    ));
    harness.dispatch(AppMessage::CreateChannelPressed);
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(loaded_channel_ids(&harness), vec![10, 11, created.id]);
    assert_eq!(signed_in.selected_channel_id, Some(created.id));
    assert_eq!(signed_in.message_history, MessageHistoryState::NotLoaded);
    assert!(
        api.calls()
            .unwrap_or_default()
            .contains(&ApiCall::CreateChannel {
                name: "native-voice".to_string(),
                kind: ChannelKind::Voice,
            })
    );
    assert!(
        !api.calls()
            .unwrap_or_default()
            .contains(&ApiCall::GetMessages {
                channel_id: created.id,
            })
    );
}

#[test]
fn channel_creation_failure_keeps_form_recoverable() {
    let api = FakeApi::default();
    assert!(
        api.set_next_create_channel_result(Err(ApiError::Unreachable {
            server_url: DEFAULT_SERVER_URL.to_string(),
            message: "connection refused".to_string(),
        }))
        .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);

    assert!(
        harness
            .dispatch(AppMessage::CreateChannelPressed)
            .is_empty()
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .create_channel
            .status,
        CreateChannelStatus::Failed("Channel name is required.".to_string())
    );

    harness.dispatch(AppMessage::CreateChannelNameEdited(
        "still-here".to_string(),
    ));
    harness.dispatch(AppMessage::CreateChannelPressed);
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.create_channel.name, "still-here");
    assert_eq!(
        signed_in.create_channel.status,
        CreateChannelStatus::Failed(
            "Could not reach the Hamlet server at http://localhost:3030.".to_string()
        )
    );
    assert_eq!(loaded_channel_ids(&harness), vec![10, 11]);

    harness.dispatch(AppMessage::CreateChannelNameEdited("recovered".to_string()));

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.create_channel.name, "recovered");
    assert_eq!(signed_in.create_channel.status, CreateChannelStatus::Idle);
}

#[test]
fn live_channel_created_event_updates_loaded_channels_in_server_order() {
    let api = FakeApi::default();
    let general = general_channel();
    let mut voice = voice_channel();
    voice.position = 2;
    let created = channel(12, "other-client", 1, ChannelKind::Text);
    assert!(
        api.set_channels(vec![general.clone(), voice.clone()])
            .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);

    let effects = harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::ChannelCreated(created.clone())),
        RealtimeEvent::Broadcast(BroadcastEvent::ChannelCreated(created.clone())),
    ]));

    assert!(effects.is_empty());
    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        loaded_channel_ids(&harness),
        vec![general.id, created.id, voice.id]
    );
    assert_eq!(
        signed_in
            .loaded_channels()
            .iter()
            .filter(|channel| channel.id == created.id)
            .count(),
        1
    );
    assert_eq!(signed_in.selected_channel_id, Some(general.id));
}

#[test]
fn moving_channel_down_updates_order_optimistically_and_submits_full_order() {
    let api = FakeApi::default();
    let general = general_channel();
    let random = channel(12, "random", 1, ChannelKind::Text);
    let mut voice = voice_channel();
    voice.position = 2;
    assert!(
        api.set_channels(vec![general.clone(), random.clone(), voice.clone()])
            .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);

    let effects = harness.dispatch(AppMessage::MoveChannelRequested {
        channel_id: general.id,
        direction: ChannelMoveDirection::Down,
    });

    assert_eq!(
        loaded_channel_ids(&harness),
        vec![random.id, general.id, voice.id]
    );
    assert_eq!(loaded_channel_positions(&harness), vec![0, 1, 2]);
    assert_eq!(
        effects,
        vec![AppEffect::ReorderChannels(ReorderChannelsRequest {
            server_url: DEFAULT_SERVER_URL.to_string(),
            session_token: Some("fake-session-token".to_string()),
            ids: vec![random.id, general.id, voice.id],
        })]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .channel_reorder,
        ChannelReorderState::Committing {
            previous_channels: vec![general, random, voice],
        }
    );
}

#[test]
fn successful_channel_reorder_response_commits_server_order() {
    let api = FakeApi::default();
    let general = general_channel();
    let random = channel(12, "random", 1, ChannelKind::Text);
    let mut voice = voice_channel();
    voice.position = 2;
    assert!(
        api.set_channels(vec![general.clone(), random.clone(), voice.clone()])
            .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    sign_in(&mut harness);

    harness.dispatch(AppMessage::MoveChannelRequested {
        channel_id: random.id,
        direction: ChannelMoveDirection::Up,
    });
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        loaded_channel_ids(&harness),
        vec![random.id, general.id, voice.id]
    );
    assert_eq!(signed_in.channel_reorder, ChannelReorderState::Idle);
    assert!(
        api.calls()
            .unwrap_or_default()
            .contains(&ApiCall::ReorderChannels {
                ids: vec![random.id, general.id, voice.id],
            })
    );
}

#[test]
fn failed_channel_reorder_rolls_back_and_shows_error() {
    let api = FakeApi::default();
    let general = general_channel();
    let random = channel(12, "random", 1, ChannelKind::Text);
    let mut voice = voice_channel();
    voice.position = 2;
    assert!(
        api.set_channels(vec![general.clone(), random.clone(), voice.clone()])
            .is_ok()
    );
    assert!(
        api.set_next_reorder_channels_result(Err(ApiError::Unreachable {
            server_url: DEFAULT_SERVER_URL.to_string(),
            message: "connection refused".to_string(),
        }))
        .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);

    harness.dispatch(AppMessage::MoveChannelRequested {
        channel_id: voice.id,
        direction: ChannelMoveDirection::Up,
    });
    assert_eq!(
        loaded_channel_ids(&harness),
        vec![general.id, voice.id, random.id]
    );

    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        loaded_channel_ids(&harness),
        vec![general.id, random.id, voice.id]
    );
    assert!(matches!(
        &signed_in.channel_reorder,
        ChannelReorderState::Failed(message)
            if message.contains("Could not reorder channels")
                && message.contains("Could not reach the Hamlet server")
    ));
}

#[test]
fn live_channels_reordered_event_updates_visible_order() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    let mut general = general_channel();
    general.position = 1;
    let mut voice = voice_channel();
    voice.position = 0;

    let effects = harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::ChannelsReordered(vec![
            voice.clone(),
            general.clone(),
        ])),
    ]));

    assert!(effects.is_empty());
    assert_eq!(loaded_channel_ids(&harness), vec![voice.id, general.id]);
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .channel_reorder,
        ChannelReorderState::Idle
    );
}

#[test]
fn channel_reorder_boundary_moves_are_noops() {
    let api = FakeApi::default();
    let general = general_channel();
    let voice = voice_channel();
    assert!(
        api.set_channels(vec![general.clone(), voice.clone()])
            .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    sign_in(&mut harness);

    assert!(
        harness
            .dispatch(AppMessage::MoveChannelRequested {
                channel_id: general.id,
                direction: ChannelMoveDirection::Up,
            })
            .is_empty()
    );
    assert!(
        harness
            .dispatch(AppMessage::MoveChannelRequested {
                channel_id: voice.id,
                direction: ChannelMoveDirection::Down,
            })
            .is_empty()
    );

    assert_eq!(loaded_channel_ids(&harness), vec![general.id, voice.id]);
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .channel_reorder,
        ChannelReorderState::Idle
    );
    assert!(
        !api.calls()
            .unwrap_or_default()
            .iter()
            .any(|call| matches!(call, ApiCall::ReorderChannels { .. }))
    );
}

#[test]
fn message_history_failure_is_visible_and_retryable() {
    let api = FakeApi::default();
    assert!(
        api.set_next_messages_result(Err(ApiError::Server {
            status: 500,
            kind: Some("internal_error".to_string()),
            message: "internal error".to_string(),
        }))
        .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();

    assert!(matches!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .message_history,
        MessageHistoryState::Failed { .. }
    ));

    let effects = harness.dispatch(AppMessage::RetryMessageHistoryPressed);

    assert_eq!(effects.len(), 1);
    harness.run_all_effects();
    assert!(matches!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .message_history,
        MessageHistoryState::Loaded { .. }
    ));
}

#[test]
fn sending_non_empty_message_clears_draft_and_appends_to_history() {
    let api = FakeApi::default();
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();

    harness.dispatch(AppMessage::DraftEdited("  hello native  ".to_string()));
    let effects = harness.dispatch(AppMessage::SendMessagePressed);

    assert_eq!(effects.len(), 1);
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .send_status,
        SendMessageStatus::Sending
    );

    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.draft, "");
    assert_eq!(signed_in.send_status, SendMessageStatus::Idle);
    match &signed_in.message_history {
        MessageHistoryState::Loaded { messages, .. } => {
            assert!(
                messages
                    .iter()
                    .any(|message| message.text == "hello native")
            );
        }
        other => panic!("expected loaded history, got {other:?}"),
    }
    assert!(
        api.calls()
            .unwrap_or_default()
            .contains(&ApiCall::SendMessage {
                channel_id: general_channel().id,
                text: "hello native".to_string(),
            })
    );
}

#[test]
fn send_failure_leaves_draft_recoverable_and_shows_error() {
    let api = FakeApi::default();
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();
    assert!(
        api.set_next_send_message_result(Err(ApiError::Unreachable {
            server_url: DEFAULT_SERVER_URL.to_string(),
            message: "connection refused".to_string(),
        }))
        .is_ok()
    );

    harness.dispatch(AppMessage::DraftEdited("still here".to_string()));
    harness.dispatch(AppMessage::SendMessagePressed);
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.draft, "still here");
    assert_eq!(
        signed_in.send_status,
        SendMessageStatus::Failed(
            "Could not reach the Hamlet server at http://localhost:3030.".to_string()
        )
    );
}

#[test]
fn emoji_search_filters_names_and_keywords() {
    let party_matches = search_emoji("party");

    assert!(party_matches.iter().any(|choice| choice.symbol == "🎉"));
    assert!(party_matches.iter().any(|choice| choice.symbol == "🥳"));
    assert!(!party_matches.iter().any(|choice| choice.symbol == "🚀"));

    let rocket_matches = search_emoji("LAUNCH");
    assert_eq!(
        rocket_matches.first().map(|choice| choice.symbol),
        Some("🚀")
    );
}

#[test]
fn emoji_picker_open_close_preserves_draft_and_focus_state() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);

    harness.dispatch(AppMessage::DraftEdited("hello native".to_string()));
    assert!(
        harness
            .dispatch(AppMessage::ToggleEmojiPickerPressed)
            .is_empty()
    );

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert!(signed_in.emoji_picker.is_open);
    assert_eq!(signed_in.emoji_picker.query, "");
    assert_eq!(
        signed_in.emoji_picker.focus_target,
        EmojiPickerFocusTarget::Search
    );
    assert_eq!(signed_in.draft, "hello native");

    harness.dispatch(AppMessage::EmojiSearchEdited("party".to_string()));
    harness.dispatch(AppMessage::CloseEmojiPickerPressed);

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert!(!signed_in.emoji_picker.is_open);
    assert_eq!(signed_in.emoji_picker.query, "");
    assert_eq!(signed_in.draft, "hello native");
    assert_eq!(
        signed_in.emoji_picker.focus_target,
        EmojiPickerFocusTarget::Composer
    );
}

#[test]
fn selecting_emoji_inserts_into_existing_draft_and_closes_picker() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);

    harness.dispatch(AppMessage::DraftEdited("Ship it ".to_string()));
    harness.dispatch(AppMessage::ToggleEmojiPickerPressed);
    harness.dispatch(AppMessage::EmojiSearchEdited("rocket".to_string()));
    harness.dispatch(AppMessage::EmojiPickerSelectFocused);

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(signed_in.draft, "Ship it 🚀");
    assert!(!signed_in.emoji_picker.is_open);
    assert_eq!(
        signed_in.emoji_picker.focus_target,
        EmojiPickerFocusTarget::Composer
    );
}

#[test]
fn emoji_picker_keyboard_navigation_updates_selection_state() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);

    harness.dispatch(AppMessage::ToggleEmojiPickerPressed);
    harness.dispatch(AppMessage::EmojiSearchEdited("face".to_string()));

    let choice_count = harness
        .state
        .signed_in
        .as_ref()
        .expect("signed in")
        .emoji_picker
        .filtered_choices()
        .len();
    assert!(choice_count > 2);

    harness.dispatch(AppMessage::EmojiPickerNavigate(EmojiPickerNavigation::Next));
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .emoji_picker
            .selected_index,
        1
    );

    harness.dispatch(AppMessage::EmojiPickerNavigate(
        EmojiPickerNavigation::Previous,
    ));
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .emoji_picker
            .selected_index,
        0
    );

    harness.dispatch(AppMessage::EmojiPickerNavigate(
        EmojiPickerNavigation::Previous,
    ));
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .emoji_picker
            .selected_index,
        choice_count - 1
    );

    harness.dispatch(AppMessage::EmojiPickerNavigate(
        EmojiPickerNavigation::First,
    ));
    harness.dispatch(AppMessage::EmojiPickerNavigate(EmojiPickerNavigation::Last));
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .emoji_picker
            .selected_index,
        choice_count - 1
    );
}

#[test]
fn own_message_actions_are_available_only_for_current_user() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);

    let own = visible_messages(&harness)
        .into_iter()
        .find(|message| message.user_id == 1)
        .expect("own message exists");
    let other = other_user_message(404, general_channel().id, "not mine");
    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::Message(other.clone())),
    ]));

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.message_action_visibility(&own),
        hamlet_client_iced::auth::MessageActionVisibility {
            can_edit: true,
            can_delete: true,
            can_suppress_embeds: false,
        }
    );
    assert_eq!(
        signed_in.message_action_visibility(&other),
        hamlet_client_iced::auth::MessageActionVisibility {
            can_edit: false,
            can_delete: false,
            can_suppress_embeds: false,
        }
    );

    assert!(
        harness
            .dispatch(AppMessage::EditMessagePressed(other.id))
            .is_empty()
    );
    assert!(
        harness
            .dispatch(AppMessage::DeleteMessagePressed(other.id))
            .is_empty()
    );
    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert!(signed_in.message_actions.editing.is_none());
    assert_eq!(
        signed_in.message_actions.delete_status,
        MessageDeleteStatus::Idle
    );
}

#[test]
fn embed_update_event_patches_existing_message_and_loads_preview_image() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    let original = visible_messages(&harness)[0].clone();
    let preview = embed(
        original.id,
        "link",
        Some("/uploads/previews/preview.png"),
        None,
    );

    let effects = harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::MessageEmbedsUpdated(
            MessageEmbedsUpdatedEvent {
                id: original.id,
                channel_id: original.channel_id,
                suppress_embeds: false,
                embeds: vec![preview.clone()],
            },
        )),
    ]));

    assert_eq!(
        effects,
        vec![AppEffect::LoadEmbedImage(
            hamlet_client_iced::embeds::EmbedImageFetchRequest {
                url: "http://localhost:3030/uploads/previews/preview.png".to_string(),
            }
        )]
    );
    let patched = visible_messages(&harness)
        .into_iter()
        .find(|message| message.id == original.id)
        .expect("message remains visible");
    assert!(!patched.suppress_embeds);
    assert_eq!(patched.embeds, vec![preview]);
}

#[test]
fn suppress_embeds_success_hides_own_message_embeds_and_calls_api() {
    let api = FakeApi::default();
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    sign_in(&mut harness);
    let original = visible_messages(&harness)[0].clone();
    let preview = embed(
        original.id,
        "link",
        Some("https://cdn.example.test/preview.png"),
        None,
    );
    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::MessageEmbedsUpdated(
            MessageEmbedsUpdatedEvent {
                id: original.id,
                channel_id: original.channel_id,
                suppress_embeds: false,
                embeds: vec![preview],
            },
        )),
    ]));
    harness.run_all_effects();

    let effects = harness.dispatch(AppMessage::SuppressEmbedsPressed(original.id));

    assert_eq!(
        effects,
        vec![AppEffect::SuppressMessageEmbeds(
            SuppressMessageEmbedsRequest {
                server_url: DEFAULT_SERVER_URL.to_string(),
                session_token: Some("fake-session-token".to_string()),
                message_id: original.id,
                channel_id: original.channel_id,
                suppress: true,
            }
        )]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .message_actions
            .suppress_status,
        MessageSuppressStatus::Suppressing {
            message_id: original.id,
            channel_id: original.channel_id,
        }
    );

    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.message_actions.suppress_status,
        MessageSuppressStatus::Idle
    );
    let patched = visible_messages(&harness)
        .into_iter()
        .find(|message| message.id == original.id)
        .expect("message remains visible");
    assert!(patched.suppress_embeds);
    assert!(patched.embeds.is_empty());
    assert!(
        api.calls()
            .unwrap_or_default()
            .contains(&ApiCall::SuppressMessageEmbeds {
                message_id: original.id,
                suppress: true,
            })
    );
}

#[test]
fn suppress_embeds_failure_keeps_embeds_visible_and_shows_error() {
    let api = FakeApi::default();
    assert!(
        api.set_next_suppress_embeds_result(Err(ApiError::Unreachable {
            server_url: DEFAULT_SERVER_URL.to_string(),
            message: "connection refused".to_string(),
        }))
        .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);
    let original = visible_messages(&harness)[0].clone();
    let preview = embed(
        original.id,
        "link",
        Some("https://cdn.example.test/preview.png"),
        None,
    );
    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::MessageEmbedsUpdated(
            MessageEmbedsUpdatedEvent {
                id: original.id,
                channel_id: original.channel_id,
                suppress_embeds: false,
                embeds: vec![preview.clone()],
            },
        )),
    ]));
    harness.run_all_effects();

    harness.dispatch(AppMessage::SuppressEmbedsPressed(original.id));
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.message_actions.suppress_status,
        MessageSuppressStatus::Failed {
            message_id: original.id,
            channel_id: original.channel_id,
            message: "Could not suppress embeds. Could not reach the Hamlet server at http://localhost:3030."
                .to_string(),
        }
    );
    let patched = visible_messages(&harness)
        .into_iter()
        .find(|message| message.id == original.id)
        .expect("message remains visible");
    assert!(!patched.suppress_embeds);
    assert_eq!(patched.embeds, vec![preview]);
}

#[test]
fn suppress_embed_action_is_hidden_for_other_users() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    let mut other = other_user_message(404, general_channel().id, "not mine");
    other.embeds = vec![embed(
        other.id,
        "link",
        Some("https://cdn.example.test/preview.png"),
        None,
    )];
    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::Message(other.clone())),
    ]));

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.message_action_visibility(&other),
        hamlet_client_iced::auth::MessageActionVisibility {
            can_edit: false,
            can_delete: false,
            can_suppress_embeds: false,
        }
    );
    assert!(
        harness
            .dispatch(AppMessage::SuppressEmbedsPressed(other.id))
            .is_empty()
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .message_actions
            .suppress_status,
        MessageSuppressStatus::Idle
    );
}

#[test]
fn editing_message_success_updates_visible_message_and_exits_edit_mode() {
    let api = FakeApi::default();
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    sign_in(&mut harness);
    let original = visible_messages(&harness)[0].clone();

    assert!(
        harness
            .dispatch(AppMessage::EditMessagePressed(original.id))
            .is_empty()
    );
    harness.dispatch(AppMessage::EditMessageDraftEdited(
        "  edited from native  ".to_string(),
    ));
    let effects = harness.dispatch(AppMessage::SaveMessageEditPressed);

    assert_eq!(
        effects,
        vec![AppEffect::EditMessage(EditMessageRequest {
            server_url: DEFAULT_SERVER_URL.to_string(),
            session_token: Some("fake-session-token".to_string()),
            message_id: original.id,
            channel_id: original.channel_id,
            text: "edited from native".to_string(),
        })]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .message_actions
            .editing
            .as_ref()
            .map(|editing| &editing.status),
        Some(&MessageEditStatus::Saving)
    );

    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert!(signed_in.message_actions.editing.is_none());
    assert!(
        visible_messages(&harness)
            .iter()
            .any(|message| { message.id == original.id && message.text == "edited from native" })
    );
    assert!(
        api.calls()
            .unwrap_or_default()
            .contains(&ApiCall::EditMessage {
                message_id: original.id,
                text: "edited from native".to_string(),
            })
    );
}

#[test]
fn edit_failure_keeps_edit_context_and_draft_recoverable() {
    let api = FakeApi::default();
    assert!(
        api.set_next_edit_message_result(Err(ApiError::Unreachable {
            server_url: DEFAULT_SERVER_URL.to_string(),
            message: "connection refused".to_string(),
        }))
        .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);
    let original = visible_messages(&harness)[0].clone();

    harness.dispatch(AppMessage::EditMessagePressed(original.id));
    harness.dispatch(AppMessage::EditMessageDraftEdited(
        "still in editor".to_string(),
    ));
    harness.dispatch(AppMessage::SaveMessageEditPressed);
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.message_actions.editing,
        Some(MessageEditState {
            message_id: original.id,
            channel_id: original.channel_id,
            draft: "still in editor".to_string(),
            status: MessageEditStatus::Failed(
                "Could not edit message. Could not reach the Hamlet server at http://localhost:3030."
                    .to_string()
            ),
        })
    );
    assert!(
        visible_messages(&harness)
            .iter()
            .any(|message| { message.id == original.id && message.text == original.text })
    );

    harness.dispatch(AppMessage::EditMessageDraftEdited(
        "recoverable".to_string(),
    ));
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .message_actions
            .editing
            .as_ref()
            .map(|editing| (&editing.draft, &editing.status)),
        Some((&"recoverable".to_string(), &MessageEditStatus::Editing))
    );
}

#[test]
fn deleting_message_success_removes_visible_message() {
    let api = FakeApi::default();
    let mut harness = ReducerHarness::boot_with_api(api.clone());
    sign_in(&mut harness);
    let original = visible_messages(&harness)[0].clone();

    let effects = harness.dispatch(AppMessage::DeleteMessagePressed(original.id));

    assert_eq!(
        effects,
        vec![AppEffect::DeleteMessage(DeleteMessageRequest {
            server_url: DEFAULT_SERVER_URL.to_string(),
            session_token: Some("fake-session-token".to_string()),
            message_id: original.id,
            channel_id: original.channel_id,
        })]
    );
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .message_actions
            .delete_status,
        MessageDeleteStatus::Deleting {
            message_id: original.id,
            channel_id: original.channel_id,
        }
    );

    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.message_actions.delete_status,
        MessageDeleteStatus::Idle
    );
    assert!(
        !visible_messages(&harness)
            .iter()
            .any(|message| message.id == original.id)
    );
    assert!(
        api.calls()
            .unwrap_or_default()
            .contains(&ApiCall::DeleteMessage {
                message_id: original.id,
            })
    );
}

#[test]
fn delete_failure_keeps_message_visible_and_failure_recoverable() {
    let api = FakeApi::default();
    assert!(
        api.set_next_delete_message_result(Err(ApiError::Unreachable {
            server_url: DEFAULT_SERVER_URL.to_string(),
            message: "connection refused".to_string(),
        }))
        .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);
    let original = visible_messages(&harness)[0].clone();

    harness.dispatch(AppMessage::DeleteMessagePressed(original.id));
    harness.run_all_effects();

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    assert_eq!(
        signed_in.message_actions.delete_status,
        MessageDeleteStatus::Failed {
            message_id: original.id,
            channel_id: original.channel_id,
            message: "Could not delete message. Could not reach the Hamlet server at http://localhost:3030."
                .to_string(),
        }
    );
    assert!(
        visible_messages(&harness)
            .iter()
            .any(|message| { message.id == original.id && message.text == original.text })
    );
}

#[test]
fn sse_message_update_and_delete_reconcile_active_history() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    let original = visible_messages(&harness)[0].clone();
    let mut updated = original.clone();
    updated.text = "edited over sse".to_string();

    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::MessageUpdated(updated.clone())),
    ]));

    assert!(
        visible_messages(&harness)
            .iter()
            .any(|message| { message.id == original.id && message.text == "edited over sse" })
    );

    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::MessageDeleted(MessageDeletedEvent {
            id: original.id,
            channel_id: original.channel_id,
        })),
    ]));

    assert!(
        !visible_messages(&harness)
            .iter()
            .any(|message| message.id == original.id)
    );
}

#[test]
fn inactive_channel_update_and_delete_events_do_not_corrupt_active_history() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    let original = visible_messages(&harness)[0].clone();
    let mut inactive_update = original.clone();
    inactive_update.channel_id = 999;
    inactive_update.text = "wrong channel".to_string();

    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::MessageUpdated(inactive_update)),
        RealtimeEvent::Broadcast(BroadcastEvent::MessageDeleted(MessageDeletedEvent {
            id: original.id,
            channel_id: 999,
        })),
    ]));

    assert!(
        visible_messages(&harness)
            .iter()
            .any(|message| { message.id == original.id && message.text == original.text })
    );
}

#[test]
fn non_empty_draft_sends_throttled_typing_pings_for_active_text_channel() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    let channel_id = general_channel().id;

    let first = harness.dispatch(AppMessage::DraftEdited("h".to_string()));
    let second = harness.dispatch(AppMessage::DraftEdited("he".to_string()));

    assert_eq!(first, vec![typing_ping_effect(channel_id)]);
    assert!(second.is_empty());

    for _ in 0..TYPING_PING_INTERVAL_MS.div_ceil(TYPING_SWEEP_MS) {
        harness.dispatch(AppMessage::TypingTimerTick);
    }

    let third = harness.dispatch(AppMessage::DraftEdited("hel".to_string()));

    assert_eq!(third, vec![typing_ping_effect(channel_id)]);
}

#[test]
fn empty_draft_and_voice_selection_do_not_send_typing_pings() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);

    assert!(
        harness
            .dispatch(AppMessage::DraftEdited("   ".to_string()))
            .is_empty()
    );

    harness.dispatch(AppMessage::ChannelSelected(voice_channel().id));

    assert!(
        harness
            .dispatch(AppMessage::DraftEdited("voice nope".to_string()))
            .is_empty()
    );
}

#[test]
fn typing_events_update_indicators_per_channel() {
    let api = FakeApi::default();
    let general = general_channel();
    let random = channel(12, "random", 1, ChannelKind::Text);
    let voice = voice_channel();
    assert!(
        api.set_channels(vec![general.clone(), random.clone(), voice])
            .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    sign_in(&mut harness);

    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::UserTyping(UserTypingEvent {
            channel_id: random.id,
            user_id: 3,
            username: "carol".to_string(),
        })),
        RealtimeEvent::Broadcast(BroadcastEvent::UserTyping(UserTypingEvent {
            channel_id: general.id,
            user_id: 2,
            username: "teo".to_string(),
        })),
    ]));

    let typing = &harness.state.signed_in.as_ref().expect("signed in").typing;
    assert_eq!(
        typing.usernames_for_channel(general.id),
        vec!["teo".to_string()]
    );
    assert_eq!(
        typing.usernames_for_channel(random.id),
        vec!["carol".to_string()]
    );
    assert_eq!(
        typing.indicator_message(general.id).as_deref(),
        Some("teo is typing…")
    );
}

#[test]
fn self_and_voice_typing_events_do_not_show_indicators() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);

    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::UserTyping(UserTypingEvent {
            channel_id: general_channel().id,
            user_id: dev_user().id,
            username: dev_user().username,
        })),
        RealtimeEvent::Broadcast(BroadcastEvent::UserTyping(UserTypingEvent {
            channel_id: voice_channel().id,
            user_id: 2,
            username: "teo".to_string(),
        })),
    ]));

    let typing = &harness.state.signed_in.as_ref().expect("signed in").typing;
    assert!(
        typing
            .usernames_for_channel(general_channel().id)
            .is_empty()
    );
    assert!(typing.usernames_for_channel(voice_channel().id).is_empty());
}

#[test]
fn typing_indicators_expire_on_timer_ticks() {
    let mut harness = ReducerHarness::boot();
    sign_in(&mut harness);
    let channel_id = general_channel().id;

    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::UserTyping(UserTypingEvent {
            channel_id,
            user_id: 2,
            username: "teo".to_string(),
        })),
    ]));
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .typing
            .usernames_for_channel(channel_id),
        vec!["teo".to_string()]
    );

    for _ in 0..TYPING_EXPIRY_MS.div_ceil(TYPING_SWEEP_MS) {
        harness.dispatch(AppMessage::TypingTimerTick);
    }

    assert!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .typing
            .usernames_for_channel(channel_id)
            .is_empty()
    );
}

#[test]
fn empty_draft_or_voice_selection_does_not_send() {
    let api = FakeApi::default();
    let voice = voice_channel();
    assert!(
        api.set_channels(vec![general_channel(), voice.clone()])
            .is_ok()
    );
    let mut harness = ReducerHarness::boot_with_api(api);
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();

    harness.dispatch(AppMessage::DraftEdited("   ".to_string()));
    assert!(harness.dispatch(AppMessage::SendMessagePressed).is_empty());

    harness.dispatch(AppMessage::DraftEdited("voice nope".to_string()));
    harness.dispatch(AppMessage::ChannelSelected(voice.id));
    assert!(harness.dispatch(AppMessage::SendMessagePressed).is_empty());
}

#[test]
fn signing_in_starts_authenticated_realtime_and_logout_stops_it() {
    let mut harness = ReducerHarness::boot();
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();

    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .realtime_status,
        RealtimeConnectionState::Connected
    );
    assert_eq!(
        harness.realtime.calls().unwrap_or_default(),
        vec![RealtimeCall::Connect {
            server_url: DEFAULT_SERVER_URL.to_string(),
            has_session: true,
        }]
    );

    harness.dispatch(AppMessage::LogoutPressed);
    harness.run_all_effects();

    assert_eq!(
        harness.realtime.calls().unwrap_or_default(),
        vec![
            RealtimeCall::Connect {
                server_url: DEFAULT_SERVER_URL.to_string(),
                has_session: true,
            },
            RealtimeCall::Disconnect,
        ]
    );
}

#[test]
fn incoming_realtime_message_appends_to_active_channel_without_duplicates() {
    let mut harness = ReducerHarness::boot();
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();
    let channel_id = general_channel().id;
    let incoming = message(4242, channel_id, "live message");

    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::Message(incoming.clone())),
        RealtimeEvent::Broadcast(BroadcastEvent::Message(incoming)),
    ]));

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    match &signed_in.message_history {
        MessageHistoryState::Loaded { messages, .. } => {
            assert_eq!(
                messages.iter().filter(|message| message.id == 4242).count(),
                1
            );
        }
        other => panic!("expected loaded history, got {other:?}"),
    }
}

#[test]
fn incoming_realtime_message_for_inactive_channel_does_not_corrupt_active_view() {
    let mut harness = ReducerHarness::boot();
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();

    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Broadcast(BroadcastEvent::Message(message(5151, 999, "elsewhere"))),
    ]));

    let signed_in = harness.state.signed_in.as_ref().expect("signed in");
    match &signed_in.message_history {
        MessageHistoryState::Loaded { messages, .. } => {
            assert!(!messages.iter().any(|message| message.id == 5151));
        }
        other => panic!("expected loaded history, got {other:?}"),
    }
}

#[test]
fn realtime_disconnect_enters_backoff_and_reconnect_due_restarts() {
    let mut harness = ReducerHarness::boot();
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();

    harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::Disconnected,
    ]));

    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .realtime_status,
        RealtimeConnectionState::BackingOff {
            attempt: 1,
            delay_ms: 500,
        }
    );

    let effects = harness.dispatch(AppMessage::RealtimeReconnectDue);

    assert_eq!(effects.len(), 1);
    harness.run_all_effects();
    assert_eq!(
        harness
            .state
            .signed_in
            .as_ref()
            .expect("signed in")
            .realtime_status,
        RealtimeConnectionState::Connected
    );
}

#[test]
fn realtime_auth_expiration_logs_out_and_clears_saved_session() {
    let storage = FakeStorage::default();
    let mut harness = ReducerHarness::boot_with_storage(storage.clone());
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();

    let effects = harness.dispatch(AppMessage::RealtimeEventsReceived(vec![
        RealtimeEvent::AuthExpired,
    ]));

    assert_eq!(
        effects,
        vec![
            AppEffect::StopRealtime,
            AppEffect::SavePreferences(
                Preferences::with_server_url(DEFAULT_SERVER_URL).unwrap_or_default()
            )
        ]
    );
    harness.run_all_effects();

    assert_eq!(harness.state.route, Route::SignedOut);
    assert_eq!(
        harness.state.signed_out.notice.as_deref(),
        Some("Your session expired. Please log in again.")
    );
    assert_eq!(
        storage
            .saved_preferences()
            .unwrap_or_default()
            .last()
            .cloned(),
        Some(Preferences::with_server_url(DEFAULT_SERVER_URL).unwrap_or_default())
    );
}

fn sign_in(harness: &mut ReducerHarness) {
    harness.run_all_effects();
    harness.dispatch(AppMessage::UsernameEdited("baipas".to_string()));
    harness.dispatch(AppMessage::PasswordEdited("password".to_string()));
    harness.dispatch(AppMessage::LoginPressed);
    harness.run_all_effects();
}

fn connect_voice(harness: &mut ReducerHarness, channel_id: i64) {
    harness.dispatch(AppMessage::VoiceJoinPressed(channel_id));
    harness.run_all_effects();
    harness.dispatch(AppMessage::VoiceWorkerEvent(VoiceEvent::Connected {
        channel_id,
        room: format!("channel-{channel_id}"),
    }));
}

fn channel(id: i64, name: &str, position: i64, kind: ChannelKind) -> Channel {
    Channel {
        id,
        name: name.to_string(),
        position,
        kind,
    }
}

fn other_user_message(id: i64, channel_id: i64, text: &str) -> Message {
    Message {
        id,
        user_id: 2,
        channel_id,
        text: text.to_string(),
        username: "teo".to_string(),
        display_name: None,
        avatar_url: None,
        suppress_embeds: false,
        embeds: Vec::new(),
    }
}

fn embed(
    message_id: i64,
    embed_type: &str,
    image_url: Option<&str>,
    iframe_url: Option<&str>,
) -> Embed {
    Embed {
        id: message_id + 10_000,
        message_id,
        url: "https://example.test/post".to_string(),
        title: Some("Example".to_string()),
        description: Some("Preview description".to_string()),
        image_url: image_url.map(str::to_string),
        site_name: Some("Example Site".to_string()),
        embed_type: embed_type.to_string(),
        iframe_url: iframe_url.map(str::to_string),
        iframe_width: Some(640),
        iframe_height: Some(360),
    }
}

fn user_with_avatar(mut user: User, avatar_url: Option<&str>) -> User {
    user.avatar_url = avatar_url.map(str::to_string);
    user
}

fn loaded_channel_ids(harness: &ReducerHarness) -> Vec<i64> {
    harness
        .state
        .signed_in
        .as_ref()
        .expect("signed in")
        .loaded_channels()
        .iter()
        .map(|channel| channel.id)
        .collect()
}

fn loaded_channel_positions(harness: &ReducerHarness) -> Vec<i64> {
    harness
        .state
        .signed_in
        .as_ref()
        .expect("signed in")
        .loaded_channels()
        .iter()
        .map(|channel| channel.position)
        .collect()
}

fn typing_ping_effect(channel_id: i64) -> AppEffect {
    AppEffect::PostTyping(TypingPingRequest {
        server_url: DEFAULT_SERVER_URL.to_string(),
        session_token: Some("fake-session-token".to_string()),
        channel_id,
    })
}

fn visible_messages(harness: &ReducerHarness) -> Vec<Message> {
    match &harness
        .state
        .signed_in
        .as_ref()
        .expect("signed in")
        .message_history
    {
        MessageHistoryState::Loaded { messages, .. } => messages.clone(),
        other => panic!("expected loaded history, got {other:?}"),
    }
}
