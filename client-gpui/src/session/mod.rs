//! Session ownership home. Bound verification and credential saving live at this seam.
//! Task/storage lifecycle coordination remains in the temporary shell until its ownership ticket.

mod state;

pub(crate) use state::*;

impl Session {
    /// Credential material goes directly from the accepted context to the ordered worker.
    pub fn save(
        &self,
        store: &crate::storage::Persistence,
    ) -> async_channel::Receiver<crate::storage::Outcome> {
        store.save(
            crate::storage::Selection {
                server: self.server.clone(),
                user: self.user.clone(),
                expires_at: self.expires_at,
            },
            self.client().credential_for_session().to_owned(),
        )
    }
}

impl AppSession {
    /// Only accepted contexts are available to protected activity.
    pub fn active_client(&self) -> Option<crate::api::AuthenticatedClient> {
        self.active.as_ref().map(Session::client)
    }

    /// The candidate remains inside this future until current-user verification completes.
    /// Identity, expiry and generation still gate activation in `finish_restore`.
    pub async fn verify_saved(server: crate::api::ServerClient, token: String) -> RestoreResult {
        let Ok(client) = server.restore_candidate(token) else {
            return RestoreResult::MissingCredential;
        };
        match client.current_user().await {
            Ok(user) => RestoreResult::Verified { user, client },
            Err(AuthError::AlreadyInvalid) => RestoreResult::Rejected,
            _ => RestoreResult::Unavailable,
        }
    }
}

#[cfg(test)]
mod binding_tests;
