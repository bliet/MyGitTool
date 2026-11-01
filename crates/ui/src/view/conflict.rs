//! The state and texts of the conflict resolver window.

use crate::view::diff::Direction;
use domain::{ConflictSide, MergeChunk, OngoingOperation, Resolution, SideKind, merged_text};

/// The decisions taken so far for the conflicts of one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolver {
    chunks: Vec<MergeChunk>,
    /// One entry per conflict, in order.
    resolutions: Vec<Option<Resolution>>,
}

impl Resolver {
    pub fn new(chunks: Vec<MergeChunk>) -> Self {
        let conflicts = chunks
            .iter()
            .filter(|chunk| matches!(chunk, MergeChunk::Conflict(_)))
            .count();
        Self {
            chunks,
            resolutions: vec![None; conflicts],
        }
    }

    pub fn chunks(&self) -> &[MergeChunk] {
        &self.chunks
    }

    pub fn conflict_count(&self) -> usize {
        self.resolutions.len()
    }

    pub fn resolved_count(&self) -> usize {
        self.resolutions.iter().flatten().count()
    }

    /// The decision for the conflict with the number `conflict`.
    pub fn resolution(&self, conflict: usize) -> Option<&Resolution> {
        self.resolutions.get(conflict)?.as_ref()
    }

    /// Decides the conflict `conflict`, or undoes the decision with `None`.
    pub fn resolve(&mut self, conflict: usize, resolution: Option<Resolution>) {
        if let Some(slot) = self.resolutions.get_mut(conflict) {
            *slot = resolution;
        }
    }

    /// The merged file, once every conflict is decided.
    pub fn merged(&self) -> Option<String> {
        merged_text(&self.chunks, &self.resolutions)
    }

    /// The conflict to jump to from `current`, wrapping around. Without a
    /// current conflict, it starts at the first undecided one.
    pub fn step(&self, current: Option<usize>, direction: Direction) -> Option<usize> {
        let count = self.conflict_count();
        if count == 0 {
            return None;
        }
        Some(match (current, direction) {
            (None, _) => self
                .resolutions
                .iter()
                .position(Option::is_none)
                .unwrap_or(0),
            (Some(i), Direction::Next) => (i + 1) % count,
            (Some(i), Direction::Previous) => (i + count - 1) % count,
        })
    }
}

/// How a decision reads in the resolved conflict's header.
pub fn resolution_label(resolution: &Resolution) -> &'static str {
    match resolution {
        Resolution::Ours => "ours",
        Resolution::Theirs => "theirs",
        Resolution::OursThenTheirs => "ours, then theirs",
        Resolution::TheirsThenOurs => "theirs, then ours",
        Resolution::Custom(_) => "edited text",
    }
}

/// What the two sides are in the operation that conflicted.
pub fn side_titles(operation: Option<OngoingOperation>) -> (&'static str, &'static str) {
    match operation {
        Some(OngoingOperation::Merge) => ("Ours: current branch", "Theirs: merged branch"),
        Some(OngoingOperation::Rebase) => ("Ours: rebasing onto", "Theirs: your commit"),
        _ => ("Ours", "Theirs"),
    }
}

/// "2 of 3 conflicts resolved".
pub fn progress_text(resolved: usize, total: usize) -> String {
    match total {
        1 => format!("{resolved} of 1 conflict resolved"),
        n => format!("{resolved} of {n} conflicts resolved"),
    }
}

/// The resolver window's title, e.g. `main.rs (conflict) — MyGitTool`.
pub fn window_title(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    format!("{name} (conflict) — MyGitTool")
}

/// A clean chunk shortened to its first and last lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub head: String,
    /// The number of lines left out between `head` and `tail`.
    pub hidden: usize,
    pub tail: String,
}

/// Shortens `text` to `context` lines at each end, unless that would hide
/// only a line or two.
pub fn clean_preview(text: &str, context: usize) -> Preview {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    if lines.len() <= 2 * context + 2 {
        return Preview {
            head: text.to_owned(),
            hidden: 0,
            tail: String::new(),
        };
    }
    Preview {
        head: lines[..context].concat(),
        hidden: lines.len() - 2 * context,
        tail: lines[lines.len() - context..].concat(),
    }
}

/// Text for a `<pre>`: without the final line break, which would show as
/// an empty line.
pub fn shown(text: &str) -> &str {
    text.strip_suffix('\n')
        .map(|t| t.strip_suffix('\r').unwrap_or(t))
        .unwrap_or(text)
}

/// Explains a conflict that is resolved by taking one side whole.
pub fn whole_file_text(ours: SideKind, theirs: SideKind) -> String {
    let describe = |kind: SideKind| match kind {
        SideKind::Deleted => "deleted the file",
        SideKind::Text => "changed the file",
        SideKind::Binary => "changed the binary file",
        SideKind::TooLarge => "changed the file, which is too large to merge here",
    };
    format!(
        "Ours {} and theirs {}. Choose the version to keep.",
        describe(ours),
        describe(theirs)
    )
}

/// The button that takes `side` whole.
pub fn take_label(side: ConflictSide, kind: SideKind) -> &'static str {
    match (side, kind) {
        (ConflictSide::Ours, SideKind::Deleted) => "Delete the file (ours)",
        (ConflictSide::Theirs, SideKind::Deleted) => "Delete the file (theirs)",
        (ConflictSide::Ours, _) => "Use ours",
        (ConflictSide::Theirs, _) => "Use theirs",
    }
}
