use domain::{
    Branch, Commit, CommitDetails, CommitId, Credential, CredentialRequest, FileChange,
    FileContent, Head, OngoingOperation, Progress, PullConfig, Remote, RepositoryInfo, Revision,
    Tag, Tracking,
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

/// User preferences that persist across sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Settings {
    /// Draw lines that show the indentation levels of tree views.
    pub indent_guides: bool,
}

/// Persists the user's settings.
pub trait SettingsStore: Send + Sync {
    /// Returns the stored settings, or `None` if nothing was stored yet.
    fn load(&self) -> Result<Option<Settings>, StorageError>;
    fn save(&self, settings: &Settings) -> Result<(), StorageError>;
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
    /// The content of the file at `path` in `revision`, or `None` if there is
    /// no file at that path. A file larger than `limit` bytes is not loaded.
    fn file_content(
        &self,
        repo: &RepositoryInfo,
        revision: Revision,
        path: &str,
        limit: u64,
    ) -> Result<Option<FileContent>, ReadError>;
    /// The upstream config of the local branch `branch` (short name), or
    /// `None` if it has none.
    fn tracking(&self, repo: &RepositoryInfo, branch: &str) -> Result<Option<Tracking>, ReadError>;
    /// The commit a ref points to, or `None` if the ref does not exist.
    fn resolve(&self, repo: &RepositoryInfo, refname: &str) -> Result<Option<CommitId>, ReadError>;
    /// The number of commits reachable from `local` but not `upstream`, and
    /// the other way round.
    fn ahead_behind(
        &self,
        repo: &RepositoryInfo,
        local: CommitId,
        upstream: CommitId,
    ) -> Result<(usize, usize), ReadError>;
    /// Tracked files with staged or unstaged changes, including conflicted
    /// ones, sorted. Untracked files are left out.
    fn dirty_files(&self, repo: &RepositoryInfo) -> Result<Vec<String>, ReadError>;
    /// The merge, rebase or similar the repository is in the middle of.
    fn ongoing_operation(
        &self,
        repo: &RepositoryInfo,
    ) -> Result<Option<OngoingOperation>, ReadError>;
    /// How `pull` integrates for the local branch `branch`.
    fn pull_config(&self, repo: &RepositoryInfo, branch: &str) -> Result<PullConfig, ReadError>;
    /// The paths whose content differs between the trees of two commits,
    /// including both paths of a rename, sorted.
    fn changed_paths(
        &self,
        repo: &RepositoryInfo,
        from: CommitId,
        to: CommitId,
    ) -> Result<Vec<String>, ReadError>;
    /// The files with unresolved conflicts in the index, sorted.
    fn conflicted_files(&self, repo: &RepositoryInfo) -> Result<Vec<String>, ReadError>;
}

/// Changes the local repository. Callers check for a dirty work tree first.
pub trait RepositoryWriter: Send + Sync {
    /// Moves the checked-out `branch` to `target`, a descendant of its tip,
    /// and updates the index and work tree.
    fn fast_forward(
        &self,
        repo: &RepositoryInfo,
        branch: &str,
        target: CommitId,
    ) -> Result<(), WriteError>;
    /// Merges `commit` into HEAD with a merge commit, even if a
    /// fast-forward were possible. Conflicts are left in the index and work
    /// tree, with the merge in progress.
    fn merge(
        &self,
        repo: &RepositoryInfo,
        commit: CommitId,
        message: &str,
    ) -> Result<Integration, WriteError>;
    /// Replays the commits of HEAD that `upstream` lacks on top of it. Stops
    /// at the first conflict and leaves the rebase in progress.
    fn rebase(&self, repo: &RepositoryInfo, upstream: CommitId) -> Result<Integration, WriteError>;
    /// Aborts the merge or rebase in progress and restores the state before it.
    fn abort(&self, repo: &RepositoryInfo) -> Result<(), WriteError>;
    /// Switches to the local `branch`, updating the index and work tree.
    /// Local changes to files the switch does not touch are kept; the switch
    /// fails rather than overwrite any other change or untracked file.
    fn checkout(&self, repo: &RepositoryInfo, branch: &str) -> Result<(), WriteError>;
    /// Creates the local branch `name` at `target`. Fails if it exists.
    fn create_branch(
        &self,
        repo: &RepositoryInfo,
        name: &str,
        target: CommitId,
    ) -> Result<(), WriteError>;
    /// Deletes the local branch `name` and its config section.
    fn delete_branch(&self, repo: &RepositoryInfo, name: &str) -> Result<(), WriteError>;
    /// Continues the merge or rebase in progress once its conflicts are
    /// resolved and staged. A rebase may stop again at a later commit.
    fn continue_operation(&self, repo: &RepositoryInfo) -> Result<Integration, WriteError>;
    /// Sets `branch.<branch>.remote` and `branch.<branch>.merge`.
    fn set_tracking(
        &self,
        repo: &RepositoryInfo,
        branch: &str,
        remote: &str,
        merge_ref: &str,
    ) -> Result<(), WriteError>;
}

/// The result of merging or rebasing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Integration {
    Completed,
    /// Stopped with conflicts in these files, sorted.
    Conflicts(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WriteError {
    #[error("{0}")]
    Backend(String),
}

/// Talks to remotes. Long operations report through `progress`, which can
/// also cancel them, and ask `credentials` when the chain of automatic
/// credentials (SSH agent, default SSH keys, Git credential helper) fails.
pub trait RemoteOperations: Send + Sync {
    /// Fetches `remote` with its configured refspecs, updating its
    /// remote-tracking branches.
    fn fetch(
        &self,
        repo: &RepositoryInfo,
        remote: &str,
        progress: &dyn ProgressReporter,
        credentials: &dyn CredentialProvider,
    ) -> Result<(), RemoteError>;
    /// Pushes the local `branch` to `remote_ref` on `remote`. Never forces:
    /// an update that is not a fast-forward is rejected.
    fn push(
        &self,
        repo: &RepositoryInfo,
        branch: &str,
        remote: &str,
        remote_ref: &str,
        progress: &dyn ProgressReporter,
        credentials: &dyn CredentialProvider,
    ) -> Result<(), RemoteError>;
    /// Deletes `remote_ref` on `remote` and its remote-tracking branch.
    fn delete_branch(
        &self,
        repo: &RepositoryInfo,
        remote: &str,
        remote_ref: &str,
        progress: &dyn ProgressReporter,
        credentials: &dyn CredentialProvider,
    ) -> Result<(), RemoteError>;
}

/// Receives the progress of a remote operation.
pub trait ProgressReporter: Sync {
    fn report(&self, progress: Progress);
    /// Polled during the transfer; `true` stops the operation.
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// Asks the user for credentials, blocking until they answer. `None` means
/// the user cancelled. Answers are used once and never stored.
pub trait CredentialProvider: Sync {
    fn credentials(&self, request: &CredentialRequest) -> Option<Credential>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RemoteError {
    #[error("there is no remote named {0}")]
    UnknownRemote(String),
    #[error("authentication failed for {0}")]
    AuthenticationFailed(String),
    #[error("cancelled")]
    Cancelled,
    /// The remote refused to update `reference`, e.g. because it is not a
    /// fast-forward.
    #[error("the remote rejected {reference}: {message}")]
    Rejected { reference: String, message: String },
    #[error("{0}")]
    Backend(String),
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
