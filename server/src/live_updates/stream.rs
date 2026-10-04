use super::Subscription;
use crate::http::auth::Session;
use actix_web::web::Bytes;
use sea_orm::DatabaseConnection;
use std::time::Duration;
use tokio::time::{Instant, sleep_until};

const INTERVAL: Duration = Duration::from_secs(15);

fn expiry_deadline(session: &Session) -> Instant {
    Instant::now()
        + (session.expires_at() - chrono::Utc::now())
            .to_std()
            .unwrap_or_default()
}

pub(super) struct LiveStream {
    subscription: Subscription,
    heartbeat: Instant,
    validate_at: Instant,
    expires_at: Instant,
    db: DatabaseConnection,
    session: Session,
}

impl LiveStream {
    pub(super) fn new(
        subscription: Subscription,
        db: DatabaseConnection,
        session: Session,
    ) -> Self {
        let next = Instant::now() + INTERVAL;
        let expires_at = expiry_deadline(&session);
        Self {
            subscription,
            heartbeat: next,
            validate_at: next,
            expires_at,
            db,
            session,
        }
    }

    pub(super) async fn next_frame(&mut self) -> Option<Bytes> {
        loop {
            // Check deadlines directly: a ready event must not outrun timer-wheel wakeup.
            if Instant::now() >= self.expires_at || chrono::Utc::now() >= self.session.expires_at()
            {
                return None;
            }
            if Instant::now() >= self.validate_at {
                let validation = tokio::select! {
                    biased;
                    _ = sleep_until(self.expires_at) => return None,
                    result = self.session.validate(&self.db) => result,
                };
                let Ok(Some(identity)) = validation else {
                    return None;
                };
                self.expires_at = self.expires_at.min(expiry_deadline(&identity.session));
                self.session = identity.session;
                self.validate_at = Instant::now() + INTERVAL;
                continue;
            }
            if self.subscription.is_terminated() {
                return None;
            }
            if !self.subscription.has_started() {
                return self.subscription.next_frame().await;
            }
            tokio::select! {
                biased;
                _ = sleep_until(self.expires_at) => return None,
                _ = sleep_until(self.validate_at) => {}
                _ = sleep_until(self.heartbeat) => {
                    self.heartbeat = Instant::now() + INTERVAL;
                    return Some(Bytes::from_static(b": heartbeat\n\n"));
                }
                frame = self.subscription.next_frame() => return frame,
            }
        }
    }
}
