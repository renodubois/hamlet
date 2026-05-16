use crate::api::ApiClient;
use crate::app::{self, AppEffect, AppMessage, AppState};
use crate::auth::AuthAction;
use crate::external_open::ExternalOpenService;
use crate::realtime::RealtimeClient;
use crate::storage::Storage;

use super::fake_api::FakeApi;
use super::fake_external_open::FakeExternalOpen;
use super::fake_realtime::FakeRealtime;
use super::fake_storage::FakeStorage;
use super::fake_voice::FakeVoiceWorker;

#[derive(Debug, Clone)]
pub struct ReducerHarness {
    pub state: AppState,
    pub storage: FakeStorage,
    pub api: FakeApi,
    pub realtime: FakeRealtime,
    pub voice: FakeVoiceWorker,
    pub external_open: FakeExternalOpen,
    effects: Vec<AppEffect>,
}

impl ReducerHarness {
    pub fn boot_with_storage(storage: FakeStorage) -> Self {
        Self::boot_with_storage_api_and_realtime(
            storage,
            FakeApi::default(),
            FakeRealtime::default(),
        )
    }

    pub fn boot_with_api(api: FakeApi) -> Self {
        Self::boot_with_storage_api_and_realtime(
            FakeStorage::default(),
            api,
            FakeRealtime::default(),
        )
    }

    pub fn boot_with_storage_and_api(storage: FakeStorage, api: FakeApi) -> Self {
        Self::boot_with_storage_api_and_realtime(storage, api, FakeRealtime::default())
    }

    pub fn boot_with_storage_api_and_realtime(
        storage: FakeStorage,
        api: FakeApi,
        realtime: FakeRealtime,
    ) -> Self {
        let (state, effects) = app::boot();

        Self {
            state,
            storage,
            api,
            realtime,
            voice: FakeVoiceWorker::default(),
            external_open: FakeExternalOpen::default(),
            effects,
        }
    }

    pub fn boot() -> Self {
        Self::boot_with_storage(FakeStorage::default())
    }

    pub fn dispatch(&mut self, message: AppMessage) -> Vec<AppEffect> {
        let effects = app::reduce(&mut self.state, message);
        self.effects.extend(effects.clone());
        effects
    }

    pub fn pending_effects(&self) -> &[AppEffect] {
        &self.effects
    }

    pub fn run_next_effect(&mut self) -> bool {
        if self.effects.is_empty() {
            return false;
        }

        let effect = self.effects.remove(0);
        let message = match effect {
            AppEffect::LoadPreferences => AppMessage::PreferencesLoaded(
                self.storage
                    .load_preferences()
                    .map_err(|error| error.to_string()),
            ),
            AppEffect::SavePreferences(preferences) => {
                let result = self
                    .storage
                    .save_preferences(&preferences)
                    .map(|()| preferences)
                    .map_err(|error| error.to_string());

                AppMessage::ServerUrlSaved(result)
            }
            AppEffect::Authenticate(request) => {
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| match request.action {
                        AuthAction::Login => self.api.login(request.username, request.password),
                        AuthAction::Register => {
                            self.api
                                .register(request.username, request.password, request.email)
                        }
                    });

                AppMessage::AuthCompleted(result)
            }
            AppEffect::RestoreSession(request) => {
                let session_token = request.session_token;
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(Some(session_token.clone())))
                    .and_then(|()| {
                        self.api.get_me().map(|user| {
                            crate::auth::AuthSession::new(user, Some(session_token.clone()))
                        })
                    });

                AppMessage::SessionRestoreCompleted(result)
            }
            AppEffect::Logout(request) => {
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| self.api.logout());

                AppMessage::LogoutCompleted(result)
            }
            AppEffect::LoadChannels(request) => {
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| self.api.list_channels());

                AppMessage::ChannelsLoaded(result)
            }
            AppEffect::CreateChannel(request) => {
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| self.api.create_channel(request.name, request.kind));

                AppMessage::ChannelCreated(result)
            }
            AppEffect::ReorderChannels(request) => {
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| self.api.reorder_channels(request.ids));

                AppMessage::ChannelReorderCompleted(result)
            }
            AppEffect::UpdateProfile(request) => {
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| self.api.update_profile(request.display_name));

                AppMessage::ProfileUpdated(result)
            }
            AppEffect::PickAvatarFile => AppMessage::AvatarFileSelected(Ok(None)),
            AppEffect::UploadAvatar(request) => {
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| self.api.upload_avatar(request.path));

                AppMessage::AvatarUploaded(result)
            }
            AppEffect::DeleteAvatar(request) => {
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| self.api.delete_avatar());

                AppMessage::AvatarDeleted(result)
            }
            AppEffect::LoadAvatarImage(request) => AppMessage::AvatarImageLoaded {
                url: request.url,
                result: Ok(vec![1, 2, 3]),
            },
            AppEffect::LoadEmbedImage(request) => AppMessage::EmbedImageLoaded {
                url: request.url,
                result: Ok(vec![4, 5, 6]),
            },
            AppEffect::LoadMessageHistory(request) => {
                let channel_id = request.channel_id;
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| self.api.get_messages(channel_id));

                AppMessage::MessageHistoryLoaded { channel_id, result }
            }
            AppEffect::SendMessage(request) => {
                let channel_id = request.channel_id;
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| self.api.send_message(channel_id, request.text));

                AppMessage::MessageSent { channel_id, result }
            }
            AppEffect::PostTyping(request) => {
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| self.api.post_typing(request.channel_id));

                AppMessage::TypingPingPosted(result)
            }
            AppEffect::EditMessage(request) => {
                let message_id = request.message_id;
                let channel_id = request.channel_id;
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| self.api.edit_message(message_id, request.text));

                AppMessage::MessageEdited {
                    message_id,
                    channel_id,
                    result,
                }
            }
            AppEffect::DeleteMessage(request) => {
                let message_id = request.message_id;
                let channel_id = request.channel_id;
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| self.api.delete_message(message_id));

                AppMessage::MessageDeleted {
                    message_id,
                    channel_id,
                    result,
                }
            }
            AppEffect::SuppressMessageEmbeds(request) => {
                let message_id = request.message_id;
                let channel_id = request.channel_id;
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| {
                        self.api
                            .suppress_message_embeds(message_id, request.suppress)
                    });

                AppMessage::EmbedsSuppressed {
                    message_id,
                    channel_id,
                    result,
                }
            }
            AppEffect::LoadVoiceParticipants(request) => {
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| {
                        request.channel_ids.into_iter().try_fold(
                            Vec::new(),
                            |mut participants, channel_id| {
                                participants.extend(self.api.list_voice_participants(channel_id)?);
                                Ok(participants)
                            },
                        )
                    });

                AppMessage::VoiceParticipantsLoaded(result)
            }
            AppEffect::LoadVoiceToken(request) => {
                let channel_id = request.channel_id;
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| self.api.get_voice_token(channel_id));

                AppMessage::VoiceTokenLoaded { channel_id, result }
            }
            AppEffect::SaveVoicePreferences(preferences) => {
                let result = self
                    .storage
                    .save_preferences(&preferences)
                    .map(|()| preferences)
                    .map_err(|error| error.to_string());

                AppMessage::VoicePreferencesSaved(result)
            }
            AppEffect::PostVoiceSpeaking(request) => {
                let result = self
                    .api
                    .set_base_url(request.server_url)
                    .and_then(|()| self.api.set_session_token(request.session_token))
                    .and_then(|()| {
                        self.api
                            .post_voice_speaking(request.channel_id, request.speaking)
                    });

                AppMessage::VoiceSpeakingPosted(result)
            }
            AppEffect::OpenExternalUrl(url) => {
                let result = self.external_open.open_external_url(&url);

                AppMessage::ExternalUrlOpened { url, result }
            }
            AppEffect::SendVoiceCommand(command) => AppMessage::VoiceWorkerEvent(
                self.voice
                    .send(command)
                    .unwrap_or_else(crate::voice::VoiceEvent::Error),
            ),
            AppEffect::StartRealtime(request) => {
                AppMessage::RealtimeStarted(self.realtime.connect(request))
            }
            AppEffect::StopRealtime => AppMessage::RealtimeStopped(self.realtime.disconnect()),
        };

        self.dispatch(message);
        true
    }

    pub fn drain_realtime(&mut self) -> Vec<AppEffect> {
        let message = self
            .realtime
            .drain_events()
            .map(AppMessage::RealtimeEventsReceived)
            .unwrap_or_else(|error| AppMessage::RealtimeStarted(Err(error)));

        self.dispatch(message)
    }

    pub fn drain_voice(&mut self) -> Vec<AppEffect> {
        self.voice
            .drain_events()
            .map(|events| {
                let mut effects = Vec::new();
                for event in events {
                    effects.extend(self.dispatch(AppMessage::VoiceWorkerEvent(event)));
                }
                effects
            })
            .unwrap_or_else(|error| {
                self.dispatch(AppMessage::VoiceWorkerEvent(
                    crate::voice::VoiceEvent::Error(error),
                ))
            })
    }

    pub fn run_all_effects(&mut self) {
        while self.run_next_effect() {}
    }
}
