//! Staging and unstaging changes, and committing what is staged.

use crate::ports::{RepositoryReader, RepositoryWriter};
use crate::sync::OperationError;
use domain::{CommitId, FileChange, RepositoryInfo};
use std::sync::Arc;

/// Moves changes between the work tree and the index.
#[derive(Clone)]
pub struct StageChanges {
    writer: Arc<dyn RepositoryWriter>,
}

impl StageChanges {
    pub fn new(writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { writer }
    }

    /// Stages the unstaged `changes`.
    pub fn stage(
        &self,
        repo: &RepositoryInfo,
        changes: &[FileChange],
    ) -> Result<(), OperationError> {
        let paths = paths(changes);
        if paths.is_empty() {
            return Ok(());
        }
        Ok(self.writer.stage(repo, &paths)?)
    }

    /// Unstages the staged `changes`, keeping them in the work tree. Both
    /// paths of a staged rename are reset.
    pub fn unstage(
        &self,
        repo: &RepositoryInfo,
        changes: &[FileChange],
    ) -> Result<(), OperationError> {
        let paths = paths(changes);
        if paths.is_empty() {
            return Ok(());
        }
        Ok(self.writer.unstage(repo, &paths)?)
    }
}

/// Every path the changes touch, sorted and without duplicates.
fn paths(changes: &[FileChange]) -> Vec<String> {
    let mut paths: Vec<String> = changes
        .iter()
        .flat_map(FileChange::paths)
        .map(str::to_owned)
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

/// Commits the staged changes.
#[derive(Clone)]
pub struct CreateCommit {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
}

impl CreateCommit {
    pub fn new(reader: Arc<dyn RepositoryReader>, writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { reader, writer }
    }

    /// Commits the index with `message` and returns the new commit. The
    /// message is cleaned up as `git commit -m` does. A merge or rebase in
    /// progress is finished with [`crate::ContinueOperation`] instead.
    pub fn commit(&self, repo: &RepositoryInfo, message: &str) -> Result<CommitId, OperationError> {
        let message = clean_message(message).ok_or(OperationError::EmptyCommitMessage)?;
        if let Some(operation) = self.reader.ongoing_operation(repo)? {
            return Err(OperationError::OperationInProgress(operation));
        }
        let conflicts = self.reader.conflicted_files(repo)?;
        if !conflicts.is_empty() {
            return Err(OperationError::UnresolvedConflicts(conflicts));
        }
        if self.reader.work_tree_changes(repo)?.staged.is_empty() {
            return Err(OperationError::NothingStaged);
        }
        Ok(self.writer.commit(repo, &message)?)
    }
}

/// Drops trailing whitespace, leading and trailing blank lines, and runs
/// of blank lines, and ends the message with a newline. `None` if nothing
/// is left.
fn clean_message(message: &str) -> Option<String> {
    let mut cleaned = String::new();
    let mut blank_run = false;
    for line in message.lines().map(str::trim_end) {
        if line.is_empty() {
            blank_run = !cleaned.is_empty();
            continue;
        }
        if blank_run {
            cleaned.push('\n');
            blank_run = false;
        }
        cleaned.push_str(line);
        cleaned.push('\n');
    }
    (!cleaned.is_empty()).then_some(cleaned)
}
