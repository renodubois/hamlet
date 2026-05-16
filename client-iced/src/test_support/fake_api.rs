use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::api::{ApiCall, ApiClient, ApiError};
use crate::auth::AuthSession;
use crate::protocol::{
    Channel, ChannelKind, Id, Message, MessageEmbedsUpdatedEvent, User, VoiceParticipant,
    VoiceToken,
};
use crate::storage::{DEFAULT_SERVER_URL, Preferences};

use super::fixtures::{dev_user, general_channel, message, voice_channel};

const DISPLAY_NAME_MAX_LEN: usize = 64;

#[derive(Debug, Clone)]
pub struct FakeApi {
    inner: Arc<Mutex<FakeApiInner>>,
}

#[derive(Debug, Clone)]
struct FakeApiInner {
    base_url: String,
    calls: Vec<ApiCall>,
    next_error: Option<ApiError>,
    next_login_result: Option<Result<AuthSession, ApiError>>,
    next_register_result: Option<Result<AuthSession, ApiError>>,
    next_me_result: Option<Result<User, ApiError>>,
    next_channels_result: Option<Result<Vec<Channel>, ApiError>>,
    next_create_channel_result: Option<Result<Channel, ApiError>>,
    next_reorder_channels_result: Option<Result<Vec<Channel>, ApiError>>,
    next_messages_result: Option<Result<Vec<Message>, ApiError>>,
    next_send_message_result: Option<Result<Message, ApiError>>,
    next_edit_message_result: Option<Result<Message, ApiError>>,
    next_delete_message_result: Option<Result<(), ApiError>>,
    next_suppress_embeds_result: Option<Result<MessageEmbedsUpdatedEvent, ApiError>>,
    next_voice_participants_result: Option<Result<Vec<VoiceParticipant>, ApiError>>,
    next_voice_token_result: Option<Result<VoiceToken, ApiError>>,
    next_update_profile_result: Option<Result<User, ApiError>>,
    next_avatar_upload_result: Option<Result<User, ApiError>>,
    next_avatar_delete_result: Option<Result<User, ApiError>>,
    authenticated_user: Option<User>,
    session_token: Option<String>,
    channels: Vec<Channel>,
    messages_by_channel: HashMap<Id, Vec<Message>>,
    voice_participants_by_channel: HashMap<Id, Vec<VoiceParticipant>>,
}

impl FakeApi {
    pub fn new(base_url: impl Into<String>) -> Self {
        let general = general_channel();
        let voice = voice_channel();
        let mut messages_by_channel = HashMap::new();
        messages_by_channel.insert(general.id, vec![message(100, general.id, "hello")]);

        Self {
            inner: Arc::new(Mutex::new(FakeApiInner {
                base_url: base_url.into(),
                calls: Vec::new(),
                next_error: None,
                next_login_result: None,
                next_register_result: None,
                next_me_result: None,
                next_channels_result: None,
                next_create_channel_result: None,
                next_reorder_channels_result: None,
                next_messages_result: None,
                next_send_message_result: None,
                next_edit_message_result: None,
                next_delete_message_result: None,
                next_suppress_embeds_result: None,
                next_voice_participants_result: None,
                next_voice_token_result: None,
                next_update_profile_result: None,
                next_avatar_upload_result: None,
                next_avatar_delete_result: None,
                authenticated_user: None,
                session_token: None,
                channels: vec![general, voice],
                messages_by_channel,
                voice_participants_by_channel: HashMap::new(),
            })),
        }
    }

    pub fn fail_next(&self, error: ApiError) -> Result<(), ApiError> {
        self.lock()?.next_error = Some(error);
        Ok(())
    }

    pub fn set_next_login_result(
        &self,
        result: Result<AuthSession, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_login_result = Some(result);
        Ok(())
    }

    pub fn set_next_register_result(
        &self,
        result: Result<AuthSession, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_register_result = Some(result);
        Ok(())
    }

    pub fn set_next_me_result(&self, result: Result<User, ApiError>) -> Result<(), ApiError> {
        self.lock()?.next_me_result = Some(result);
        Ok(())
    }

    pub fn set_channels(&self, channels: Vec<Channel>) -> Result<(), ApiError> {
        self.lock()?.channels = channels;
        Ok(())
    }

    pub fn set_messages(&self, channel_id: Id, messages: Vec<Message>) -> Result<(), ApiError> {
        self.lock()?
            .messages_by_channel
            .insert(channel_id, messages);
        Ok(())
    }

    pub fn set_voice_participants(
        &self,
        channel_id: Id,
        participants: Vec<VoiceParticipant>,
    ) -> Result<(), ApiError> {
        self.lock()?
            .voice_participants_by_channel
            .insert(channel_id, participants);
        Ok(())
    }

    pub fn set_next_channels_result(
        &self,
        result: Result<Vec<Channel>, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_channels_result = Some(result);
        Ok(())
    }

    pub fn set_next_create_channel_result(
        &self,
        result: Result<Channel, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_create_channel_result = Some(result);
        Ok(())
    }

    pub fn set_next_reorder_channels_result(
        &self,
        result: Result<Vec<Channel>, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_reorder_channels_result = Some(result);
        Ok(())
    }

    pub fn set_next_messages_result(
        &self,
        result: Result<Vec<Message>, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_messages_result = Some(result);
        Ok(())
    }

    pub fn set_next_send_message_result(
        &self,
        result: Result<Message, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_send_message_result = Some(result);
        Ok(())
    }

    pub fn set_next_edit_message_result(
        &self,
        result: Result<Message, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_edit_message_result = Some(result);
        Ok(())
    }

    pub fn set_next_delete_message_result(
        &self,
        result: Result<(), ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_delete_message_result = Some(result);
        Ok(())
    }

    pub fn set_next_suppress_embeds_result(
        &self,
        result: Result<MessageEmbedsUpdatedEvent, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_suppress_embeds_result = Some(result);
        Ok(())
    }

    pub fn set_next_voice_participants_result(
        &self,
        result: Result<Vec<VoiceParticipant>, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_voice_participants_result = Some(result);
        Ok(())
    }

    pub fn set_next_update_profile_result(
        &self,
        result: Result<User, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_update_profile_result = Some(result);
        Ok(())
    }

    pub fn set_next_avatar_upload_result(
        &self,
        result: Result<User, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_avatar_upload_result = Some(result);
        Ok(())
    }

    pub fn set_next_avatar_delete_result(
        &self,
        result: Result<User, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_avatar_delete_result = Some(result);
        Ok(())
    }

    pub fn set_next_voice_token_result(
        &self,
        result: Result<VoiceToken, ApiError>,
    ) -> Result<(), ApiError> {
        self.lock()?.next_voice_token_result = Some(result);
        Ok(())
    }

    pub fn record_call(&self, call: ApiCall) -> Result<(), ApiError> {
        let mut inner = self.lock()?;

        if let Some(error) = inner.next_error.take() {
            return Err(error);
        }

        inner.calls.push(call);
        Ok(())
    }

    pub fn calls(&self) -> Result<Vec<ApiCall>, ApiError> {
        Ok(self.lock()?.calls.clone())
    }

    pub fn authenticated_user(&self) -> Result<Option<User>, ApiError> {
        Ok(self.lock()?.authenticated_user.clone())
    }

    fn take_next_error(inner: &mut FakeApiInner) -> Result<(), ApiError> {
        if let Some(error) = inner.next_error.take() {
            return Err(error);
        }

        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, FakeApiInner>, ApiError> {
        self.inner
            .lock()
            .map_err(|_| ApiError::Fake("fake API lock poisoned".to_string()))
    }
}

impl Default for FakeApi {
    fn default() -> Self {
        Self::new(DEFAULT_SERVER_URL)
    }
}

impl ApiClient for FakeApi {
    fn base_url(&self) -> String {
        match self.lock() {
            Ok(inner) => inner.base_url.clone(),
            Err(_) => DEFAULT_SERVER_URL.to_string(),
        }
    }

    fn set_base_url(&self, server_url: String) -> Result<(), ApiError> {
        let preferences = Preferences::with_server_url(server_url)
            .map_err(|error| ApiError::InvalidServerUrl(error.to_string()))?;
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;

        inner.base_url = preferences.server_url.clone();
        inner.session_token = None;
        inner
            .calls
            .push(ApiCall::SetBaseUrl(preferences.server_url));

        Ok(())
    }

    fn set_session_token(&self, session_token: Option<String>) -> Result<(), ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;

        let present = session_token
            .as_ref()
            .is_some_and(|token| !token.is_empty());
        inner.session_token = session_token.filter(|token| !token.is_empty());
        inner.calls.push(ApiCall::SetSessionToken { present });

        Ok(())
    }

    fn login(&self, username: String, _password: String) -> Result<AuthSession, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::Login {
            username: username.clone(),
        });

        let result = inner
            .next_login_result
            .take()
            .unwrap_or_else(|| Ok(fake_session(&username)));

        if let Ok(session) = &result {
            inner.authenticated_user = Some(session.user.clone());
            inner.session_token = session.session_token.clone();
        }

        result
    }

    fn register(
        &self,
        username: String,
        _password: String,
        _email: Option<String>,
    ) -> Result<AuthSession, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::Register {
            username: username.clone(),
        });

        let result = inner
            .next_register_result
            .take()
            .unwrap_or_else(|| Ok(fake_session(&username)));

        if let Ok(session) = &result {
            inner.authenticated_user = Some(session.user.clone());
            inner.session_token = session.session_token.clone();
        }

        result
    }

    fn get_me(&self) -> Result<User, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::GetMe);

        if let Some(result) = inner.next_me_result.take() {
            if let Ok(user) = &result {
                inner.authenticated_user = Some(user.clone());
            }
            return result;
        }

        if let Some(user) = &inner.authenticated_user {
            return Ok(user.clone());
        }

        if inner.session_token.is_some() {
            let user = dev_user();
            inner.authenticated_user = Some(user.clone());
            return Ok(user);
        }

        Err(ApiError::Unauthorized)
    }

    fn logout(&self) -> Result<(), ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::Logout);
        inner.authenticated_user = None;
        inner.session_token = None;

        Ok(())
    }

    fn update_profile(&self, display_name: Option<String>) -> Result<User, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::UpdateProfile {
            display_name: display_name.clone(),
        });

        let result = inner
            .next_update_profile_result
            .take()
            .unwrap_or_else(|| fake_update_profile(&inner, display_name));

        if let Ok(user) = &result {
            inner.authenticated_user = Some(user.clone());
            refresh_messages_for_user(&mut inner.messages_by_channel, user);
        }

        result
    }

    fn upload_avatar(&self, path: PathBuf) -> Result<User, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::UploadAvatar { path });

        let result = inner
            .next_avatar_upload_result
            .take()
            .unwrap_or_else(|| fake_upload_avatar(&inner));

        if let Ok(user) = &result {
            inner.authenticated_user = Some(user.clone());
            refresh_messages_for_user(&mut inner.messages_by_channel, user);
            refresh_voice_participants_for_user(&mut inner.voice_participants_by_channel, user);
        }

        result
    }

    fn delete_avatar(&self) -> Result<User, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::DeleteAvatar);

        let result = inner
            .next_avatar_delete_result
            .take()
            .unwrap_or_else(|| fake_delete_avatar(&inner));

        if let Ok(user) = &result {
            inner.authenticated_user = Some(user.clone());
            refresh_messages_for_user(&mut inner.messages_by_channel, user);
            refresh_voice_participants_for_user(&mut inner.voice_participants_by_channel, user);
        }

        result
    }

    fn list_channels(&self) -> Result<Vec<Channel>, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::ListChannels);

        inner
            .next_channels_result
            .take()
            .unwrap_or_else(|| Ok(inner.channels.clone()))
    }

    fn create_channel(&self, name: String, kind: ChannelKind) -> Result<Channel, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::CreateChannel {
            name: name.clone(),
            kind,
        });

        let result = inner.next_create_channel_result.take().unwrap_or_else(|| {
            let id = inner
                .channels
                .iter()
                .map(|channel| channel.id)
                .max()
                .unwrap_or(899)
                .saturating_add(1);
            let position = inner
                .channels
                .iter()
                .map(|channel| channel.position)
                .max()
                .unwrap_or(-1)
                .saturating_add(1);

            Ok(Channel {
                id,
                name,
                position,
                kind,
            })
        });

        if let Ok(channel) = &result {
            if let Some(existing) = inner
                .channels
                .iter_mut()
                .find(|existing| existing.id == channel.id)
            {
                *existing = channel.clone();
            } else {
                inner.channels.push(channel.clone());
            }
            inner
                .channels
                .sort_by_key(|channel| (channel.position, channel.id));
        }

        result
    }

    fn reorder_channels(&self, ids: Vec<Id>) -> Result<Vec<Channel>, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner
            .calls
            .push(ApiCall::ReorderChannels { ids: ids.clone() });

        let result = inner
            .next_reorder_channels_result
            .take()
            .unwrap_or_else(|| {
                if ids.len() != inner.channels.len() {
                    return Err(ApiError::InvalidRequest(
                        "channel reorder must include every channel".to_string(),
                    ));
                }

                let mut reordered = Vec::with_capacity(ids.len());
                for (position, id) in ids.iter().enumerate() {
                    if reordered.iter().any(|channel: &Channel| channel.id == *id) {
                        return Err(ApiError::InvalidRequest(
                            "channel reorder contains a duplicate channel".to_string(),
                        ));
                    }

                    let Some(existing) = inner.channels.iter().find(|channel| channel.id == *id)
                    else {
                        return Err(ApiError::InvalidRequest(
                            "channel reorder contains an unknown channel".to_string(),
                        ));
                    };
                    let mut channel = existing.clone();
                    channel.position = position as Id;
                    reordered.push(channel);
                }

                Ok(reordered)
            });

        if let Ok(channels) = &result {
            inner.channels = channels.clone();
            inner
                .channels
                .sort_by_key(|channel| (channel.position, channel.id));
        }

        result
    }

    fn get_messages(&self, channel_id: Id) -> Result<Vec<Message>, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::GetMessages { channel_id });

        inner.next_messages_result.take().unwrap_or_else(|| {
            Ok(inner
                .messages_by_channel
                .get(&channel_id)
                .cloned()
                .unwrap_or_default())
        })
    }

    fn send_message(&self, channel_id: Id, text: String) -> Result<Message, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::SendMessage {
            channel_id,
            text: text.clone(),
        });

        let result = inner.next_send_message_result.take().unwrap_or_else(|| {
            Ok(message_from_user(
                900,
                channel_id,
                &text,
                inner.authenticated_user.as_ref(),
            ))
        });

        if let Ok(message) = &result {
            inner
                .messages_by_channel
                .entry(channel_id)
                .or_default()
                .push(message.clone());
        }

        result
    }

    fn post_typing(&self, channel_id: Id) -> Result<(), ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::PostTyping { channel_id });

        Ok(())
    }

    fn edit_message(&self, message_id: Id, text: String) -> Result<Message, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::EditMessage {
            message_id,
            text: text.clone(),
        });

        let result = inner
            .next_edit_message_result
            .take()
            .unwrap_or_else(|| fake_edit_message(&inner, message_id, &text));

        if let Ok(message) = &result {
            upsert_message(&mut inner.messages_by_channel, message.clone());
        }

        result
    }

    fn delete_message(&self, message_id: Id) -> Result<(), ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::DeleteMessage { message_id });

        let result = inner
            .next_delete_message_result
            .take()
            .unwrap_or_else(|| fake_delete_message(&inner, message_id));

        if result.is_ok() {
            remove_message(&mut inner.messages_by_channel, message_id);
        }

        result
    }

    fn suppress_message_embeds(
        &self,
        message_id: Id,
        suppress: bool,
    ) -> Result<MessageEmbedsUpdatedEvent, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::SuppressMessageEmbeds {
            message_id,
            suppress,
        });

        let result = inner
            .next_suppress_embeds_result
            .take()
            .unwrap_or_else(|| fake_suppress_message_embeds(&inner, message_id, suppress));

        if let Ok(update) = &result {
            patch_message_embeds(&mut inner.messages_by_channel, update);
        }

        result
    }

    fn list_voice_participants(&self, channel_id: Id) -> Result<Vec<VoiceParticipant>, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner
            .calls
            .push(ApiCall::ListVoiceParticipants { channel_id });

        inner
            .next_voice_participants_result
            .take()
            .unwrap_or_else(|| {
                Ok(inner
                    .voice_participants_by_channel
                    .get(&channel_id)
                    .cloned()
                    .unwrap_or_default())
            })
    }

    fn get_voice_token(&self, channel_id: Id) -> Result<VoiceToken, ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::GetVoiceToken { channel_id });

        inner
            .next_voice_token_result
            .take()
            .unwrap_or_else(|| Ok(fake_voice_token(channel_id)))
    }

    fn post_voice_speaking(&self, channel_id: Id, speaking: bool) -> Result<(), ApiError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(ApiCall::PostVoiceSpeaking {
            channel_id,
            speaking,
        });

        Ok(())
    }
}

fn fake_update_profile(
    inner: &FakeApiInner,
    display_name: Option<String>,
) -> Result<User, ApiError> {
    if inner.authenticated_user.is_none() && inner.session_token.is_none() {
        return Err(ApiError::Unauthorized);
    }

    let mut user = inner.authenticated_user.clone().unwrap_or_else(dev_user);
    user.display_name = match display_name.as_deref() {
        None => None,
        Some(name) => {
            let trimmed = name.trim();

            if trimmed.is_empty() {
                None
            } else if trimmed.chars().count() > DISPLAY_NAME_MAX_LEN {
                return Err(ApiError::InvalidRequest(
                    "display name is too long".to_string(),
                ));
            } else {
                Some(trimmed.to_string())
            }
        }
    };

    Ok(user)
}

fn fake_upload_avatar(inner: &FakeApiInner) -> Result<User, ApiError> {
    let mut user = authenticated_user(inner)?;
    user.avatar_url = Some(format!("/uploads/avatars/{}.webp?v=1", user.id));

    Ok(user)
}

fn fake_delete_avatar(inner: &FakeApiInner) -> Result<User, ApiError> {
    let mut user = authenticated_user(inner)?;
    user.avatar_url = None;

    Ok(user)
}

fn refresh_messages_for_user(messages_by_channel: &mut HashMap<Id, Vec<Message>>, user: &User) {
    for message in messages_by_channel
        .values_mut()
        .flat_map(|messages| messages.iter_mut())
        .filter(|message| message.user_id == user.id)
    {
        message.username = user.username.clone();
        message.display_name = user.display_name.clone();
        message.avatar_url = user.avatar_url.clone();
    }
}

fn refresh_voice_participants_for_user(
    participants_by_channel: &mut HashMap<Id, Vec<VoiceParticipant>>,
    user: &User,
) {
    for participant in participants_by_channel
        .values_mut()
        .flat_map(|participants| participants.iter_mut())
        .filter(|participant| participant.user_id == user.id)
    {
        participant.username = user.username.clone();
        participant.avatar_url = user.avatar_url.clone();
    }
}

fn fake_edit_message(
    inner: &FakeApiInner,
    message_id: Id,
    text: &str,
) -> Result<Message, ApiError> {
    let user = authenticated_user(inner)?;
    let mut message = find_message(&inner.messages_by_channel, message_id)?;

    if message.user_id != user.id {
        return Err(forbidden_error());
    }

    message.text = text.to_string();
    message.username = user.username;
    message.display_name = user.display_name;
    message.avatar_url = user.avatar_url;

    Ok(message)
}

fn fake_delete_message(inner: &FakeApiInner, message_id: Id) -> Result<(), ApiError> {
    let user = authenticated_user(inner)?;
    let message = find_message(&inner.messages_by_channel, message_id)?;

    if message.user_id != user.id {
        return Err(forbidden_error());
    }

    Ok(())
}

fn fake_suppress_message_embeds(
    inner: &FakeApiInner,
    message_id: Id,
    suppress: bool,
) -> Result<MessageEmbedsUpdatedEvent, ApiError> {
    let user = authenticated_user(inner)?;
    let message = find_message(&inner.messages_by_channel, message_id)?;

    if message.user_id != user.id {
        return Err(forbidden_error());
    }

    Ok(MessageEmbedsUpdatedEvent {
        id: message.id,
        channel_id: message.channel_id,
        suppress_embeds: suppress,
        embeds: message.embeds,
    })
}

fn authenticated_user(inner: &FakeApiInner) -> Result<User, ApiError> {
    if let Some(user) = &inner.authenticated_user {
        Ok(user.clone())
    } else if inner.session_token.is_some() {
        Ok(dev_user())
    } else {
        Err(ApiError::Unauthorized)
    }
}

fn find_message(
    messages_by_channel: &HashMap<Id, Vec<Message>>,
    message_id: Id,
) -> Result<Message, ApiError> {
    messages_by_channel
        .values()
        .flat_map(|messages| messages.iter())
        .find(|message| message.id == message_id)
        .cloned()
        .ok_or_else(|| ApiError::Server {
            status: 404,
            kind: Some("not_found".to_string()),
            message: "message not found".to_string(),
        })
}

fn upsert_message(messages_by_channel: &mut HashMap<Id, Vec<Message>>, message: Message) {
    let messages = messages_by_channel.entry(message.channel_id).or_default();

    if let Some(existing) = messages
        .iter_mut()
        .find(|existing| existing.id == message.id)
    {
        *existing = message;
    } else {
        messages.push(message);
    }
}

fn remove_message(messages_by_channel: &mut HashMap<Id, Vec<Message>>, message_id: Id) {
    for messages in messages_by_channel.values_mut() {
        messages.retain(|message| message.id != message_id);
    }
}

fn patch_message_embeds(
    messages_by_channel: &mut HashMap<Id, Vec<Message>>,
    update: &MessageEmbedsUpdatedEvent,
) {
    if let Some(message) = messages_by_channel
        .get_mut(&update.channel_id)
        .and_then(|messages| messages.iter_mut().find(|message| message.id == update.id))
    {
        message.suppress_embeds = update.suppress_embeds;
        message.embeds = update.embeds.clone();
    }
}

fn forbidden_error() -> ApiError {
    ApiError::Server {
        status: 403,
        kind: Some("forbidden".to_string()),
        message: "forbidden".to_string(),
    }
}

fn message_from_user(id: Id, channel_id: Id, text: &str, user: Option<&User>) -> Message {
    let user = user.cloned().unwrap_or_else(dev_user);

    Message {
        id,
        user_id: user.id,
        channel_id,
        text: text.to_string(),
        username: user.username,
        display_name: user.display_name,
        avatar_url: user.avatar_url,
        suppress_embeds: false,
        embeds: Vec::new(),
    }
}

fn fake_voice_token(channel_id: Id) -> VoiceToken {
    VoiceToken {
        url: "ws://localhost:7880".to_string(),
        token: "fake-livekit-token".to_string(),
        room: format!("channel-{channel_id}"),
    }
}

fn fake_session(username: &str) -> AuthSession {
    AuthSession::new(
        dev_user_with_username(username),
        Some("fake-session-token".to_string()),
    )
}

fn dev_user_with_username(username: &str) -> User {
    let mut user = dev_user();
    user.username = username.to_string();
    user.display_name = None;
    user
}
