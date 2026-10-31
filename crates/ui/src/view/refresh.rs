//! What an automatic refresh reloads.

use application::RepoChanges;

/// The parts of the repository view to reload after outside changes. The
/// history is only reloaded if the refs turn out to have changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reload {
    pub refs: bool,
    pub work_tree: bool,
    pub ongoing: bool,
}

impl Reload {
    pub fn after(changes: RepoChanges) -> Self {
        // The index lives in the Git directory, so its changes show in the
        // work tree status too.
        Self {
            refs: changes.git_dir,
            work_tree: changes.git_dir || changes.work_tree,
            ongoing: changes.git_dir,
        }
    }
}
