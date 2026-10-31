use application::{CheckoutTarget, FetchTarget, IntegrationOutcome, OngoingState};
use domain::{CommitId, OngoingOperation, Progress, web_link};
use ui::view::branch_tree::{BranchItem, TagItem, remote_key, remote_of_folder};
use ui::view::operation::{
    Action, MenuEntry, branch_menu, can_continue, commit_menu, conflict_title, dirty_texts,
    integration_summary, ongoing_text, progress_fraction, progress_text, remote_menu,
    running_label, tag_menu,
};

fn tip() -> CommitId {
    CommitId::from_bytes([1; 20])
}

fn local(name: &str, is_head: bool) -> BranchItem {
    BranchItem {
        label: name.rsplit('/').next().unwrap().into(),
        name: name.into(),
        refname: format!("refs/heads/{name}"),
        target: tip(),
        is_head,
        ahead: None,
        behind: None,
        upstream_gone: false,
        remote: None,
        web: None,
    }
}

fn remote(remote: &str, name: &str) -> BranchItem {
    BranchItem {
        label: name.rsplit('/').next().unwrap().into(),
        name: format!("{remote}/{name}"),
        refname: format!("refs/remotes/{remote}/{name}"),
        target: tip(),
        is_head: false,
        ahead: None,
        behind: None,
        upstream_gone: false,
        remote: Some(remote.into()),
        web: None,
    }
}

fn actions(entries: Vec<MenuEntry>) -> Vec<Action> {
    entries.into_iter().map(|e| e.action).collect()
}

fn labels(entries: &[MenuEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.label.as_str()).collect()
}

#[test]
fn the_current_branch_can_be_branched_from_fetched_pulled_and_pushed() {
    assert_eq!(
        actions(branch_menu(&local("main", true))),
        [
            Action::CreateBranch {
                start: tip(),
                from: "main".into()
            },
            Action::Fetch(FetchTarget::Branch("main".into())),
            Action::Pull,
            Action::Push {
                branch: "main".into()
            },
        ]
    );
}

#[test]
fn another_local_branch_offers_every_branch_operation_but_pull() {
    let entries = branch_menu(&local("feature/x", false));
    assert_eq!(
        labels(&entries),
        [
            "Check out feature/x",
            "Merge feature/x into current",
            "Rebase current onto feature/x",
            "Create branch from here…",
            "Fetch upstream",
            "Push feature/x",
            "Delete feature/x",
        ]
    );
    let actions = actions(entries);
    assert_eq!(
        actions[0],
        Action::Checkout(CheckoutTarget::Local("feature/x".into()))
    );
    assert_eq!(
        actions[1],
        Action::Merge {
            refname: "refs/heads/feature/x".into(),
            name: "feature/x".into()
        }
    );
    assert!(!actions.contains(&Action::Pull));
}

#[test]
fn a_remote_branch_checks_out_as_a_tracking_branch_and_deletes_on_the_server() {
    let entries = branch_menu(&remote("origin", "feature/x"));
    assert_eq!(
        labels(&entries),
        [
            "Check out as feature/x",
            "Merge origin/feature/x into current",
            "Rebase current onto origin/feature/x",
            "Create branch from here…",
            "Fetch origin",
            "Delete feature/x from origin…",
        ]
    );
    let actions = actions(entries);
    assert_eq!(
        actions[0],
        Action::Checkout(CheckoutTarget::Remote {
            remote: "origin".into(),
            branch: "feature/x".into()
        })
    );
    assert_eq!(
        actions[5],
        Action::DeleteRemote {
            remote: "origin".into(),
            branch: "feature/x".into()
        }
    );
}

#[test]
fn menus_group_entries_with_separators() {
    let separated: Vec<bool> = branch_menu(&local("x", false))
        .iter()
        .map(|e| e.separated)
        .collect();
    assert_eq!(separated, [false, false, false, true, true, false, true]);
}

#[test]
fn remotes_fetch_and_commits_branch() {
    assert_eq!(
        actions(remote_menu("origin", None)),
        [Action::Fetch(FetchTarget::Remote("origin".into()))]
    );
    assert_eq!(remote_menu("origin", None)[0].label, "Fetch origin");
    let id = CommitId::from_bytes([0xab; 20]);
    assert_eq!(
        actions(commit_menu(id)),
        [
            Action::CreateBranch {
                start: id,
                from: "abababa".into()
            },
            Action::CreateTag {
                target: id,
                from: "abababa".into()
            }
        ]
    );
}

#[test]
fn tags_branch_push_and_delete_per_remote() {
    let tag = TagItem {
        name: "v1".into(),
        target: tip(),
    };
    let remotes = ["origin".to_owned(), "fork".to_owned()];
    let menu = tag_menu(&tag, &remotes);
    let labels: Vec<&str> = menu.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Create branch from here…",
            "Push v1 to origin",
            "Push v1 to fork",
            "Delete v1",
            "Delete v1 from origin…",
            "Delete v1 from fork…",
        ]
    );
    let separated: Vec<bool> = menu.iter().map(|e| e.separated).collect();
    assert_eq!(separated, [false, true, false, true, false, false]);
    assert_eq!(
        menu[2].action,
        Action::PushTag {
            tag: "v1".into(),
            remote: "fork".into()
        }
    );
    assert_eq!(running_label(&menu[4].action), "Deleting v1 from origin…");
    // Without remotes, a tag can only be branched from and deleted.
    assert_eq!(tag_menu(&tag, &[]).len(), 2);
}

#[test]
fn only_top_level_remote_folders_are_remotes() {
    assert_eq!(remote_of_folder(&remote_key("origin")), Some("origin"));
    assert_eq!(remote_of_folder("remote:origin:feature"), None);
    assert_eq!(remote_of_folder("local:feature"), None);
}

#[test]
fn running_labels_name_what_runs() {
    let cases = [
        (Action::Fetch(FetchTarget::All), "Fetching all remotes…"),
        (
            Action::Fetch(FetchTarget::Remote("origin".into())),
            "Fetching origin…",
        ),
        (
            Action::Push {
                branch: "main".into(),
            },
            "Pushing main…",
        ),
        (
            Action::Checkout(CheckoutTarget::Remote {
                remote: "origin".into(),
                branch: "x".into(),
            }),
            "Checking out x…",
        ),
        (
            Action::Merge {
                refname: "refs/heads/x".into(),
                name: "x".into(),
            },
            "Merging x…",
        ),
        (
            Action::Rebase {
                refname: "refs/heads/x".into(),
                name: "x".into(),
            },
            "Rebasing onto x…",
        ),
        (
            Action::DeleteRemote {
                remote: "origin".into(),
                branch: "x".into(),
            },
            "Deleting x from origin…",
        ),
    ];
    for (action, label) in cases {
        assert_eq!(running_label(&action), label);
    }
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
fn integration_summaries_and_conflict_titles() {
    let id = CommitId::from_bytes([0xab; 20]);
    assert_eq!(
        integration_summary(&IntegrationOutcome::UpToDate),
        "Already up to date"
    );
    assert_eq!(
        integration_summary(&IntegrationOutcome::FastForwarded { to: id }),
        "Fast-forwarded to abababa"
    );
    assert_eq!(
        integration_summary(&IntegrationOutcome::Merged),
        "Merge completed"
    );
    assert_eq!(
        conflict_title("Pull", OngoingOperation::Rebase),
        "Pull stopped: the rebase has conflicts"
    );
    assert_eq!(
        conflict_title("Merge", OngoingOperation::Merge),
        "Merge stopped with conflicts"
    );
}

#[test]
fn a_blocked_switch_names_the_files_it_would_overwrite() {
    let (title, intro) = dirty_texts("Checkout");
    assert_eq!(title, "Checkout would overwrite local changes");
    assert!(intro.contains("would overwrite"));
    let (title, _) = dirty_texts("Pull");
    assert_eq!(title, "Pull needs a clean work tree");
}

#[test]
fn the_banner_explains_the_ongoing_operation() {
    let merge = |conflicts: &[&str]| OngoingState {
        operation: OngoingOperation::Merge,
        conflicts: conflicts.iter().map(|c| c.to_string()).collect(),
    };
    assert_eq!(
        ongoing_text(&merge(&["a.txt"])),
        "A merge is in progress. 1 file has conflicts: a.txt"
    );
    assert_eq!(
        ongoing_text(&merge(&["a.txt", "b.txt"])),
        "A merge is in progress. 2 files have conflicts: a.txt, b.txt"
    );
    assert_eq!(
        ongoing_text(&merge(&[])),
        "A merge is in progress. All conflicts are resolved."
    );
    let picking = OngoingState {
        operation: OngoingOperation::CherryPick,
        conflicts: vec![],
    };
    assert_eq!(
        ongoing_text(&picking),
        "A cherry-pick is in progress. Finish or abort it with Git."
    );
    assert!(can_continue(OngoingOperation::Rebase));
    assert!(!can_continue(OngoingOperation::Revert));
}

#[test]
fn remotes_on_a_known_hoster_can_be_opened_in_the_browser() {
    let web = web_link("git@github.com:owner/repo.git", None).unwrap();
    let menu = remote_menu("origin", Some(&web));
    assert_eq!(menu.len(), 2);
    assert_eq!(menu[1].label, "Open on GitHub");
    assert_eq!(menu[1].action, Action::OpenInBrowser(web));

    let branch_web = web_link("git@github.com:owner/repo.git", Some("fix/x")).unwrap();
    let branch = BranchItem {
        web: Some(branch_web.clone()),
        ..remote("origin", "fix/x")
    };
    let menu = branch_menu(&branch);
    let last = menu.last().unwrap();
    assert!(
        menu.iter()
            .any(|e| e.action == Action::OpenInBrowser(branch_web.clone()))
    );
    // Deleting stays the last entry.
    assert!(matches!(last.action, Action::DeleteRemote { .. }));
}
