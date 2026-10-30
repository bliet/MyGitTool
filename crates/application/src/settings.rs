use crate::ports::{Settings, SettingsStore, StorageError};
use std::sync::Arc;

/// Loads and saves the user's settings.
#[derive(Clone)]
pub struct UserSettings {
    store: Arc<dyn SettingsStore>,
}

impl UserSettings {
    pub fn new(store: Arc<dyn SettingsStore>) -> Self {
        Self { store }
    }

    /// The stored settings, or the defaults if none were stored yet.
    pub fn load(&self) -> Result<Settings, StorageError> {
        Ok(self.store.load()?.unwrap_or_default())
    }

    pub fn save(&self, settings: &Settings) -> Result<(), StorageError> {
        self.store.save(settings)
    }
}
