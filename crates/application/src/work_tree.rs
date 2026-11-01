use crate::ports::{ReadError, RepositoryReader, RepositoryWriter};
use crate::sync::OperationError;
use domain::{DirtyWorkTree, RepositoryInfo, WorkTreeChanges};
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

/// Turns the uncommitted changes into a patch file.
#[derive(Clone)]
pub struct CreatePatch {
    reader: Arc<dyn RepositoryReader>,
}

impl CreatePatch {
    pub fn new(reader: Arc<dyn RepositoryReader>) -> Self {
        Self { reader }
    }

    /// The staged, unstaged and untracked changes as a patch.
    pub fn patch(&self, repo: &RepositoryInfo) -> Result<Vec<u8>, OperationError> {
        let patch = self.reader.work_tree_patch(repo)?;
        if patch.is_empty() {
            return Err(OperationError::NothingToPatch);
        }
        Ok(patch)
    }
}

/// Applies a patch file to the work tree.
#[derive(Clone)]
pub struct ApplyPatch {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
}

impl ApplyPatch {
    pub fn new(reader: Arc<dyn RepositoryReader>, writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { reader, writer }
    }

    /// Applies `patch` to a clean work tree. The changes are left
    /// unstaged.
    pub fn apply(&self, repo: &RepositoryInfo, patch: &[u8]) -> Result<(), OperationError> {
        if patch.iter().all(u8::is_ascii_whitespace) {
            return Err(OperationError::EmptyPatch);
        }
        if let Some(operation) = self.reader.ongoing_operation(repo)? {
            return Err(OperationError::OperationInProgress(operation));
        }
        let files = self.reader.dirty_files(repo)?;
        if !files.is_empty() {
            return Err(DirtyWorkTree { files }.into());
        }
        Ok(self.writer.apply_patch(repo, patch)?)
    }
}
