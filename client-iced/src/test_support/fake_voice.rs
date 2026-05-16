use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::voice::{VoiceCommand, VoiceError, VoiceEvent};

#[derive(Debug, Clone, Default)]
pub struct FakeVoiceWorker {
    inner: Arc<Mutex<FakeVoiceWorkerInner>>,
}

#[derive(Debug, Clone, Default)]
struct FakeVoiceWorkerInner {
    events: VecDeque<VoiceEvent>,
    commands: Vec<VoiceCommand>,
    next_error: Option<VoiceError>,
}

impl FakeVoiceWorker {
    pub fn push(&self, event: VoiceEvent) -> Result<(), VoiceError> {
        self.lock()?.events.push_back(event);
        Ok(())
    }

    pub fn fail_next(&self, message: impl Into<String>) -> Result<(), VoiceError> {
        self.lock()?.next_error = Some(VoiceError::worker(message));
        Ok(())
    }

    pub fn commands(&self) -> Result<Vec<VoiceCommand>, VoiceError> {
        Ok(self.lock()?.commands.clone())
    }

    pub fn send(&self, command: VoiceCommand) -> Result<VoiceEvent, VoiceError> {
        let mut inner = self.lock()?;

        if let Some(error) = inner.next_error.take() {
            return Err(error);
        }

        inner.commands.push(command);
        Ok(VoiceEvent::CommandAccepted)
    }

    pub fn drain_events(&self) -> Result<Vec<VoiceEvent>, VoiceError> {
        let mut inner = self.lock()?;

        if let Some(error) = inner.next_error.take() {
            return Err(error);
        }

        Ok(inner.events.drain(..).collect())
    }

    fn lock(&self) -> Result<MutexGuard<'_, FakeVoiceWorkerInner>, VoiceError> {
        self.inner
            .lock()
            .map_err(|_| VoiceError::worker("fake voice worker lock poisoned"))
    }
}
