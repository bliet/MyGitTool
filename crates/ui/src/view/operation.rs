//! Texts and menus for the branch and remote operations.

use crate::view::branch_tree::{BranchItem, TagItem};
use crate::view::diff::format_size;
use application::{CheckoutTarget, FetchTarget, ForcePushPlan, IntegrationOutcome, OngoingState};
use domain::{CommitId, OngoingOperation, Progress, Stash, WebLink};

/// An operation the user can start from a menu or the toolbar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Opens a page of the repository's hoster in the browser.
    OpenInBrowser(WebLink),
    Fetch(FetchTarget),
    /// Pulls the current branch.
    Pull,
    Push {
        branch: String,
    },
    /// Shows what a force push would replace, then force pushes with a
    /// lease after confirmation.
    ForcePush {
        branch: String,
    },
    Checkout(CheckoutTarget),
    /// Asks for a name, then creates a branch at `start`. `from` names the
    /// start for the dialog, e.g. a branch name or a short hash.
    CreateBranch {
        start: CommitId,
        from: String,
    },
    /// Asks for confirmation, then deletes the local branch.
    DeleteLocal {
        branch: String,
    },
    /// Asks for confirmation, then deletes the branch on the remote.
    DeleteRemote {
        remote: String,
        branch: String,
    },
    /// Asks for a name and a message, then creates a tag at `target`.
    /// `from` names the target for the dialog.
    CreateTag {
        target: CommitId,
        from: String,
    },
    PushTag {
        tag: String,
        remote: String,
    },
    DeleteTag {
        tag: String,
    },
    /// Asks for confirmation, then deletes the tag on the remote.
    DeleteRemoteTag {
        remote: String,
        tag: String,
    },
    /// Merges `refname` (shown as `name`) into the current branch.
    Merge {
        refname: String,
        name: String,
    },
    /// Rebases the current branch onto `refname` (shown as `name`).
    Rebase {
        refname: String,
        name: String,
    },
    /// Asks for a message, then stashes the local changes.
    SaveStash,
    /// Applies the stash; `pop` also drops it.
    ApplyStash {
        stash: Stash,
        pop: bool,
    },
    /// Asks for confirmation, then drops the stash.
    DropStash {
        stash: Stash,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    pub label: String,
    pub action: Action,
    /// Draw a separator above this entry.
    pub separated: bool,
}

fn entry(label: impl Into<String>, action: Action) -> MenuEntry {
    MenuEntry {
        label: label.into(),
        action,
        separated: false,
    }
}

fn separated(mut entry: MenuEntry) -> MenuEntry {
    entry.separated = true;
    entry
}

/// The context menu of a branch.
///
/// A local branch can be checked out, merged into or rebased onto from the
/// current one, branched from, fetched, pushed and deleted; the current
/// branch can also be pulled but not checked out, merged or deleted. A
/// remote branch can be checked out as a local tracking branch, merged,
/// rebased onto, branched from, fetched and deleted on the server.
pub fn branch_menu(branch: &BranchItem) -> Vec<MenuEntry> {
    let create = entry(
        "Create branch from here…",
        Action::CreateBranch {
            start: branch.target,
            from: branch.name.clone(),
        },
    );
    let integrate = || {
        [
            entry(
                format!("Merge {} into current", branch.name),
                Action::Merge {
                    refname: branch.refname.clone(),
                    name: branch.name.clone(),
                },
            ),
            entry(
                format!("Rebase current onto {}", branch.name),
                Action::Rebase {
                    refname: branch.refname.clone(),
                    name: branch.name.clone(),
                },
            ),
        ]
    };

    if let Some(remote) = &branch.remote {
        let on_remote = branch
            .name
            .strip_prefix(remote.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or(&branch.name)
            .to_owned();
        let mut entries = vec![entry(
            format!("Check out as {on_remote}"),
            Action::Checkout(CheckoutTarget::Remote {
                remote: remote.clone(),
                branch: on_remote.clone(),
            }),
        )];
        entries.extend(integrate());
        entries.push(separated(create));
        entries.push(separated(fetch_remote(remote)));
        if let Some(web) = &branch.web {
            entries.push(open_in_browser(web));
        }
        entries.push(separated(entry(
            format!("Delete {on_remote} from {remote}…"),
            Action::DeleteRemote {
                remote: remote.clone(),
                branch: on_remote,
            },
        )));
        return entries;
    }

    let fetch = entry(
        "Fetch upstream",
        Action::Fetch(FetchTarget::Branch(branch.name.clone())),
    );
    let push = entry(
        format!("Push {}", branch.name),
        Action::Push {
            branch: branch.name.clone(),
        },
    );
    let force = branch.has_upstream().then(|| {
        entry(
            format!("Force push {}…", branch.name),
            Action::ForcePush {
                branch: branch.name.clone(),
            },
        )
    });
    if branch.is_head {
        let mut entries = vec![create, separated(fetch), entry("Pull", Action::Pull), push];
        entries.extend(force);
        return entries;
    }
    let mut entries = vec![entry(
        format!("Check out {}", branch.name),
        Action::Checkout(CheckoutTarget::Local(branch.name.clone())),
    )];
    entries.extend(integrate());
    entries.push(separated(create));
    entries.push(separated(fetch));
    entries.push(push);
    entries.extend(force);
    entries.push(separated(entry(
        format!("Delete {}", branch.name),
        Action::DeleteLocal {
            branch: branch.name.clone(),
        },
    )));
    entries
}

/// The context menu of a remote.
pub fn remote_menu(remote: &str, web: Option<&WebLink>) -> Vec<MenuEntry> {
    let mut entries = vec![fetch_remote(remote)];
    entries.extend(web.map(open_in_browser));
    entries
}

fn fetch_remote(remote: &str) -> MenuEntry {
    entry(
        format!("Fetch {remote}"),
        Action::Fetch(FetchTarget::Remote(remote.to_owned())),
    )
}

fn open_in_browser(web: &WebLink) -> MenuEntry {
    entry(
        format!("Open on {}", web.hoster),
        Action::OpenInBrowser(web.clone()),
    )
}

/// The context menu of a commit in the history.
pub fn commit_menu(id: CommitId) -> Vec<MenuEntry> {
    vec![
        entry(
            "Create branch here…",
            Action::CreateBranch {
                start: id,
                from: id.short(),
            },
        ),
        entry(
            "Create tag here…",
            Action::CreateTag {
                target: id,
                from: id.short(),
            },
        ),
    ]
}

/// The context menu of a tag: branch from it, push it to each remote, and
/// delete it here or on each remote.
pub fn tag_menu(tag: &TagItem, remotes: &[String]) -> Vec<MenuEntry> {
    let mut entries = vec![entry(
        "Create branch from here…",
        Action::CreateBranch {
            start: tag.target,
            from: tag.name.clone(),
        },
    )];
    for (i, remote) in remotes.iter().enumerate() {
        let push = entry(
            format!("Push {} to {remote}", tag.name),
            Action::PushTag {
                tag: tag.name.clone(),
                remote: remote.clone(),
            },
        );
        entries.push(if i == 0 { separated(push) } else { push });
    }
    entries.push(separated(entry(
        format!("Delete {}", tag.name),
        Action::DeleteTag {
            tag: tag.name.clone(),
        },
    )));
    for remote in remotes {
        entries.push(entry(
            format!("Delete {} from {remote}…", tag.name),
            Action::DeleteRemoteTag {
                remote: remote.clone(),
                tag: tag.name.clone(),
            },
        ));
    }
    entries
}

/// The context menu of a stash.
pub fn stash_menu(stash: &Stash) -> Vec<MenuEntry> {
    vec![
        entry(
            "Apply",
            Action::ApplyStash {
                stash: stash.clone(),
                pop: false,
            },
        ),
        entry(
            "Pop (apply and drop)",
            Action::ApplyStash {
                stash: stash.clone(),
                pop: true,
            },
        ),
        separated(entry(
            "Drop…",
            Action::DropStash {
                stash: stash.clone(),
            },
        )),
    ]
}

/// What the status strip says after a stash was applied or popped.
pub fn stash_applied(stash: &Stash, pop: bool) -> String {
    if pop {
        format!("Popped {}", stash.name())
    } else {
        format!("Applied {}", stash.name())
    }
}

/// What the status strip says while `action` runs.
pub fn running_label(action: &Action) -> String {
    match action {
        Action::OpenInBrowser(web) => format!("Opening {}…", web.url),
        Action::Fetch(FetchTarget::All) => "Fetching all remotes…".to_owned(),
        Action::Fetch(FetchTarget::Remote(remote)) => format!("Fetching {remote}…"),
        Action::Fetch(FetchTarget::Branch(branch)) => {
            format!("Fetching the upstream of {branch}…")
        }
        Action::Pull => "Pulling…".to_owned(),
        Action::Push { branch } => format!("Pushing {branch}…"),
        Action::ForcePush { branch } => format!("Force pushing {branch}…"),
        Action::Checkout(CheckoutTarget::Local(branch))
        | Action::Checkout(CheckoutTarget::Remote { branch, .. }) => {
            format!("Checking out {branch}…")
        }
        Action::CreateBranch { .. } => "Creating the branch…".to_owned(),
        Action::DeleteLocal { branch } => format!("Deleting {branch}…"),
        Action::DeleteRemote { remote, branch } => format!("Deleting {branch} from {remote}…"),
        Action::CreateTag { .. } => "Creating the tag…".to_owned(),
        Action::PushTag { tag, remote } => format!("Pushing {tag} to {remote}…"),
        Action::DeleteTag { tag } => format!("Deleting {tag}…"),
        Action::DeleteRemoteTag { remote, tag } => format!("Deleting {tag} from {remote}…"),
        Action::Merge { name, .. } => format!("Merging {name}…"),
        Action::Rebase { name, .. } => format!("Rebasing onto {name}…"),
        Action::SaveStash => "Stashing…".to_owned(),
        Action::ApplyStash { stash, pop: false } => format!("Applying {}…", stash.name()),
        Action::ApplyStash { stash, pop: true } => format!("Popping {}…", stash.name()),
        Action::DropStash { stash } => format!("Dropping {}…", stash.name()),
    }
}

/// The paragraphs of the force push confirmation: what is replaced, what
/// is lost, and the lease that keeps unseen commits safe.
pub fn force_push_texts(plan: &ForcePushPlan) -> Vec<String> {
    let remote_branch = plan
        .remote_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(&plan.remote_ref);
    let mut texts = vec![format!(
        "This replaces {remote_branch} on {} with your local {}, even though it is no fast-forward.",
        plan.remote, plan.branch
    )];
    texts.push(match plan.discarded {
        0 => format!("Nothing on {} is lost: {} contains all of it.", plan.tracking, plan.branch),
        1 => format!(
            "1 commit on {} that {} does not contain will be removed from {}, for everyone who uses it.",
            plan.tracking, plan.branch, plan.remote
        ),
        n => format!(
            "{n} commits on {} that {} does not contain will be removed from {}, for everyone who uses it.",
            plan.tracking, plan.branch, plan.remote
        ),
    });
    texts.push(match plan.expected {
        Some(id) => format!(
            "It only goes through while the branch on {} is still at {}, as last fetched. If someone pushed since, nothing is replaced.",
            plan.remote,
            id.short()
        ),
        None => format!(
            "It only goes through while {} has no branch {remote_branch}, as last fetched.",
            plan.remote
        ),
    });
    texts
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

/// What a finished pull, merge, rebase or continue did, for the status strip.
pub fn integration_summary(outcome: &IntegrationOutcome) -> String {
    match outcome {
        IntegrationOutcome::UpToDate => "Already up to date".to_owned(),
        IntegrationOutcome::FastForwarded { to } => format!("Fast-forwarded to {}", to.short()),
        IntegrationOutcome::Merged => "Merge completed".to_owned(),
        IntegrationOutcome::Rebased => "Rebase completed".to_owned(),
        IntegrationOutcome::Conflicts { operation, .. } => {
            format!("The {operation} stopped with conflicts")
        }
    }
}

/// The title of the dialog shown when `what` (e.g. "Pull") stops with
/// conflicts.
pub fn conflict_title(what: &str, operation: OngoingOperation) -> String {
    if what.eq_ignore_ascii_case(&operation.to_string()) {
        format!("{what} stopped with conflicts")
    } else {
        format!("{what} stopped: the {operation} has conflicts")
    }
}

/// The title and introduction of the dialog that lists the local changes
/// blocking `what`. A switch to another branch only minds the files it
/// would overwrite; everything else needs a clean work tree.
pub fn dirty_texts(what: &str) -> (String, String) {
    if matches!(what, "Checkout" | "Create branch") {
        (
            format!("{what} would overwrite local changes"),
            "These files have changes that the switch would overwrite. Commit or stash them first:"
                .to_owned(),
        )
    } else {
        (
            format!("{what} needs a clean work tree"),
            "Commit or stash the changes in these files first:".to_owned(),
        )
    }
}

/// The text of the banner shown while an operation is in progress.
pub fn ongoing_text(state: &OngoingState) -> String {
    let operation = state.operation;
    let progress = match state.conflicts.len() {
        0 => "All conflicts are resolved.".to_owned(),
        1 => format!("1 file has conflicts: {}", state.conflicts[0]),
        n => format!("{n} files have conflicts: {}", state.conflicts.join(", ")),
    };
    if can_continue(operation) {
        format!("A {operation} is in progress. {progress}")
    } else {
        format!("A {operation} is in progress. Finish or abort it with Git.")
    }
}

/// Whether the app can continue and abort `operation`.
pub fn can_continue(operation: OngoingOperation) -> bool {
    matches!(
        operation,
        OngoingOperation::Merge | OngoingOperation::Rebase
    )
}
