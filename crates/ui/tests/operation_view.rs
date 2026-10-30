use application::{FetchTarget, PullOutcome};
use domain::{CommitId, OngoingOperation, Progress};
use ui::view::branch_tree::{BranchItem, remote_key, remote_of_folder};
use ui::view::operation::{
    RemoteAction, branch_menu, conflict_title, progress_fraction, progress_text, pull_summary,
    remote_menu, running_label,
};

fn branch(name: &str, is_head: bool, remote: Option<&str>) -> BranchItem {
    BranchItem {
        label: name.rsplit('/').next().unwrap().into(),
        name: name.into(),
        refname: format!("refs/heads/{name}"),
        target: CommitId::from_bytes([1; 20]),
        is_head,
        ahead: None,
        behind: None,
        upstream_gone: false,
        remote: remote.map(str::to_owned),
    }
}

fn actions(entries: Vec<ui::view::operation::MenuEntry>) -> Vec<RemoteAction> {
    entries.into_iter().map(|e| e.action).collect()
}

#[test]
fn the_current_branch_can_fetch_pull_and_push() {
    assert_eq!(
        actions(branch_menu(&branch("main", true, None))),
        [
            RemoteAction::Fetch(FetchTarget::Branch("main".into())),
            RemoteAction::Pull,
            RemoteAction::Push {
                branch: "main".into()
            },
        ]
    );
}

#[test]
fn other_local_branches_cannot_pull() {
    let entries = actions(branch_menu(&branch("feature/x", false, None)));
    assert!(!entries.contains(&RemoteAction::Pull));
    assert!(entries.contains(&RemoteAction::Push {
        branch: "feature/x".into()
    }));
}

#[test]
fn remote_branches_and_remotes_fetch_their_remote() {
    let fetch_origin = [RemoteAction::Fetch(FetchTarget::Remote("origin".into()))];
    assert_eq!(
        actions(branch_menu(&branch("origin/main", false, Some("origin")))),
        fetch_origin
    );
    assert_eq!(actions(remote_menu("origin")), fetch_origin);
    assert_eq!(remote_menu("origin")[0].label, "Fetch origin");
}

#[test]
fn only_top_level_remote_folders_are_remotes() {
    assert_eq!(remote_of_folder(&remote_key("origin")), Some("origin"));
    assert_eq!(remote_of_folder("remote:origin:feature"), None);
    assert_eq!(remote_of_folder("local:feature"), None);
}

#[test]
fn running_labels_name_what_runs() {
    assert_eq!(
        running_label(&RemoteAction::Fetch(FetchTarget::All)),
        "Fetching all remotes…"
    );
    assert_eq!(
        running_label(&RemoteAction::Fetch(FetchTarget::Remote("origin".into()))),
        "Fetching origin…"
    );
    assert_eq!(
        running_label(&RemoteAction::Push {
            branch: "main".into()
        }),
        "Pushing main…"
    );
}

#[test]
fn progress_is_described_and_measured() {
    let receiving = Progress::Receiving {
        objects: 50,
        total: 200,
        bytes: 3 * 1024 * 1024,
    };
    assert_eq!(
        progress_text(&receiving),
        "Receiving objects 50/200 (3.0 MB)"
    );
    assert_eq!(progress_fraction(&receiving), Some(0.25));
    let resolving = Progress::Resolving {
        deltas: 0,
        total: 0,
    };
    assert_eq!(progress_fraction(&resolving), None);
    let message = Progress::Message("Counting objects: 42".into());
    assert_eq!(progress_text(&message), "Counting objects: 42");
    assert_eq!(progress_fraction(&message), None);
}

#[test]
fn pull_summaries() {
    let id = CommitId::from_bytes([0xab; 20]);
    assert_eq!(pull_summary(&PullOutcome::UpToDate), "Already up to date");
    assert_eq!(
        pull_summary(&PullOutcome::FastForwarded { to: id }),
        "Fast-forwarded to abababa"
    );
    assert_eq!(
        conflict_title(OngoingOperation::Rebase),
        "Pull stopped: the rebase has conflicts"
    );
}
