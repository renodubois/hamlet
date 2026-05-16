use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::auth::AuthenticatedRequest;
use crate::realtime::{RealtimeCall, RealtimeClient, RealtimeError, RealtimeEvent};

#[derive(Debug, Clone, Default)]
pub struct FakeRealtime {
    inner: Arc<Mutex<FakeRealtimeInner>>,
}

#[derive(Debug, Clone, Default)]
struct FakeRealtimeInner {
    events: VecDeque<RealtimeEvent>,
    calls: Vec<RealtimeCall>,
    next_error: Option<RealtimeError>,
    connected: bool,
}

impl FakeRealtime {
    pub fn push(&self, event: RealtimeEvent) -> Result<(), RealtimeError> {
        self.lock()?.events.push_back(event);
        Ok(())
    }

    pub fn fail_next(&self, message: impl Into<String>) -> Result<(), RealtimeError> {
        self.lock()?.next_error = Some(RealtimeError::Fake(message.into()));
        Ok(())
    }

    pub fn calls(&self) -> Result<Vec<RealtimeCall>, RealtimeError> {
        Ok(self.lock()?.calls.clone())
    }

    fn take_next_error(inner: &mut FakeRealtimeInner) -> Result<(), RealtimeError> {
        if let Some(error) = inner.next_error.take() {
            return Err(error);
        }

        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, FakeRealtimeInner>, RealtimeError> {
        self.inner
            .lock()
            .map_err(|_| RealtimeError::Fake("fake realtime lock poisoned".to_string()))
    }
}

impl RealtimeClient for FakeRealtime {
    fn connect(&self, request: AuthenticatedRequest) -> Result<(), RealtimeError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.connected = true;
        inner.calls.push(RealtimeCall::Connect {
            server_url: request.server_url,
            has_session: request.session_token.is_some(),
        });

        Ok(())
    }

    fn disconnect(&self) -> Result<(), RealtimeError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.connected = false;
        inner.calls.push(RealtimeCall::Disconnect);

        Ok(())
    }

    fn drain_events(&self) -> Result<Vec<RealtimeEvent>, RealtimeError> {
        let mut inner = self.lock()?;

        Self::take_next_error(&mut inner)?;
        inner.calls.push(RealtimeCall::DrainEvents);

        Ok(inner.events.drain(..).collect())
    }
}
