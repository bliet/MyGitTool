mod common;

use application::{CreateCommit, OperationError, StageChanges};
use common::{FakeReader, FakeWriter, id};
use domain::{ChangeKind, FileChange, OngoingOperation, RepositoryInfo, WorkTreeChanges};
use std::sync::Arc;

fn repo() -> RepositoryInfo {
    RepositoryInfo::new("/repo")
}

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

fn renamed(from: &str, to: &str) -> FileChange {
    FileChange {
        old_path: Some(from.into()),
        kind: ChangeKind::Renamed,
        ..change(to)
    }
}

/// A reader with `a.txt` staged.
fn with_staged() -> FakeReader {
    FakeReader {
        work_tree: WorkTreeChanges {
            staged: vec![change("a.txt")],
            unstaged: Vec::new(),
            conflicts: Vec::new(),
        },
        ..FakeReader::default()
    }
}

#[test]
fn stages_the_paths_of_the_changes() {
    let writer = Arc::new(FakeWriter::default());
    StageChanges::new(writer.clone())
        .stage(&repo(), &[change("b.txt"), change("a.txt")])
        .unwrap();
    assert_eq!(writer.calls(), ["stage a.txt b.txt"]);
}

#[test]
fn unstaging_a_rename_resets_both_paths() {
    let writer = Arc::new(FakeWriter::default());
    StageChanges::new(writer.clone())
        .unstage(&repo(), &[renamed("old.txt", "new.txt")])
        .unwrap();
    assert_eq!(writer.calls(), ["unstage new.txt old.txt"]);
}

#[test]
fn staging_nothing_does_not_touch_the_index() {
    let writer = Arc::new(FakeWriter::default());
    let stage = StageChanges::new(writer.clone());
    stage.stage(&repo(), &[]).unwrap();
    stage.unstage(&repo(), &[]).unwrap();
    assert!(writer.calls().is_empty());
}

#[test]
fn commits_the_cleaned_up_message() {
    let writer = Arc::new(FakeWriter::default());
    let created = CreateCommit::new(Arc::new(with_staged()), writer.clone())
        .commit(&repo(), "\n\nFix the parser  \n\n\n\n  It crashed.\t\n\n")
        .unwrap();
    assert_eq!(created, id(9));
    assert_eq!(
        writer.calls(),
        [r#"commit "Fix the parser\n\n  It crashed.\n""#]
    );
}

#[test]
fn a_blank_message_is_refused() {
    let writer = Arc::new(FakeWriter::default());
    let result =
        CreateCommit::new(Arc::new(with_staged()), writer.clone()).commit(&repo(), " \n\t");
    assert_eq!(result, Err(OperationError::EmptyCommitMessage));
    assert!(writer.calls().is_empty());
}

#[test]
fn nothing_staged_is_refused() {
    let reader = FakeReader {
        work_tree: WorkTreeChanges {
            staged: Vec::new(),
            unstaged: vec![change("a.txt")],
            conflicts: Vec::new(),
        },
        ..FakeReader::default()
    };
    let writer = Arc::new(FakeWriter::default());
    let result = CreateCommit::new(Arc::new(reader), writer.clone()).commit(&repo(), "Message");
    assert_eq!(result, Err(OperationError::NothingStaged));
    assert!(writer.calls().is_empty());
}

#[test]
fn a_merge_in_progress_is_finished_with_continue_instead() {
    let reader = FakeReader {
        ongoing: Some(OngoingOperation::Merge),
        ..with_staged()
    };
    let writer = Arc::new(FakeWriter::default());
    let result = CreateCommit::new(Arc::new(reader), writer.clone()).commit(&repo(), "Message");
    assert_eq!(
        result,
        Err(OperationError::OperationInProgress(OngoingOperation::Merge))
    );
    assert!(writer.calls().is_empty());
}

#[test]
fn unresolved_conflicts_are_refused() {
    let reader = FakeReader {
        conflicts: vec!["a.txt".into()],
        ..with_staged()
    };
    let writer = Arc::new(FakeWriter::default());
    let result = CreateCommit::new(Arc::new(reader), writer.clone()).commit(&repo(), "Message");
    assert_eq!(
        result,
        Err(OperationError::UnresolvedConflicts(vec!["a.txt".into()]))
    );
}
