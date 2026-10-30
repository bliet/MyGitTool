//! Texts and menus for fetch, pull and push.

use crate::view::branch_tree::BranchItem;
use crate::view::diff::format_size;
use application::{FetchTarget, PullOutcome};
use domain::{OngoingOperation, Progress};

/// A remote operation the user can start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteAction {
    Fetch(FetchTarget),
    /// Pulls the current branch.
    Pull,
    Push {
        branch: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    pub label: String,
    pub action: RemoteAction,
}

fn entry(label: impl Into<String>, action: RemoteAction) -> MenuEntry {
    MenuEntry {
        label: label.into(),
        action,
    }
}

/// The context menu of a branch: a local branch fetches its upstream's
/// remote and pushes; the current one also pulls. A remote branch fetches
/// its remote.
pub fn branch_menu(branch: &BranchItem) -> Vec<MenuEntry> {
    match &branch.remote {
        Some(remote) => remote_menu(remote),
        None => {
            let mut entries = vec![entry(
                "Fetch upstream",
                RemoteAction::Fetch(FetchTarget::Branch(branch.name.clone())),
            )];
            if branch.is_head {
                entries.push(entry("Pull", RemoteAction::Pull));
            }
            entries.push(entry(
                format!("Push {}", branch.name),
                RemoteAction::Push {
                    branch: branch.name.clone(),
                },
            ));
            entries
        }
    }
}

/// The context menu of a remote.
pub fn remote_menu(remote: &str) -> Vec<MenuEntry> {
    vec![entry(
        format!("Fetch {remote}"),
        RemoteAction::Fetch(FetchTarget::Remote(remote.to_owned())),
    )]
}

/// What the status strip says while `action` runs.
pub fn running_label(action: &RemoteAction) -> String {
    match action {
        RemoteAction::Fetch(FetchTarget::All) => "Fetching all remotes…".to_owned(),
        RemoteAction::Fetch(FetchTarget::Remote(remote)) => format!("Fetching {remote}…"),
        RemoteAction::Fetch(FetchTarget::Branch(branch)) => {
            format!("Fetching the upstream of {branch}…")
        }
        RemoteAction::Pull => "Pulling…".to_owned(),
        RemoteAction::Push { branch } => format!("Pushing {branch}…"),
    }
}

/// A one-line description of the progress.
pub fn progress_text(progress: &Progress) -> String {
    match progress {
        Progress::Receiving {
            objects,
            total,
            bytes,
        } => format!(
            "Receiving objects {objects}/{total} ({})",
            format_size(*bytes as u64)
        ),
        Progress::Resolving { deltas, total } => format!("Resolving deltas {deltas}/{total}"),
        Progress::Sending {
            objects,
            total,
            bytes,
        } => format!(
            "Sending objects {objects}/{total} ({})",
            format_size(*bytes as u64)
        ),
        Progress::Message(message) => message.clone(),
    }
}

/// How far along the transfer is, from 0 to 1; `None` if unknown.
pub fn progress_fraction(progress: &Progress) -> Option<f64> {
    let (done, total) = match progress {
        Progress::Receiving { objects, total, .. } | Progress::Sending { objects, total, .. } => {
            (*objects, *total)
        }
        Progress::Resolving { deltas, total } => (*deltas, *total),
        Progress::Message(_) => return None,
    };
    // Object counts fit easily into an f64 mantissa.
    (total > 0).then(|| (done as f64 / total as f64).clamp(0.0, 1.0))
}

/// What a finished pull did, for the status strip.
pub fn pull_summary(outcome: &PullOutcome) -> String {
    match outcome {
        PullOutcome::UpToDate => "Already up to date".to_owned(),
        PullOutcome::FastForwarded { to } => format!("Fast-forwarded to {}", to.short()),
        PullOutcome::Merged => "Merged the upstream".to_owned(),
        PullOutcome::Rebased => "Rebased onto the upstream".to_owned(),
        PullOutcome::Conflicts { operation, .. } => {
            format!("The {operation} stopped with conflicts")
        }
    }
}

/// The title of the dialog shown when a pull stops with conflicts.
pub fn conflict_title(operation: OngoingOperation) -> String {
    format!("Pull stopped: the {operation} has conflicts")
}
