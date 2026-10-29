//! Pure domain types and rules of MyGitTool. This crate performs no I/O.

mod branch;
mod change;
mod commit;
mod repository;

pub use branch::{Branch, BranchKind, Head, Remote, Tag, Upstream};
pub use change::{ChangeKind, FileChange};
pub use commit::{Commit, CommitDetails, CommitId, InvalidCommitId, Signature, Timestamp};
pub use repository::RepositoryInfo;
