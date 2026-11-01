use domain::{ChangeKind, CommitId, FileChange, OngoingOperation};
use ui::view::commit::{
    Staging, changes_under, commit_blocker, commit_label, committed_notice, without_conflicts,
};

fn change(path: &str) -> FileChange {
    FileChange {
        path: path.into(),
        old_path: None,
        kind: ChangeKind::Modified,
        additions: 1,
        deletions: 0,
        binary: false,
    }
}

#[test]
fn a_directory_holds_the_changes_below_it() {
    let changes = [
        change("src/a.rs"),
        change("src/deep/b.rs"),
        change("src-old/c.rs"),
        change("README.md"),
    ];
    let paths: Vec<_> = changes_under(&changes, "src")
        .into_iter()
        .map(|c| c.path)
        .collect();
    assert_eq!(paths, ["src/a.rs", "src/deep/b.rs"]);
}

#[test]
fn a_commit_needs_staged_changes_and_a_message() {
    assert_eq!(
        commit_blocker(0, "Message", None, 0).as_deref(),
        Some("Stage the changes to commit")
    );
    assert_eq!(
        commit_blocker(2, " \n", None, 0).as_deref(),
        Some("Enter a commit message")
    );
    assert_eq!(commit_blocker(2, "Message", None, 0), None);
}

#[test]
fn an_operation_in_progress_blocks_the_commit() {
    assert_eq!(
        commit_blocker(1, "Message", Some(OngoingOperation::Merge), 1).as_deref(),
        Some("A merge is in progress: stage the resolved files, then use Continue")
    );
    assert_eq!(
        commit_blocker(1, "Message", Some(OngoingOperation::CherryPick), 0).as_deref(),
        Some("A cherry-pick is in progress: finish or abort it with Git")
    );
}

#[test]
fn texts_count_the_files() {
    assert_eq!(commit_label(0), "Commit");
    assert_eq!(commit_label(1), "Commit 1 file");
    assert_eq!(commit_label(3), "Commit 3 files");
    assert_eq!(Staging::Unstage.title(1), "1 staged file");
    assert_eq!(Staging::Stage.title(2), "2 unstaged files");
}

#[test]
fn the_notice_names_the_commit_and_its_subject() {
    let id = CommitId::from_bytes([0xab; 20]);
    assert_eq!(
        committed_notice(id, "\n  Fix the parser\n\nDetails"),
        "Committed abababa: Fix the parser"
    );
}

#[test]
fn conflicts_block_the_commit_and_are_never_staged_in_bulk() {
    assert_eq!(
        commit_blocker(1, "Message", None, 2).as_deref(),
        Some("Resolve the conflicts first")
    );
    let paths: Vec<_> = without_conflicts(
        vec![change("a.txt"), change("b.txt"), change("c.txt")],
        &["b.txt".to_owned()],
    )
    .into_iter()
    .map(|c| c.path)
    .collect();
    assert_eq!(paths, ["a.txt", "c.txt"]);
}
