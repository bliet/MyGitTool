//! Use cases and port traits of MyGitTool.
//!
//! The ports are implemented by the infrastructure crates and injected into
//! the use cases by the composition root (`app`).

mod branches;
mod commit_details;
mod diff;
mod file_diff;
mod graph;
mod history;
mod layout;
mod open_repository;
mod ports;
mod recent_repositories;
mod refs;
mod search;
mod settings;
mod staging;
mod stash;
mod sync;
mod tags;
mod watch;
mod work_tree;

pub use branches::{
    Checkout, CheckoutTarget, ContinueOperation, CreateBranch, DeleteBranch, LoadOngoing,
    MergeIntoCurrent, OngoingState, RebaseCurrentOnto, merge_message,
};
pub use commit_details::GetCommitDetails;
pub use diff::{is_binary, side_by_side};
pub use file_diff::{ComputeFileDiff, MAX_DIFF_FILE_SIZE};
pub use graph::{EdgeHalf, GraphEdge, GraphLayout, GraphRow};
pub use history::{HistoryPager, HistoryRow, LoadHistory, SEARCH_BATCH};
pub use layout::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, RememberLayout};
pub use open_repository::{OpenRepository, Startup};
pub use ports::{
    CredentialProvider, DateFormat, FileViewMode, HistoryCursor, HistoryScope, Integration,
    LayoutStore, LocateError, ProgressReporter, ReadError, RecentRepositoriesStore, RemoteError,
    RemoteOperations, RepoChanges, RepoWatcher, RepositoryLocator, RepositoryReader,
    RepositoryWriter, Settings, SettingsStore, StorageError, Theme, WatchError, WatchGuard,
    WindowLayout, WriteError,
};
pub use recent_repositories::{DEFAULT_CAPACITY, RecentRepositories};
pub use refs::{LoadBranches, RefIndex, RefLabel, RefLabelKind, RemoteBranches, RepositoryRefs};
pub use search::CommitQuery;
pub use settings::UserSettings;
pub use staging::{CreateCommit, StageChanges};
pub use stash::{ApplyStash, DropStash, SaveStash, StashOutcome};
pub use sync::{
    AbortOperation, Fetch, FetchTarget, IntegrationOutcome, OperationError, Pull, Push,
    PushDestination, PushPlan,
};
pub use tags::{CreateTag, DeleteTag, PushTag};
pub use watch::WatchRepository;
pub use work_tree::{ApplyPatch, CreatePatch, LoadWorkTreeChanges};
