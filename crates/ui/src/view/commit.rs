//! Texts and rules of the staging lists and the commit box.

use crate::view::operation::can_continue;
use domain::{CommitId, FileChange, OngoingOperation};

/// Which way the buttons of a changed-files list move files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Staging {
    /// The list shows unstaged changes; its buttons stage them.
    Stage,
    /// The list shows staged changes; its buttons unstage them.
    Unstage,
}

impl Staging {
    /// The label of the button that moves every file of the list.
    pub fn all_label(self) -> &'static str {
        match self {
            Self::Stage => "Stage all",
            Self::Unstage => "Unstage all",
        }
    }

    /// The symbol of the per-file button.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Stage => "+",
            Self::Unstage => "−",
        }
    }

    /// The tooltip of the per-file or per-directory button.
    pub fn tooltip(self, name: &str) -> String {
        match self {
            Self::Stage => format!("Stage {name}"),
            Self::Unstage => format!("Unstage {name}"),
        }
    }

    /// What an empty list says.
    pub fn empty_text(self) -> &'static str {
        match self {
            Self::Stage => "No unstaged changes",
            Self::Unstage => "Nothing staged yet",
        }
    }

    /// The heading of the list, e.g. `2 staged files`.
    pub fn title(self, count: usize) -> String {
        let state = match self {
            Self::Stage => "unstaged",
            Self::Unstage => "staged",
        };
        match count {
            1 => format!("1 {state} file"),
            n => format!("{n} {state} files"),
        }
    }
}

/// The changes inside the directory `dir` (a full path without a trailing
/// slash), at any depth.
pub fn changes_under(changes: &[FileChange], dir: &str) -> Vec<FileChange> {
    let prefix = format!("{dir}/");
    changes
        .iter()
        .filter(|change| change.path.starts_with(&prefix))
        .cloned()
        .collect()
}

/// Why the commit button is disabled, or `None` if a commit can be made.
pub fn commit_blocker(
    staged: usize,
    message: &str,
    ongoing: Option<OngoingOperation>,
) -> Option<String> {
    if let Some(operation) = ongoing {
        return Some(if can_continue(operation) {
            format!("A {operation} is in progress: stage the resolved files, then use Continue")
        } else {
            format!("A {operation} is in progress: finish or abort it with Git")
        });
    }
    if staged == 0 {
        return Some("Stage the changes to commit".to_owned());
    }
    if message.trim().is_empty() {
        return Some("Enter a commit message".to_owned());
    }
    None
}

/// The label of the commit button.
pub fn commit_label(staged: usize) -> String {
    match staged {
        0 => "Commit".to_owned(),
        1 => "Commit 1 file".to_owned(),
        n => format!("Commit {n} files"),
    }
}

/// What the status strip says after a commit.
pub fn committed_notice(id: CommitId, message: &str) -> String {
    let subject = message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    format!("Committed {}: {subject}", id.short())
}
