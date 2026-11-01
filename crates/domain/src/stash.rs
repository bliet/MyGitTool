use crate::commit::CommitId;

/// A stash entry. Entries are numbered from the newest, so a stash's index
/// changes as others are saved or dropped; its commit id does not.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Stash {
    /// The position in the stash list, `0` for the newest.
    pub index: usize,
    /// The message, e.g. `WIP on main: 1a2b3c4 Fix the parser`.
    pub message: String,
    /// The stash commit. Its first parent is the commit the stash was made on.
    pub id: CommitId,
}

impl Stash {
    /// The name Git uses for the entry, e.g. `stash@{0}`.
    pub fn name(&self) -> String {
        format!("stash@{{{}}}", self.index)
    }
}
