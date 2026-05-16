use std::sync::{Arc, Mutex, MutexGuard};

use crate::storage::{Preferences, Storage, StorageError};

#[derive(Debug, Clone)]
pub struct FakeStorage {
    inner: Arc<Mutex<FakeStorageInner>>,
}

#[derive(Debug, Clone)]
struct FakeStorageInner {
    preferences: Preferences,
    load_error: Option<StorageError>,
    save_error: Option<StorageError>,
    saved_preferences: Vec<Preferences>,
}

impl FakeStorage {
    pub fn new(preferences: Preferences) -> Self {
        Self {
            inner: Arc::new(Mutex::new(FakeStorageInner {
                preferences,
                load_error: None,
                save_error: None,
                saved_preferences: Vec::new(),
            })),
        }
    }

    pub fn with_default_preferences() -> Self {
        Self::new(Preferences::default())
    }

    pub fn fail_load(&self, message: impl Into<String>) -> Result<(), StorageError> {
        self.lock()?.load_error = Some(StorageError::Fake(message.into()));
        Ok(())
    }

    pub fn fail_save(&self, message: impl Into<String>) -> Result<(), StorageError> {
        self.lock()?.save_error = Some(StorageError::Fake(message.into()));
        Ok(())
    }

    pub fn saved_preferences(&self) -> Result<Vec<Preferences>, StorageError> {
        Ok(self.lock()?.saved_preferences.clone())
    }

    fn lock(&self) -> Result<MutexGuard<'_, FakeStorageInner>, StorageError> {
        self.inner
            .lock()
            .map_err(|_| StorageError::Fake("fake storage lock poisoned".to_string()))
    }
}

impl Default for FakeStorage {
    fn default() -> Self {
        Self::with_default_preferences()
    }
}

impl Storage for FakeStorage {
    fn load_preferences(&self) -> Result<Preferences, StorageError> {
        let inner = self.lock()?;

        if let Some(error) = &inner.load_error {
            return Err(error.clone());
        }

        Ok(inner.preferences.clone())
    }

    fn save_preferences(&self, preferences: &Preferences) -> Result<(), StorageError> {
        let mut inner = self.lock()?;

        if let Some(error) = &inner.save_error {
            return Err(error.clone());
        }

        inner.preferences = preferences.clone();
        inner.saved_preferences.push(preferences.clone());

        Ok(())
    }
}
