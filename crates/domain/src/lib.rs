//! Pure domain types and rules of MyGitTool. This crate performs no I/O.

mod branch;
mod change;
mod commit;
mod diff;
mod remote;
mod repository;
mod state;

pub use branch::{Branch, BranchKind, Head, Remote, Tag, Upstream};
pub use change::{ChangeKind, FileChange};
pub use commit::{Commit, CommitDetails, CommitId, InvalidCommitId, Signature, Timestamp};
pub use diff::{
    DiffBody, DiffHunk, DiffLine, DiffRow, DiffSource, FileContent, FileDiff, Revision, RowKind,
    SideBySide,
};
pub use remote::{Credential, CredentialRequest, FastForward, Progress, PullConfig, Tracking};
pub use repository::RepositoryInfo;
pub use state::{DirtyWorkTree, OngoingOperation};
