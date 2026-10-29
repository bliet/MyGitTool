use domain::RepositoryInfo;
use std::error::Error;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Finds the Git repository a path belongs to.
pub trait RepositoryLocator: Send + Sync {
    /// Returns the repository that contains `path`, which is either the
    /// repository root itself or any directory inside it.
    fn locate(&self, path: &Path) -> Result<RepositoryInfo, LocateError>;
}

#[derive(Debug, Error)]
pub enum LocateError {
    #[error("{} does not exist", .path.display())]
    NotFound { path: PathBuf },
    #[error("{} is not inside a Git repository", .path.display())]
    NotARepository { path: PathBuf },
    #[error("cannot open the repository at {}: {message}", .path.display())]
    Unreadable { path: PathBuf, message: String },
}

/// Persists the list of recently opened repositories, most recent first.
pub trait RecentRepositoriesStore: Send + Sync {
    /// Returns the stored list, or an empty list if nothing was stored yet.
    fn load(&self) -> Result<Vec<PathBuf>, StorageError>;
    fn save(&self, paths: &[PathBuf]) -> Result<(), StorageError>;
}

#[derive(Debug, Error)]
#[error("{context}: {source}")]
pub struct StorageError {
    context: String,
    #[source]
    source: Box<dyn Error + Send + Sync>,
}

impl StorageError {
    pub fn new(
        context: impl Into<String>,
        source: impl Into<Box<dyn Error + Send + Sync>>,
    ) -> Self {
        Self {
            context: context.into(),
            source: source.into(),
        }
    }
}
