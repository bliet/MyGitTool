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
