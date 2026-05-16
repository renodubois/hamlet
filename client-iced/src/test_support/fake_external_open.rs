use std::sync::{Arc, Mutex, MutexGuard};

use crate::external_open::{ExternalOpenError, ExternalOpenService, validate_external_url};

#[derive(Debug, Clone, Default)]
pub struct FakeExternalOpen {
    inner: Arc<Mutex<FakeExternalOpenInner>>,
}

#[derive(Debug, Clone, Default)]
struct FakeExternalOpenInner {
    opened_urls: Vec<String>,
    next_error: Option<ExternalOpenError>,
}

impl FakeExternalOpen {
    pub fn fail_next(&self, error: ExternalOpenError) -> Result<(), ExternalOpenError> {
        self.lock()?.next_error = Some(error);
        Ok(())
    }

    pub fn opened_urls(&self) -> Result<Vec<String>, ExternalOpenError> {
        Ok(self.lock()?.opened_urls.clone())
    }

    fn lock(&self) -> Result<MutexGuard<'_, FakeExternalOpenInner>, ExternalOpenError> {
        self.inner.lock().map_err(|_| {
            ExternalOpenError::Platform("fake external-open lock poisoned".to_string())
        })
    }
}

impl ExternalOpenService for FakeExternalOpen {
    fn open_external_url(&self, target: &str) -> Result<(), ExternalOpenError> {
        let url = validate_external_url(target)?;
        let mut inner = self.lock()?;

        if let Some(error) = inner.next_error.take() {
            return Err(error);
        }

        inner.opened_urls.push(url.as_str().to_string());
        Ok(())
    }
}
