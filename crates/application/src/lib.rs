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
mod open_repository;
mod ports;
mod recent_repositories;
mod refs;
mod settings;
mod sync;
mod work_tree;

pub use branches::{
    Checkout, CheckoutTarget, ContinueOperation, CreateBranch, DeleteBranch, LoadOngoing,
    MergeIntoCurrent, OngoingState, RebaseCurrentOnto, merge_message,
};
pub use commit_details::GetCommitDetails;
pub use diff::{is_binary, side_by_side};
pub use file_diff::{ComputeFileDiff, MAX_DIFF_FILE_SIZE};
pub use graph::{EdgeHalf, GraphEdge, GraphLayout, GraphRow};
pub use history::{HistoryPager, HistoryRow, LoadHistory};
pub use open_repository::{OpenRepository, Startup};
pub use ports::{
    CredentialProvider, HistoryCursor, HistoryScope, Integration, LocateError, ProgressReporter,
    ReadError, RecentRepositoriesStore, RemoteError, RemoteOperations, RepositoryLocator,
    RepositoryReader, RepositoryWriter, Settings, SettingsStore, StorageError, WriteError,
};
pub use recent_repositories::{DEFAULT_CAPACITY, RecentRepositories};
pub use refs::{LoadBranches, RefIndex, RefLabel, RefLabelKind, RemoteBranches, RepositoryRefs};
pub use settings::UserSettings;
pub use sync::{
    AbortOperation, Fetch, FetchTarget, IntegrationOutcome, OperationError, Pull, Push,
    PushDestination, PushPlan,
};
pub use work_tree::LoadWorkTreeChanges;
