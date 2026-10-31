//! Pure domain types and rules of MyGitTool. This crate performs no I/O.

mod branch;
mod branch_name;
mod change;
mod commit;
mod diff;
mod hosting;
mod remote;
mod repository;
mod state;

pub use branch::{Branch, BranchKind, Head, Remote, Tag, Upstream};
pub use branch_name::{InvalidBranchName, validate_branch_name};
pub use change::{ChangeKind, FileChange};
pub use commit::{Commit, CommitDetails, CommitId, InvalidCommitId, Signature, Timestamp};
pub use diff::{
    DiffBody, DiffHunk, DiffLine, DiffRow, DiffSource, FileContent, FileDiff, Revision, RowKind,
    SideBySide,
};
pub use hosting::{Hoster, WebLink, web_link};
pub use remote::{Credential, CredentialRequest, FastForward, Progress, PullConfig, Tracking};
pub use repository::RepositoryInfo;
pub use state::{DirtyWorkTree, OngoingOperation};
