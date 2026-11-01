//! Saving, applying and dropping stashes.

use crate::ports::{Integration, RepositoryReader, RepositoryWriter};
use crate::sync::OperationError;
use domain::{DirtyWorkTree, RepositoryInfo, Stash};
use std::sync::Arc;

/// What applying a stash did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StashOutcome {
    Applied,
    /// The stash was applied with conflicts in these files, sorted. They
    /// are resolved by editing and staging them; a pop keeps the stash.
    Conflicts(Vec<String>),
}

/// Stashes the local changes.
#[derive(Clone)]
pub struct SaveStash {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
}

impl SaveStash {
    pub fn new(reader: Arc<dyn RepositoryReader>, writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { reader, writer }
    }

    /// Stashes the changes to tracked files, and the untracked files if
    /// `include_untracked`. A blank message lets Git name the stash.
    pub fn save(
        &self,
        repo: &RepositoryInfo,
        message: &str,
        include_untracked: bool,
    ) -> Result<(), OperationError> {
        if let Some(operation) = self.reader.ongoing_operation(repo)? {
            return Err(OperationError::OperationInProgress(operation));
        }
        let tracked_changes = !self.reader.dirty_files(repo)?.is_empty();
        let any_changes = || -> Result<bool, OperationError> {
            Ok(!self.reader.work_tree_changes(repo)?.is_empty())
        };
        if !tracked_changes && !(include_untracked && any_changes()?) {
            return Err(OperationError::NothingToStash);
        }
        let message = message.trim();
        let message = (!message.is_empty()).then_some(message);
        Ok(self.writer.stash_save(repo, message, include_untracked)?)
    }
}

/// Applies a stash to the work tree, keeping or dropping it.
#[derive(Clone)]
pub struct ApplyStash {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
}

impl ApplyStash {
    pub fn new(reader: Arc<dyn RepositoryReader>, writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { reader, writer }
    }

    /// Applies `stash` and keeps it.
    pub fn apply(
        &self,
        repo: &RepositoryInfo,
        stash: &Stash,
    ) -> Result<StashOutcome, OperationError> {
        let index = self.prepare(repo, stash)?;
        Ok(outcome(self.writer.stash_apply(repo, index)?))
    }

    /// Applies `stash` and drops it, unless applying it left conflicts.
    pub fn pop(
        &self,
        repo: &RepositoryInfo,
        stash: &Stash,
    ) -> Result<StashOutcome, OperationError> {
        let index = self.prepare(repo, stash)?;
        let outcome = outcome(self.writer.stash_apply(repo, index)?);
        if outcome == StashOutcome::Applied {
            self.writer.stash_drop(repo, index)?;
        }
        Ok(outcome)
    }

    /// Checks that the stash can be applied and returns its current index.
    fn prepare(&self, repo: &RepositoryInfo, stash: &Stash) -> Result<usize, OperationError> {
        if let Some(operation) = self.reader.ongoing_operation(repo)? {
            return Err(OperationError::OperationInProgress(operation));
        }
        let files = self.reader.dirty_files(repo)?;
        if !files.is_empty() {
            return Err(DirtyWorkTree { files }.into());
        }
        current_index(self.reader.as_ref(), repo, stash)
    }
}

/// Deletes stashes.
#[derive(Clone)]
pub struct DropStash {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
}

impl DropStash {
    pub fn new(reader: Arc<dyn RepositoryReader>, writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { reader, writer }
    }

    pub fn drop_stash(&self, repo: &RepositoryInfo, stash: &Stash) -> Result<(), OperationError> {
        let index = current_index(self.reader.as_ref(), repo, stash)?;
        Ok(self.writer.stash_drop(repo, index)?)
    }
}

/// Where `stash` is in the list now. Indexes shift when stashes are saved
/// or dropped outside the app, so the stash is found by its commit.
fn current_index(
    reader: &dyn RepositoryReader,
    repo: &RepositoryInfo,
    stash: &Stash,
) -> Result<usize, OperationError> {
    reader
        .stashes(repo)?
        .into_iter()
        .find(|current| current.id == stash.id)
        .map(|current| current.index)
        .ok_or_else(|| OperationError::StashNotFound(stash.name()))
}

fn outcome(integration: Integration) -> StashOutcome {
    match integration {
        Integration::Completed => StashOutcome::Applied,
        Integration::Conflicts(files) => StashOutcome::Conflicts(files),
    }
}
