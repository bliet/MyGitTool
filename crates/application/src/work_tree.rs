use crate::ports::{ReadError, RepositoryReader};
use domain::{FileChange, RepositoryInfo};
use std::sync::Arc;

/// Loads the uncommitted changes, sorted by path.
#[derive(Clone)]
pub struct LoadWorkTreeChanges {
    reader: Arc<dyn RepositoryReader>,
}

impl LoadWorkTreeChanges {
    pub fn new(reader: Arc<dyn RepositoryReader>) -> Self {
        Self { reader }
    }

    pub fn load(&self, repo: &RepositoryInfo) -> Result<Vec<FileChange>, ReadError> {
        let mut changes = self.reader.work_tree_changes(repo)?;
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(changes)
    }
}
