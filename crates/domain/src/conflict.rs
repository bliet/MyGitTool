use crate::diff::FileContent;

/// The three versions of a file with an unresolved conflict, as the index
/// holds them. `None` means the file does not exist in that version, e.g.
/// it was deleted on one side or added on both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictSides {
    /// The common ancestor.
    pub base: Option<FileContent>,
    /// The version being merged into: HEAD in a merge, the upstream in a
    /// rebase.
    pub ours: Option<FileContent>,
    /// The version being merged: the other branch in a merge, the commit
    /// being replayed in a rebase.
    pub theirs: Option<FileContent>,
}

/// One of the two sides of a conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConflictSide {
    Ours,
    Theirs,
}

/// A file with a conflict, ready to be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileConflict {
    pub path: String,
    pub content: ConflictContent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictContent {
    /// Both sides are text: the file merged line by line, with the parts
    /// the sides disagree on as conflicts.
    Text(Vec<MergeChunk>),
    /// The file cannot be merged line by line; one side is taken whole.
    WholeFile { ours: SideKind, theirs: SideKind },
}

/// What a side of a whole-file conflict holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SideKind {
    Deleted,
    Text,
    Binary,
    TooLarge,
}

/// A part of a merged text file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeChunk {
    /// Lines that need no decision: unchanged, changed on one side only,
    /// or changed the same way on both.
    Clean(String),
    Conflict(ConflictHunk),
}

/// Lines the two sides changed differently. Each text holds whole lines
/// with their line endings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictHunk {
    pub base: String,
    pub ours: String,
    pub theirs: String,
}

/// How a conflict hunk is resolved.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Resolution {
    Ours,
    Theirs,
    OursThenTheirs,
    TheirsThenOurs,
    /// Text the user wrote.
    Custom(String),
}

impl Resolution {
    /// The text that replaces `hunk`.
    pub fn apply(&self, hunk: &ConflictHunk) -> String {
        match self {
            Self::Ours => hunk.ours.clone(),
            Self::Theirs => hunk.theirs.clone(),
            Self::OursThenTheirs => join_lines(&hunk.ours, &hunk.theirs),
            Self::TheirsThenOurs => join_lines(&hunk.theirs, &hunk.ours),
            Self::Custom(text) => text.clone(),
        }
    }
}

/// `first` followed by `second`, with a line break between them if
/// `first` ends without one (at the end of a file).
fn join_lines(first: &str, second: &str) -> String {
    let mut text = first.to_owned();
    if !text.is_empty() && !text.ends_with('\n') && !second.is_empty() {
        text.push('\n');
    }
    text.push_str(second);
    text
}

/// The merged file: the clean chunks with each conflict replaced by its
/// resolution. `resolutions` has one entry per conflict, in order; `None`
/// while any conflict is undecided.
pub fn merged_text(chunks: &[MergeChunk], resolutions: &[Option<Resolution>]) -> Option<String> {
    let mut text = String::new();
    let mut resolutions = resolutions.iter();
    for chunk in chunks {
        match chunk {
            MergeChunk::Clean(clean) => text.push_str(clean),
            MergeChunk::Conflict(hunk) => {
                let resolution = resolutions.next()?.as_ref()?;
                text.push_str(&resolution.apply(hunk));
            }
        }
    }
    Some(text)
}
