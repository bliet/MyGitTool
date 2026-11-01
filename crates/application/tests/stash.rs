mod common;

use application::{ApplyStash, DropStash, Integration, OperationError, SaveStash, StashOutcome};
use common::{FakeReader, FakeWriter, stash};
use domain::{
    ChangeKind, DirtyWorkTree, FileChange, OngoingOperation, RepositoryInfo, WorkTreeChanges,
};
use std::sync::Arc;

fn repo() -> RepositoryInfo {
    RepositoryInfo::new("/repo")
}

fn untracked(path: &str) -> FileChange {
    FileChange {
        path: path.into(),
        old_path: None,
        kind: ChangeKind::Added,
        additions: 1,
        deletions: 0,
        binary: false,
    }
}

/// A reader with a modified tracked file.
fn dirty() -> FakeReader {
    FakeReader {
        dirty: vec!["a.txt".into()],
        ..FakeReader::default()
    }
}

/// A reader whose only change is an untracked file.
fn only_untracked() -> FakeReader {
    FakeReader {
        work_tree: WorkTreeChanges {
            staged: Vec::new(),
            unstaged: vec![untracked("new.txt")],
        },
        ..FakeReader::default()
    }
}

/// A reader with two stashes: stash 10 at index 0, stash 11 at index 1.
fn with_stashes() -> FakeReader {
    FakeReader {
        stashes: vec![stash(0, 10), stash(1, 11)],
        ..FakeReader::default()
    }
}

fn conflicts() -> FakeWriter {
    FakeWriter {
        integration: Integration::Conflicts(vec!["a.txt".into()]),
        ..FakeWriter::default()
    }
}

// Save

#[test]
fn saves_with_a_trimmed_message() {
    let writer = Arc::new(FakeWriter::default());
    SaveStash::new(Arc::new(dirty()), writer.clone())
        .save(&repo(), "  half done \n", false)
        .unwrap();
    assert_eq!(
        writer.calls(),
        [r#"stash_save Some("half done") untracked=false"#]
    );
}

#[test]
fn a_blank_message_lets_git_name_the_stash() {
    let writer = Arc::new(FakeWriter::default());
    SaveStash::new(Arc::new(dirty()), writer.clone())
        .save(&repo(), " ", true)
        .unwrap();
    assert_eq!(writer.calls(), ["stash_save None untracked=true"]);
}

#[test]
fn untracked_files_alone_need_include_untracked() {
    let writer = Arc::new(FakeWriter::default());
    let save = SaveStash::new(Arc::new(only_untracked()), writer.clone());
    assert_eq!(
        save.save(&repo(), "", false),
        Err(OperationError::NothingToStash)
    );
    save.save(&repo(), "", true).unwrap();
    assert_eq!(writer.calls(), ["stash_save None untracked=true"]);
}

#[test]
fn a_clean_work_tree_has_nothing_to_stash() {
    let writer = Arc::new(FakeWriter::default());
    let result =
        SaveStash::new(Arc::new(FakeReader::default()), writer.clone()).save(&repo(), "", true);
    assert_eq!(result, Err(OperationError::NothingToStash));
    assert!(writer.calls().is_empty());
}

#[test]
fn saving_during_a_merge_is_refused() {
    let reader = FakeReader {
        ongoing: Some(OngoingOperation::Merge),
        ..dirty()
    };
    let result =
        SaveStash::new(Arc::new(reader), Arc::new(FakeWriter::default())).save(&repo(), "", false);
    assert_eq!(
        result,
        Err(OperationError::OperationInProgress(OngoingOperation::Merge))
    );
}

// Apply and pop

#[test]
fn applies_the_stash_at_its_current_index() {
    // The UI saw stash 11 at index 0; another stash was saved since.
    let writer = Arc::new(FakeWriter::default());
    let outcome = ApplyStash::new(Arc::new(with_stashes()), writer.clone())
        .apply(&repo(), &stash(0, 11))
        .unwrap();
    assert_eq!(outcome, StashOutcome::Applied);
    assert_eq!(writer.calls(), ["stash_apply 1"]);
}

#[test]
fn pop_drops_the_stash_once_applied() {
    let writer = Arc::new(FakeWriter::default());
    ApplyStash::new(Arc::new(with_stashes()), writer.clone())
        .pop(&repo(), &stash(1, 11))
        .unwrap();
    assert_eq!(writer.calls(), ["stash_apply 1", "stash_drop 1"]);
}

#[test]
fn pop_keeps_the_stash_when_it_conflicts() {
    let writer = Arc::new(conflicts());
    let outcome = ApplyStash::new(Arc::new(with_stashes()), writer.clone())
        .pop(&repo(), &stash(0, 10))
        .unwrap();
    assert_eq!(outcome, StashOutcome::Conflicts(vec!["a.txt".into()]));
    assert_eq!(writer.calls(), ["stash_apply 0"]);
}

#[test]
fn applying_needs_a_clean_work_tree() {
    let reader = FakeReader {
        dirty: vec!["a.txt".into()],
        ..with_stashes()
    };
    let writer = Arc::new(FakeWriter::default());
    let result = ApplyStash::new(Arc::new(reader), writer.clone()).apply(&repo(), &stash(0, 10));
    assert_eq!(
        result,
        Err(OperationError::DirtyWorkTree(DirtyWorkTree {
            files: vec!["a.txt".into()]
        }))
    );
    assert!(writer.calls().is_empty());
}

#[test]
fn a_stash_that_is_gone_is_reported() {
    let writer = Arc::new(FakeWriter::default());
    let result =
        ApplyStash::new(Arc::new(with_stashes()), writer.clone()).pop(&repo(), &stash(2, 12));
    assert_eq!(
        result,
        Err(OperationError::StashNotFound("stash@{2}".into()))
    );
    assert!(writer.calls().is_empty());
}

// Drop

#[test]
fn drops_the_stash_at_its_current_index() {
    let writer = Arc::new(FakeWriter::default());
    DropStash::new(Arc::new(with_stashes()), writer.clone())
        .drop_stash(&repo(), &stash(0, 11))
        .unwrap();
    assert_eq!(writer.calls(), ["stash_drop 1"]);
}
