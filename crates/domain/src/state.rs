use std::fmt;
use thiserror::Error;

/// A multi-step operation the repository is in the middle of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OngoingOperation {
    Merge,
    Rebase,
    CherryPick,
    Revert,
    Bisect,
    /// Something else, e.g. applying a mailbox.
    Other,
}

impl fmt::Display for OngoingOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
            Self::CherryPick => "cherry-pick",
            Self::Revert => "revert",
            Self::Bisect => "bisect",
            Self::Other => "operation",
        })
    }
}

/// An operation that changes the work tree was refused because tracked
/// files have uncommitted changes. Changes are never discarded silently.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("the work tree has uncommitted changes in {}", .files.join(", "))]
pub struct DirtyWorkTree {
    /// Paths relative to the repository root, sorted.
    pub files: Vec<String>,
}
