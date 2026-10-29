//! Captured-target confirmation and request-owned message deletion outcomes.
//! Pure transitions, like the channel operations in `state.rs`; the coordinator dispatches.
use super::state::{ChatState, Identity, Load};
use crate::api::ApiError;

const PREVIEW_CHARS: usize = 240;

#[derive(Clone)]
pub struct MessageDeletion {
    pub channel_id: String,
    pub message_id: String,
    pub preview: String,
    pub pending: bool,
    pub feedback: Option<String>,
    serial: u64,
}

#[derive(Clone)]
pub(crate) struct MessageDeleteRequest {
    pub generation: u64,
    pub channel_id: String,
    pub message_id: String,
    serial: u64,
}

impl ChatState {
    /// Resolve the captured identity from loaded history, never from a display name, current
    /// hover target, or caller-supplied preview. Only the session user's messages qualify.
    pub(super) fn open_message_deletion(
        &mut self,
        session: &Identity,
        user_id: &str,
        now: i64,
        channel_id: &str,
        message_id: &str,
    ) -> bool {
        if self.message_deletion.is_some()
            || session.session_generation().is_none()
            || now >= session.expires_at
        {
            return false;
        }
        let Some(Load::Ready(rows)) = self.history(channel_id) else {
            return false;
        };
        let Some(message) = rows
            .iter()
            .find(|m| m.id == message_id && m.channel_id == channel_id && m.author_id == user_id)
        else {
            return false;
        };
        let mut preview: String = message.text.chars().take(PREVIEW_CHARS).collect();
        if message.text.chars().count() > PREVIEW_CHARS {
            preview.push('…');
        }
        let serial = self.next_serial();
        self.message_deletion = Some(MessageDeletion {
            channel_id: channel_id.into(),
            message_id: message_id.into(),
            preview,
            pending: false,
            feedback: None,
            serial,
        });
        true
    }

    pub(super) fn cancel_message_deletion(&mut self) {
        if self.message_deletion.as_ref().is_some_and(|d| !d.pending) {
            self.message_deletion = None;
        }
    }

    pub(super) fn confirm_message_deletion(
        &mut self,
        session: &Identity,
        now: i64,
    ) -> Option<MessageDeleteRequest> {
        // The coordinator's next tick/delivery reports expiry to Session. No protected
        // work can start in the gap before that notification.
        if now >= session.expires_at {
            return None;
        }
        let generation = session.session_generation()?;
        if self.message_deletion.as_ref()?.pending {
            return None;
        }
        let serial = self.next_serial();
        let dialog = self.message_deletion.as_mut()?;
        dialog.pending = true;
        dialog.feedback = None;
        dialog.serial = serial;
        Some(MessageDeleteRequest {
            generation,
            channel_id: dialog.channel_id.clone(),
            message_id: dialog.message_id.clone(),
            serial,
        })
    }

    /// Returns whether `request` was the confirmation's current request.
    pub(super) fn complete_message_delete(
        &mut self,
        session: &mut Identity,
        request: &MessageDeleteRequest,
        result: Result<(), ApiError>,
    ) -> bool {
        // Authorization rejection still belongs to the originating session, even if
        // a live deletion already closed the dialog.
        if result == Err(ApiError::SessionRejected) {
            session.protected_rejected(request.generation);
            self.clear();
            return false;
        }
        let Some(dialog) = self.message_deletion.as_mut().filter(|d| {
            d.pending
                && d.serial == request.serial
                && d.channel_id == request.channel_id
                && d.message_id == request.message_id
        }) else {
            return false;
        };
        match result {
            Ok(()) => self.merge_message_deletion(&request.channel_id, &request.message_id),
            Err(error) => {
                dialog.pending = false;
                dialog.feedback = Some(match error {
                    ApiError::Forbidden => "You can only delete your own messages.",
                    ApiError::NotFound => "The message or its channel was not found.",
                    ApiError::InvalidInput | ApiError::Conflict | ApiError::InvalidCredentials | ApiError::InvalidRequest => "The server rejected this message deletion.",
                    ApiError::ServerFailure => "The server could not delete this message. Try again deliberately.",
                    _ => "Could not confirm deletion; it may have succeeded. Check the conversation before trying again.",
                }.into());
            }
        }
        true
    }
}
