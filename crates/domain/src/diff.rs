use crate::change::ChangeKind;
use crate::commit::CommitId;
use std::ops::Range;

/// What a file diff compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffSource {
    /// A commit against its first parent. `parent` is `None` for a root commit.
    Commit {
        id: CommitId,
        parent: Option<CommitId>,
    },
    /// HEAD against the work tree.
    WorkTree,
}

/// A state of the repository that file content is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Revision {
    Commit(CommitId),
    Head,
    WorkTree,
}

/// The content of a file at a revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileContent {
    Bytes(Vec<u8>),
    /// The file is larger than the requested limit and was not loaded.
    TooLarge {
        size: u64,
    },
}

/// The diff of one file, ready for a side-by-side view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    /// The path on the old side; `None` if the file was added.
    pub old_path: Option<String>,
    /// The path on the new side; `None` if the file was deleted.
    pub new_path: Option<String>,
    pub kind: ChangeKind,
    pub body: DiffBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffBody {
    Text(SideBySide),
    Binary,
    /// At least one side exceeds the size limit; `size` is the larger one.
    TooLarge {
        size: u64,
    },
}

/// Aligned rows of a text diff. Every row has the same height on both sides,
/// so the two sides scroll together row by row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SideBySide {
    pub rows: Vec<DiffRow>,
    /// The changes in order. Empty if both sides are equal.
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRow {
    pub kind: RowKind,
    /// The old line; `None` for an added row.
    pub old: Option<DiffLine>,
    /// The new line; `None` for a removed row.
    pub new: Option<DiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RowKind {
    /// The line is the same on both sides.
    Context,
    Removed,
    Added,
    /// The line was edited: both sides are set and differ.
    Changed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// The 1-based line number.
    pub number: usize,
    /// The line without its line ending.
    pub text: String,
    /// Byte ranges of `text` that changed within a `Changed` row, in order
    /// and without overlaps. Empty for every other row kind.
    pub highlights: Vec<Range<usize>>,
}

/// A block of consecutive changed rows: the unit that "next change" and
/// "previous change" jump between.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DiffHunk {
    pub rows: Range<usize>,
}
