//! Use cases and port traits of MyGitTool.
//!
//! The ports are implemented by the infrastructure crates and injected into
//! the use cases by the composition root (`app`).

mod commit_details;
mod graph;
mod history;
mod open_repository;
mod ports;
mod recent_repositories;
mod refs;
mod work_tree;

pub use commit_details::GetCommitDetails;
pub use graph::{EdgeHalf, GraphEdge, GraphLayout, GraphRow};
pub use history::{HistoryPager, HistoryRow, LoadHistory};
pub use open_repository::{OpenRepository, Startup};
pub use ports::{
    HistoryCursor, HistoryScope, LocateError, ReadError, RecentRepositoriesStore,
    RepositoryLocator, RepositoryReader, StorageError,
};
pub use recent_repositories::{DEFAULT_CAPACITY, RecentRepositories};
pub use refs::{LoadBranches, RefIndex, RefLabel, RefLabelKind, RemoteBranches, RepositoryRefs};
pub use work_tree::LoadWorkTreeChanges;
