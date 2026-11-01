use crate::diff::{is_binary, side_by_side};
use crate::ports::{ReadError, RepositoryReader};
use domain::{
    ChangeKind, DiffBody, DiffSource, FileChange, FileContent, FileDiff, RepositoryInfo, Revision,
};
use std::sync::Arc;

/// Files larger than this many bytes are not diffed.
pub const MAX_DIFF_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Computes the side-by-side diff of one changed file.
#[derive(Clone)]
pub struct ComputeFileDiff {
    reader: Arc<dyn RepositoryReader>,
    size_limit: u64,
}

impl ComputeFileDiff {
    pub fn new(reader: Arc<dyn RepositoryReader>) -> Self {
        Self {
            reader,
            size_limit: MAX_DIFF_FILE_SIZE,
        }
    }

    /// Replaces [`MAX_DIFF_FILE_SIZE`] as the largest file size that is diffed.
    pub fn with_size_limit(mut self, bytes: u64) -> Self {
        self.size_limit = bytes;
        self
    }

    /// Diffs `change` as listed for `source`: a commit against its first
    /// parent, HEAD against the index, or the index against the work tree.
    pub fn compute(
        &self,
        repo: &RepositoryInfo,
        source: DiffSource,
        change: &FileChange,
    ) -> Result<FileDiff, ReadError> {
        let old_path = (change.kind != ChangeKind::Added).then(|| {
            change
                .old_path
                .clone()
                .unwrap_or_else(|| change.path.clone())
        });
        let new_path = (change.kind != ChangeKind::Deleted).then(|| change.path.clone());
        let body = if change.binary {
            DiffBody::Binary
        } else {
            let (old_revision, new_revision) = match source {
                DiffSource::Commit { id, parent } => {
                    (parent.map(Revision::Commit), Revision::Commit(id))
                }
                DiffSource::Staged => (Some(Revision::Head), Revision::Index),
                DiffSource::Unstaged => (Some(Revision::Index), Revision::WorkTree),
            };
            let old = match (old_revision, &old_path) {
                (Some(revision), Some(path)) => self.read(repo, revision, path)?,
                _ => None,
            };
            let new = match &new_path {
                Some(path) => self.read(repo, new_revision, path)?,
                None => None,
            };
            body(old, new)
        };
        Ok(FileDiff {
            old_path,
            new_path,
            kind: change.kind,
            body,
        })
    }

    fn read(
        &self,
        repo: &RepositoryInfo,
        revision: Revision,
        path: &str,
    ) -> Result<Option<FileContent>, ReadError> {
        self.reader
            .file_content(repo, revision, path, self.size_limit)
    }
}

/// A missing side counts as an empty file.
fn body(old: Option<FileContent>, new: Option<FileContent>) -> DiffBody {
    let (old, new) = match (bytes(old), bytes(new)) {
        (Ok(old), Ok(new)) => (old, new),
        (Err(old), Err(new)) => return DiffBody::TooLarge { size: old.max(new) },
        (Err(size), Ok(_)) | (Ok(_), Err(size)) => return DiffBody::TooLarge { size },
    };
    if is_binary(&old) || is_binary(&new) {
        return DiffBody::Binary;
    }
    DiffBody::Text(side_by_side(
        &String::from_utf8_lossy(&old),
        &String::from_utf8_lossy(&new),
    ))
}

/// The loaded bytes, or the size of a file too large to load.
fn bytes(content: Option<FileContent>) -> Result<Vec<u8>, u64> {
    match content {
        None => Ok(Vec::new()),
        Some(FileContent::Bytes(bytes)) => Ok(bytes),
        Some(FileContent::TooLarge { size }) => Err(size),
    }
}
