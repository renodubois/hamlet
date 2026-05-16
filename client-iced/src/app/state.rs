use crate::auth::{AuthSession, AuthStatus, SignedInState, SignedOutState};
use crate::storage::{Preferences, VoiceDevicePreferences};

use super::route::Route;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppState {
    pub boot_status: BootStatus,
    pub route: Route,
    pub signed_out: SignedOutState,
    pub signed_in: Option<SignedInState>,
}

impl AppState {
    pub fn booting() -> Self {
        Self {
            boot_status: BootStatus::LoadingPreferences,
            route: Route::SignedOut,
            signed_out: SignedOutState::default(),
            signed_in: None,
        }
    }

    pub fn apply_preferences(&mut self, preferences: &Preferences) {
        self.signed_out = SignedOutState::new(preferences);
        self.signed_in = None;
        self.boot_status = BootStatus::Ready;
        self.route = Route::SignedOut;
    }

    pub fn begin_session_restore(&mut self, preferences: &Preferences) {
        self.signed_out = SignedOutState::new(preferences);
        self.signed_in = None;
        self.boot_status = BootStatus::RestoringSession;
        self.route = Route::SignedOut;
    }

    pub fn sign_in(
        &mut self,
        session: AuthSession,
        server_url: String,
        voice_preferences: VoiceDevicePreferences,
    ) {
        self.signed_out.password.clear();
        self.signed_out.auth_status = AuthStatus::Idle;
        self.signed_out.notice = None;
        self.signed_in = Some(SignedInState::new(session, server_url, voice_preferences));
        self.boot_status = BootStatus::Ready;
        self.route = Route::SignedIn;
    }

    pub fn return_to_signed_out(&mut self, preferences: &Preferences, notice: Option<String>) {
        self.signed_out = SignedOutState::new(preferences);
        self.signed_out.notice = notice;
        self.signed_in = None;
        self.boot_status = BootStatus::Ready;
        self.route = Route::SignedOut;
    }

    pub fn is_loading_preferences(&self) -> bool {
        self.boot_status == BootStatus::LoadingPreferences
    }

    pub fn is_restoring_session(&self) -> bool {
        self.boot_status == BootStatus::RestoringSession
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootStatus {
    LoadingPreferences,
    RestoringSession,
    Ready,
}
