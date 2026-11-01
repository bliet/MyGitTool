//! Resolving conflicts in the app.

use crate::diff::is_binary;
use crate::file_diff::MAX_DIFF_FILE_SIZE;
use crate::merge::merge3;
use crate::ports::{ReadError, RepositoryReader, RepositoryWriter};
use crate::sync::OperationError;
use domain::{
    ConflictContent, ConflictSide, ConflictSides, FileConflict, FileContent, RepositoryInfo,
    SideKind,
};
use std::sync::Arc;

/// Loads a conflicted file for the resolver.
#[derive(Clone)]
pub struct LoadConflict {
    reader: Arc<dyn RepositoryReader>,
    size_limit: u64,
}

impl LoadConflict {
    pub fn new(reader: Arc<dyn RepositoryReader>) -> Self {
        Self {
            reader,
            size_limit: MAX_DIFF_FILE_SIZE,
        }
    }

    /// Replaces [`MAX_DIFF_FILE_SIZE`] as the largest version that is
    /// merged line by line.
    pub fn with_size_limit(mut self, bytes: u64) -> Self {
        self.size_limit = bytes;
        self
    }

    /// Merges the versions of `path` line by line. A file deleted on one
    /// side, binary or too large is offered as a whole-file choice.
    pub fn load(&self, repo: &RepositoryInfo, path: &str) -> Result<FileConflict, OperationError> {
        let sides = self
            .reader
            .conflict_sides(repo, path, self.size_limit)?
            .ok_or_else(|| OperationError::NotConflicted(path.to_owned()))?;
        Ok(FileConflict {
            path: path.to_owned(),
            content: content(sides),
        })
    }
}

fn content(sides: ConflictSides) -> ConflictContent {
    let text = |content: &Option<FileContent>| match content {
        Some(FileContent::Bytes(bytes)) if !is_binary(bytes) => {
            String::from_utf8(bytes.clone()).ok()
        }
        _ => None,
    };
    // A file added on both sides merges against an empty base.
    let base = match &sides.base {
        None => Some(String::new()),
        base => text(base),
    };
    match (base, text(&sides.ours), text(&sides.theirs)) {
        (Some(base), Some(ours), Some(theirs)) => {
            ConflictContent::Text(merge3(&base, &ours, &theirs))
        }
        _ => ConflictContent::WholeFile {
            ours: kind(&sides.ours),
            theirs: kind(&sides.theirs),
        },
    }
}

fn kind(content: &Option<FileContent>) -> SideKind {
    match content {
        None => SideKind::Deleted,
        Some(FileContent::TooLarge { .. }) => SideKind::TooLarge,
        Some(FileContent::Bytes(bytes)) if is_binary(bytes) => SideKind::Binary,
        Some(FileContent::Bytes(bytes)) if std::str::from_utf8(bytes).is_err() => SideKind::Binary,
        Some(FileContent::Bytes(_)) => SideKind::Text,
    }
}

/// Resolves conflicted files and stages the result.
#[derive(Clone)]
pub struct ResolveConflict {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
}

impl ResolveConflict {
    pub fn new(reader: Arc<dyn RepositoryReader>, writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { reader, writer }
    }

    /// Replaces the file with the merged `text`.
    pub fn resolve_text(
        &self,
        repo: &RepositoryInfo,
        path: &str,
        text: &str,
    ) -> Result<(), OperationError> {
        self.check(repo, path)?;
        Ok(self
            .writer
            .resolve_conflict(repo, path, Some(text.as_bytes()))?)
    }

    /// Takes one side whole; a side that deleted the file deletes it.
    pub fn take_side(
        &self,
        repo: &RepositoryInfo,
        path: &str,
        side: ConflictSide,
    ) -> Result<(), OperationError> {
        let sides = self
            .reader
            .conflict_sides(repo, path, u64::MAX)?
            .ok_or_else(|| OperationError::NotConflicted(path.to_owned()))?;
        let content = match side {
            ConflictSide::Ours => sides.ours,
            ConflictSide::Theirs => sides.theirs,
        };
        let bytes = match content {
            None => None,
            Some(FileContent::Bytes(bytes)) => Some(bytes),
            // The limit above loads every size, so this only guards a
            // reader that ignores it.
            Some(FileContent::TooLarge { size }) => {
                return Err(ReadError::Backend(format!(
                    "{path} is too large to load ({size} bytes)"
                ))
                .into());
            }
        };
        Ok(self.writer.resolve_conflict(repo, path, bytes.as_deref())?)
    }

    /// Marks the file resolved as it is in the work tree, e.g. after it
    /// was edited with another tool.
    pub fn keep_work_tree(&self, repo: &RepositoryInfo, path: &str) -> Result<(), OperationError> {
        self.check(repo, path)?;
        Ok(self.writer.stage(repo, &[path.to_owned()])?)
    }

    fn check(&self, repo: &RepositoryInfo, path: &str) -> Result<(), OperationError> {
        if self
            .reader
            .conflicted_files(repo)?
            .iter()
            .any(|p| p == path)
        {
            Ok(())
        } else {
            Err(OperationError::NotConflicted(path.to_owned()))
        }
    }
}
