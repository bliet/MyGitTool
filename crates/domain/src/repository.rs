use std::path::{Path, PathBuf};

/// An opened Git repository, identified by its root directory.
///
/// For a repository with a work tree the root is the work tree directory,
/// for a bare repository it is the Git directory itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepositoryInfo {
    root: PathBuf,
}

impl RepositoryInfo {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// A short display name: the last component of the root path, or the
    /// whole path when it has none (e.g. a filesystem root).
    pub fn name(&self) -> String {
        match self.root.file_name() {
            Some(name) => name.to_string_lossy().into_owned(),
            None => self.root.display().to_string(),
        }
    }
}
