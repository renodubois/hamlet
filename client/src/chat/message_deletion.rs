//! Captured-target confirmation and session/request-owned deletion outcomes.
use super::{state::*, *};

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

impl ChatHandle {
    pub fn owns_message(&self, message: &Message) -> bool {
        let owner = self.0.borrow();
        owner.client.is_some() && owner.user_id == message.author_id
    }

    /// Shared opening seam for message actions. Resolve the captured identity from Chat,
    /// never trust a display name, current hover target, or caller-supplied preview.
    pub fn open_message_deletion(&self, channel_id: &str, message_id: &str) -> bool {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        if owner.client.is_none()
            || owner.state.message_deletion.is_some()
            || owner.identity.session_generation().is_none()
            || owner.execution.unix_seconds() >= owner.identity.expires_at
        {
            return false;
        }
        let Some(Load::Ready(rows)) = owner.state.history.get(channel_id) else {
            return false;
        };
        let Some(message) = rows.iter().find(|m| {
            m.id == message_id && m.channel_id == channel_id && m.author_id == owner.user_id
        }) else {
            return false;
        };
        let mut preview: String = message.text.chars().take(240).collect();
        if message.text.chars().count() > 240 {
            preview.push('…');
        }
        owner.state.message_delete_serial = owner.state.message_delete_serial.wrapping_add(1);
        owner.state.message_deletion = Some(MessageDeletion {
            channel_id: channel_id.into(),
            message_id: message_id.into(),
            preview,
            pending: false,
            feedback: None,
            serial: owner.state.message_delete_serial,
        });
        true
    }

    pub fn cancel_message_deletion(&self) {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        if owner
            .state
            .message_deletion
            .as_ref()
            .is_some_and(|d| !d.pending)
        {
            owner.state.message_deletion = None;
        }
    }

    pub fn confirm_message_deletion(&self) {
        let _notify = Notify(self);
        let mut owner = self.0.borrow_mut();
        let Some(api) = owner.client.clone() else {
            return;
        };
        // The coordinator's next tick/delivery reports expiry to Session. No protected
        // work can start in the gap before that notification.
        if owner.execution.unix_seconds() >= owner.identity.expires_at {
            return;
        }
        let Some(generation) = owner.identity.session_generation() else {
            return;
        };
        let Some(dialog) = owner.state.message_deletion.as_mut() else {
            return;
        };
        if dialog.pending {
            return;
        }
        dialog.pending = true;
        dialog.feedback = None;
        dialog.serial = dialog.serial.wrapping_add(1);
        let request = MessageDeleteRequest {
            generation,
            channel_id: dialog.channel_id.clone(),
            message_id: dialog.message_id.clone(),
            serial: dialog.serial,
        };
        owner.state.message_delete_serial = request.serial;
        let work = request.clone();
        let deliver = owner.deliver.clone();
        let timeout = owner.execution.sleep(READ_SEND_DEADLINE);
        let (task, _) = owner.execution.start(async move {
            let result = tokio::select! {
                biased;
                _ = timeout => Err(ApiError::Unavailable),
                result = api.delete_message(work.channel_id, work.message_id) => result,
            };
            let _ = deliver
                .send(ChatUpdate(Update::MessageDelete(request, result)))
                .await;
        });
        owner.message_delete_task = Some(task);
    }
}

impl Coordinator {
    pub(super) fn complete_message_delete(
        &mut self,
        request: MessageDeleteRequest,
        result: Result<(), ApiError>,
    ) {
        if request.generation != self.generation {
            return;
        }
        // Authorization rejection still belongs to the originating session, even if
        // a live deletion already closed the dialog.
        if result == Err(ApiError::AlreadyInvalid) {
            self.identity.protected_rejected(request.generation);
            self.state.clear();
            return;
        }
        let current = self.state.message_deletion.as_ref().is_some_and(|d| {
            d.pending
                && d.serial == request.serial
                && d.channel_id == request.channel_id
                && d.message_id == request.message_id
        });
        if !current {
            return;
        }
        self.message_delete_task = None;
        match result {
            Ok(()) => self
                .state
                .merge_message_deletion(&request.channel_id, &request.message_id),
            Err(error) => {
                let dialog = self
                    .state
                    .message_deletion
                    .as_mut()
                    .expect("current confirmation");
                dialog.pending = false;
                dialog.feedback = Some(match error {
                    ApiError::Forbidden => "You can only delete your own messages.",
                    ApiError::NotFound => "The message or its channel was not found.",
                    ApiError::InvalidInput | ApiError::Conflict | ApiError::InvalidCredentials => "The server rejected this message deletion.",
                    ApiError::ServerFailure => "The server could not delete this message. Try again deliberately.",
                    _ => "Could not confirm deletion; it may have succeeded. Check the conversation before trying again.",
                }.into());
            }
        }
    }
}
