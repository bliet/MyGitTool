use crate::ports::{RecentRepositoriesStore, StorageError};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The number of recent repositories kept by default.
pub const DEFAULT_CAPACITY: usize = 10;

/// Maintains the list of recently opened repositories, most recent first and
/// without duplicates.
#[derive(Clone)]
pub struct RecentRepositories {
    store: Arc<dyn RecentRepositoriesStore>,
    capacity: usize,
}

impl RecentRepositories {
    pub fn new(store: Arc<dyn RecentRepositoriesStore>) -> Self {
        Self::with_capacity(store, DEFAULT_CAPACITY)
    }

    pub fn with_capacity(store: Arc<dyn RecentRepositoriesStore>, capacity: usize) -> Self {
        Self { store, capacity }
    }

    pub fn list(&self) -> Result<Vec<PathBuf>, StorageError> {
        let mut paths = self.store.load()?;
        paths.truncate(self.capacity);
        Ok(paths)
    }

    /// Moves `path` to the front of the list and returns the new list.
    pub fn record(&self, path: &Path) -> Result<Vec<PathBuf>, StorageError> {
        let mut paths = self.store.load()?;
        paths.retain(|p| p != path);
        paths.insert(0, path.to_path_buf());
        paths.truncate(self.capacity);
        self.store.save(&paths)?;
        Ok(paths)
    }

    /// Removes `path` from the list and returns the new list.
    pub fn forget(&self, path: &Path) -> Result<Vec<PathBuf>, StorageError> {
        let mut paths = self.store.load()?;
        paths.retain(|p| p != path);
        self.store.save(&paths)?;
        Ok(paths)
    }
}
