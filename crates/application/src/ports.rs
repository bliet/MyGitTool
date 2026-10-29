use domain::{
    Branch, Commit, CommitDetails, CommitId, FileChange, Head, Remote, RepositoryInfo, Tag,
};
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

/// Which commits a history walk covers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HistoryScope {
    /// HEAD and every branch and tag.
    All,
    /// Only the commits reachable from this ref, e.g. `refs/heads/main`.
    Ref(String),
}

/// Reads repository data. Every call opens the repository at `repo`, so one
/// reader serves any number of repositories.
pub trait RepositoryReader: Send + Sync {
    fn head(&self, repo: &RepositoryInfo) -> Result<Head, ReadError>;
    /// Local and remote branches. Symbolic refs like `origin/HEAD` are left out.
    fn branches(&self, repo: &RepositoryInfo) -> Result<Vec<Branch>, ReadError>;
    fn remotes(&self, repo: &RepositoryInfo) -> Result<Vec<Remote>, ReadError>;
    fn tags(&self, repo: &RepositoryInfo) -> Result<Vec<Tag>, ReadError>;
    /// Starts a walk over the commits in `scope`, in topological order
    /// (children before parents) with newer commits first where possible.
    fn history(
        &self,
        repo: &RepositoryInfo,
        scope: &HistoryScope,
    ) -> Result<Box<dyn HistoryCursor>, ReadError>;
    fn commit_details(
        &self,
        repo: &RepositoryInfo,
        id: CommitId,
    ) -> Result<CommitDetails, ReadError>;
    /// Staged, unstaged and untracked changes against HEAD. Empty if the
    /// work tree is clean or the repository is bare.
    fn work_tree_changes(&self, repo: &RepositoryInfo) -> Result<Vec<FileChange>, ReadError>;
}

/// A history walk that hands out commits page by page.
pub trait HistoryCursor: Send {
    /// Returns up to `max` further commits. Fewer than `max` means the walk is done.
    fn next_page(&mut self, max: usize) -> Result<Vec<Commit>, ReadError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReadError {
    #[error("commit {0} not found")]
    CommitNotFound(CommitId),
    #[error("ref {0} not found")]
    RefNotFound(String),
    #[error("{0}")]
    Backend(String),
}
