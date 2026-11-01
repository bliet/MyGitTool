mod common;

use application::{ApplyPatch, CreatePatch, LoadWorkTreeChanges, OperationError};
use common::{FakeReader, FakeWriter};
use domain::{
    ChangeKind, DirtyWorkTree, FileChange, OngoingOperation, RepositoryInfo, WorkTreeChanges,
};
use std::sync::Arc;

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
fn staged_and_unstaged_changes_are_each_sorted_by_path() {
    let reader = FakeReader {
        work_tree: WorkTreeChanges {
            staged: vec![change("z.txt"), change("b.txt")],
            unstaged: vec![change("src/b.rs"), change("README.md"), change("src/a.rs")],
        },
        ..FakeReader::default()
    };
    let changes = LoadWorkTreeChanges::new(Arc::new(reader))
        .load(&RepositoryInfo::new("/r"))
        .unwrap();
    let paths = |list: &[FileChange]| list.iter().map(|c| c.path.clone()).collect::<Vec<_>>();
    assert_eq!(paths(&changes.staged), ["b.txt", "z.txt"]);
    assert_eq!(
        paths(&changes.unstaged),
        ["README.md", "src/a.rs", "src/b.rs"]
    );
}

#[test]
fn the_patch_holds_the_uncommitted_changes() {
    let reader = FakeReader {
        patch: b"diff --git a/a.txt b/a.txt\n".to_vec(),
        ..FakeReader::default()
    };
    let patch = CreatePatch::new(Arc::new(reader))
        .patch(&RepositoryInfo::new("/r"))
        .unwrap();
    assert_eq!(patch, b"diff --git a/a.txt b/a.txt\n");
}

#[test]
fn a_clean_work_tree_makes_no_patch() {
    let result =
        CreatePatch::new(Arc::new(FakeReader::default())).patch(&RepositoryInfo::new("/r"));
    assert_eq!(result, Err(OperationError::NothingToPatch));
}

#[test]
fn applies_a_patch_to_a_clean_work_tree() {
    let writer = Arc::new(FakeWriter::default());
    ApplyPatch::new(Arc::new(FakeReader::default()), writer.clone())
        .apply(&RepositoryInfo::new("/r"), b"diff")
        .unwrap();
    assert_eq!(writer.calls(), ["apply_patch diff"]);
}

#[test]
fn applying_a_patch_needs_a_clean_work_tree() {
    let reader = FakeReader {
        dirty: vec!["a.txt".into()],
        ..FakeReader::default()
    };
    let writer = Arc::new(FakeWriter::default());
    let result = ApplyPatch::new(Arc::new(reader), writer.clone())
        .apply(&RepositoryInfo::new("/r"), b"diff");
    assert_eq!(
        result,
        Err(OperationError::DirtyWorkTree(DirtyWorkTree {
            files: vec!["a.txt".into()]
        }))
    );
    assert!(writer.calls().is_empty());
}

#[test]
fn an_empty_patch_or_an_operation_in_progress_is_refused() {
    let writer = Arc::new(FakeWriter::default());
    let apply = ApplyPatch::new(Arc::new(FakeReader::default()), writer.clone());
    assert_eq!(
        apply.apply(&RepositoryInfo::new("/r"), b" \n"),
        Err(OperationError::EmptyPatch)
    );
    let reader = FakeReader {
        ongoing: Some(OngoingOperation::Rebase),
        ..FakeReader::default()
    };
    assert_eq!(
        ApplyPatch::new(Arc::new(reader), writer.clone())
            .apply(&RepositoryInfo::new("/r"), b"diff"),
        Err(OperationError::OperationInProgress(
            OngoingOperation::Rebase
        ))
    );
    assert!(writer.calls().is_empty());
}
