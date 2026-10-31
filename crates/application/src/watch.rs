use crate::ports::{RepoChanges, RepoWatcher, WatchError, WatchGuard};
use domain::RepositoryInfo;
use std::sync::Arc;

/// Watches a repository so the UI can refresh on outside changes.
#[derive(Clone)]
pub struct WatchRepository {
    watcher: Arc<dyn RepoWatcher>,
}

impl WatchRepository {
    pub fn new(watcher: Arc<dyn RepoWatcher>) -> Self {
        Self { watcher }
    }

    /// Calls `on_change` after changes to `repo` until the guard is dropped.
    /// Bursts of changes arrive as one call.
    pub fn watch(
        &self,
        repo: &RepositoryInfo,
        on_change: impl Fn(RepoChanges) + Send + 'static,
    ) -> Result<WatchGuard, WatchError> {
        self.watcher.watch(repo, Box::new(on_change))
    }
}
