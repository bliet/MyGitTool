#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    /// The file type changed, e.g. from a regular file to a symlink.
    TypeChanged,
}

impl ChangeKind {
    /// The one-letter status as `git status --short` shows it.
    pub fn letter(self) -> char {
        match self {
            Self::Added => 'A',
            Self::Modified => 'M',
            Self::Deleted => 'D',
            Self::Renamed => 'R',
            Self::TypeChanged => 'T',
        }
    }
}

/// A changed file. Paths are relative to the repository root and use `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    /// The previous path of a renamed file.
    pub old_path: Option<String>,
    pub kind: ChangeKind,
    pub additions: usize,
    pub deletions: usize,
    pub binary: bool,
}

impl FileChange {
    /// The paths the change touches: the path, and the previous path of a
    /// renamed file.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.path.as_str()).chain(self.old_path.as_deref())
    }
}

/// The uncommitted changes, split the way `git status` shows them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkTreeChanges {
    /// HEAD against the index: what the next commit contains.
    pub staged: Vec<FileChange>,
    /// The index against the work tree, including untracked files and
    /// files with unresolved conflicts.
    pub unstaged: Vec<FileChange>,
}

impl WorkTreeChanges {
    pub fn is_empty(&self) -> bool {
        self.staged.is_empty() && self.unstaged.is_empty()
    }

    /// The number of changed files. A file with both staged and unstaged
    /// changes counts once.
    pub fn file_count(&self) -> usize {
        let mut paths: Vec<&str> = self
            .staged
            .iter()
            .chain(&self.unstaged)
            .map(|change| change.path.as_str())
            .collect();
        paths.sort_unstable();
        paths.dedup();
        paths.len()
    }
}
