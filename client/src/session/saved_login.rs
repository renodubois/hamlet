//! Session-lifetime saved-login policy. Storage alone owns ordered provider/file mechanics.
//! Auth generations gate restoration; save serials and deletion IDs have separate lifetimes.
use super::{RestoreDecision, RestoreResult, SessionCoordinator, SessionUpdate, Update};
use crate::storage::{Config, Outcome, Persistence, Selection, account};
use std::time::Duration;

// Each workflow gets its own budget. Restoration shares ONE budget between read and /me.
const SECURE_STORE_DEADLINE: Duration = Duration::from_secs(10);

pub(crate) enum StorageRetry {
    Deletion,
    Restoration,
}

struct Deletion {
    selection: Selection,
    // Deduplicate only within one cleanup batch, never across a newer save/logout.
    batch: u64,
    id: u64,
    in_flight: bool,
}

pub(super) struct SavedLogin {
    persistence: Option<Persistence>,
    initial_username: String,
    credential: Option<Selection>,
    retained_credentials: Vec<Selection>,
    feedback: Option<String>,
    cleanup_feedback: Option<String>,
    serial: u64,
    deletions: Vec<Deletion>,
    deletion_id: u64,
}

pub(super) enum SavedUpdate {
    Delete {
        id: u64,
        outcome: Option<Outcome>,
    },
    Save {
        serial: u64,
        selection: Selection,
        previous: Option<Selection>,
        remembered: Option<Outcome>,
        outcome: Option<Outcome>,
    },
    Restore {
        generation: u64,
        selection: Selection,
        result: RestoreResult,
    },
}

impl SavedLogin {
    pub(super) fn new(config: Config, persistence: Option<Persistence>, server: &str) -> Self {
        let deletion_id = config.pending_deletions.len() as u64;
        // Keep noncandidate metadata for replacement/logout cleanup. Exact server
        // matching gates restoration and prefill, not ownership of the stored identity.
        let credential = config.saved;
        let initial_username = credential
            .as_ref()
            .filter(|s| s.server == server)
            .map(|s| s.user.username.clone())
            .unwrap_or_default();
        Self {
            persistence,
            initial_username,
            credential,
            retained_credentials: Vec::new(),
            feedback: None,
            cleanup_feedback: if config.pending_deletions.is_empty() {
                None
            } else {
                Some("Previous saved login cleanup is pending; retry deletion if it fails.".into())
            },
            serial: 0,
            deletions: config
                .pending_deletions
                .into_iter()
                .enumerate()
                .map(|(i, selection)| Deletion {
                    selection,
                    batch: 0,
                    id: i as u64 + 1,
                    in_flight: false,
                })
                .collect(),
            deletion_id,
        }
    }
}

impl SessionCoordinator {
    /// Startup prefill only; editable form values remain exclusively in the login controls.
    pub fn initial_username(&self) -> &str {
        &self.saved.initial_username
    }

    pub fn storage_feedback(&self) -> Option<String> {
        // A new save/restore cannot erase unrelated cleanup, nor can late cleanup
        // hide a current memory-only/unconfirmed save. One unchanged UI status shows both.
        match (&self.saved.feedback, &self.saved.cleanup_feedback) {
            (Some(current), Some(cleanup)) => Some(format!("{current} {cleanup}")),
            (Some(current), None) => Some(current.clone()),
            (None, cleanup) => cleanup.clone(),
        }
    }

    pub fn storage_retry(&self) -> Option<StorageRetry> {
        if self.saved.persistence.is_none() || self.pending() {
            return None;
        }
        if self.retryable_deletion().is_some() {
            Some(StorageRetry::Deletion)
        } else if self.active().is_none()
            && self
                .saved
                .credential
                .as_ref()
                .is_some_and(|s| s.server == self.server())
        {
            Some(StorageRetry::Restoration)
        } else {
            None
        }
    }

    pub fn retry_storage(&mut self) {
        match self.storage_retry() {
            Some(StorageRetry::Deletion) => self.run_deletion(self.retryable_deletion().unwrap()),
            Some(StorageRetry::Restoration) => self.start_restore(),
            None => {}
        }
    }

    pub(super) fn start_saved_login(&mut self) {
        let ids: Vec<_> = self.saved.deletions.iter().map(|d| d.id).collect();
        for id in ids {
            self.run_deletion(id);
        }
        self.start_restore();
    }

    fn queue_deletion(&mut self, selection: Selection) {
        if self.saved.persistence.is_none()
            || self
                .saved
                .deletions
                .iter()
                .any(|d| d.selection == selection && d.batch == self.saved.serial)
        {
            return;
        }
        self.saved.deletion_id = self.saved.deletion_id.wrapping_add(1);
        let id = self.saved.deletion_id;
        self.saved.deletions.push(Deletion {
            selection,
            batch: self.saved.serial,
            id,
            in_flight: false,
        });
        self.run_deletion(id);
    }

    fn active_account(&self) -> Option<String> {
        self.active().map(|s| {
            account(&Selection {
                server: s.server.clone(),
                user: s.user.clone(),
                expires_at: s.expires_at,
            })
        })
    }

    fn retryable_deletion(&self) -> Option<u64> {
        let active = self.active_account();
        self.saved
            .deletions
            .iter()
            .find(|d| !d.in_flight && active.as_deref() != Some(account(&d.selection).as_str()))
            .map(|d| d.id)
    }

    fn refresh_cleanup_feedback(&mut self) {
        self.saved.cleanup_feedback = Some(if self.saved.deletions.iter().any(|d| !d.in_flight) {
            "Could not confirm deletion from Secret Service; a saved login may remain. Unlock the store and retry deletion.".into()
        } else if !self.saved.deletions.is_empty() {
            "Removing saved login from Secret Service…".into()
        } else if self.active().is_some() {
            "Previous saved login removed from Secret Service; current session is unchanged.".into()
        } else {
            "Saved login removed from Secret Service.".into()
        });
    }

    fn run_deletion(&mut self, id: u64) {
        let active = self.active_account();
        let Some(deletion) = self
            .saved
            .deletions
            .iter_mut()
            .find(|d| d.id == id && !d.in_flight)
        else {
            return;
        };
        // A retry for an old login must never target a newer active login's account.
        if active.as_deref() == Some(account(&deletion.selection).as_str()) {
            return;
        }
        let Some(store) = &self.saved.persistence else {
            return;
        };
        deletion.in_flight = true;
        let queued = store.delete(deletion.selection.clone());
        let reply =
            self.execution.bounded(
                SECURE_STORE_DEADLINE,
                async move { queued.recv().await.ok() },
            );
        self.refresh_cleanup_feedback();
        let deliver = self.deliver.clone();
        self.execution.spawn(async move {
            let outcome = reply.recv().await.ok().flatten().flatten();
            let _ = deliver
                .send(SessionUpdate(Update::Saved(SavedUpdate::Delete {
                    id,
                    outcome,
                })))
                .await;
        });
    }

    pub(super) fn invalidate_storage(&mut self) {
        self.saved.feedback = None;
        self.saved.serial = self.saved.serial.wrapping_add(1);
        if let Some(store) = &self.saved.persistence {
            store.invalidate();
        }
        if let Some(selection) = self.saved.credential.take() {
            self.queue_deletion(selection);
        }
        for selection in std::mem::take(&mut self.saved.retained_credentials) {
            self.queue_deletion(selection);
        }
    }

    pub(super) fn save_login(&mut self) {
        let Some(store) = &self.saved.persistence else {
            return;
        };
        let Some(session) = self.active() else {
            return;
        };
        let selection = Selection {
            server: session.server.clone(),
            user: session.user.clone(),
            expires_at: session.expires_at,
        };
        // Submit the ordered operations before borrowing workflow state mutably.
        let remembered = store.remember(selection.server.clone());
        let queued = session.save(store);
        let previous = self.saved.credential.replace(selection.clone());
        // A failed/invalidated same-account replacement can leave the OLD metadata
        // selection in place. Keep that exact cleanup identity as well as the attempt.
        if let Some(old) = previous.as_ref()
            && old != &selection
            && !self.saved.retained_credentials.contains(old)
        {
            self.saved.retained_credentials.push(old.clone());
        }
        let reply = self.execution.bounded(SECURE_STORE_DEADLINE, async move {
            (remembered.recv().await.ok(), queued.recv().await.ok())
        });
        self.saved.serial = self.saved.serial.wrapping_add(1);
        let serial = self.saved.serial;
        self.saved.feedback = Some("Saving login to Secret Service…".into());
        let deliver = self.deliver.clone();
        self.execution.spawn(async move {
            let (remembered, outcome) = reply.recv().await.ok().flatten().unwrap_or((None, None));
            let _ = deliver
                .send(SessionUpdate(Update::Saved(SavedUpdate::Save {
                    serial,
                    selection,
                    previous,
                    remembered,
                    outcome,
                })))
                .await;
        });
    }

    fn start_restore(&mut self) {
        let Some(store) = &self.saved.persistence else {
            return;
        };
        let Some(selection) = self.saved.credential.clone() else {
            return;
        };
        if self.active().is_some() || self.pending() || selection.server != self.server() {
            return;
        }
        let generation = self.state.begin_restore();
        if selection.expires_at <= self.execution.unix_seconds() {
            self.finish_saved_restore(generation, &selection, RestoreResult::Unavailable);
            return;
        }
        let reply = store.read(selection.clone());
        let server = self.api.server(&selection.server);
        let result = self.execution.bounded(SECURE_STORE_DEADLINE, async move {
            match reply.recv().await {
                Ok(Outcome::Token(Some(token))) => match server {
                    Ok(server) => SessionCoordinator::verify_saved(server, token).await,
                    Err(_) => RestoreResult::Unavailable,
                },
                Ok(Outcome::Token(None)) => RestoreResult::MissingCredential,
                _ => RestoreResult::Unavailable,
            }
        });
        let deliver = self.deliver.clone();
        self.execution.spawn(async move {
            let result = result
                .recv()
                .await
                .ok()
                .flatten()
                .unwrap_or(RestoreResult::Unavailable);
            let _ = deliver
                .send(SessionUpdate(Update::Saved(SavedUpdate::Restore {
                    generation,
                    selection,
                    result,
                })))
                .await;
        });
    }

    fn finish_saved_restore(
        &mut self,
        generation: u64,
        selection: &Selection,
        result: RestoreResult,
    ) {
        match self.state.finish_restore(
            generation,
            &selection.server,
            &selection.user,
            selection.expires_at,
            result,
            self.execution.unix_seconds(),
        ) {
            RestoreDecision::Restored => {
                self.saved.feedback = Some("Login restored from Secret Service.".into());
                self.activated(false);
            }
            RestoreDecision::Delete => self.invalidate_storage(),
            RestoreDecision::Retry => {
                self.saved.feedback = Some(
                    "Could not verify saved login; use a memory-only login or retry restoration."
                        .into(),
                )
            }
            RestoreDecision::Stale => {}
        }
    }

    pub(super) fn apply_saved(&mut self, update: SavedUpdate) {
        match update {
            SavedUpdate::Delete { id, outcome } => {
                let Some(index) = self.saved.deletions.iter().position(|d| d.id == id) else {
                    return;
                };
                if matches!(outcome, Some(Outcome::Deleted)) {
                    let deleted_account = account(&self.saved.deletions[index].selection);
                    // This ordered deletion also settles earlier attempts for that key
                    // (including a stale replacement selection after rollback). Never
                    // retire a later identity: a newer save/logout may follow this result.
                    for earlier in (0..=index).rev() {
                        if account(&self.saved.deletions[earlier].selection) == deleted_account {
                            self.saved.deletions.remove(earlier);
                        }
                    }
                } else {
                    self.saved.deletions[index].in_flight = false;
                }
                self.refresh_cleanup_feedback();
            }
            SavedUpdate::Save {
                serial,
                selection,
                previous,
                remembered,
                outcome,
            } => {
                if self.saved.serial != serial || self.active().is_none() {
                    return;
                }
                if matches!(
                    outcome,
                    Some(Outcome::Saved | Outcome::SavedWithCleanupWarning)
                ) {
                    self.saved
                        .deletions
                        .retain(|d| account(&d.selection) != account(&selection));
                    if self.saved.deletions.is_empty() {
                        self.saved.cleanup_feedback = None;
                    } else {
                        self.refresh_cleanup_feedback();
                    }
                    if let Some(old) = previous {
                        self.saved
                            .retained_credentials
                            .retain(|s| account(s) != account(&old));
                        if matches!(outcome, Some(Outcome::SavedWithCleanupWarning)) {
                            self.queue_deletion(old);
                        }
                    }
                }
                self.saved.feedback = Some(match outcome {
                    Some(Outcome::Saved) => "Login saved in Secret Service for this server and user.".into(),
                    Some(Outcome::SavedWithCleanupWarning) => "Login saved, but a previous server/user credential could not be removed from Secret Service.".into(),
                    Some(Outcome::Failed) if remembered == Some(Outcome::Remembered) => "Secure storage failed; this session is memory-only. A previous saved login may remain until deletion is confirmed.".into(),
                    Some(Outcome::Failed) => "Secure storage and server preference could not be saved; this session is memory-only.".into(),
                    _ => "Secure storage timed out or was interrupted; saving is unconfirmed. Continue in memory, but a credential may still appear in Secret Service. Log out to request deletion.".into(),
                });
            }
            SavedUpdate::Restore {
                generation,
                selection,
                result,
            } => self.finish_saved_restore(generation, &selection, result),
        }
    }
}
