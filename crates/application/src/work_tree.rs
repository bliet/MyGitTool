use crate::ports::{ReadError, RepositoryReader};
use domain::{RepositoryInfo, WorkTreeChanges};
use std::sync::Arc;

/// Loads the staged and unstaged changes, each sorted by path.
#[derive(Clone)]
pub struct LoadWorkTreeChanges {
    reader: Arc<dyn RepositoryReader>,
}

impl LoadWorkTreeChanges {
    pub fn new(reader: Arc<dyn RepositoryReader>) -> Self {
        Self { reader }
    }

    pub fn load(&self, repo: &RepositoryInfo) -> Result<WorkTreeChanges, ReadError> {
        let mut changes = self.reader.work_tree_changes(repo)?;
        changes.staged.sort_by(|a, b| a.path.cmp(&b.path));
        changes.unstaged.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(changes)
    }
}
