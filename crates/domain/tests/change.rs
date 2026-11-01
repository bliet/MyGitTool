use domain::{ChangeKind, FileChange, WorkTreeChanges};

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
fn a_rename_touches_both_paths() {
    let renamed = FileChange {
        old_path: Some("old.txt".into()),
        kind: ChangeKind::Renamed,
        ..change("new.txt")
    };
    assert_eq!(renamed.paths().collect::<Vec<_>>(), ["new.txt", "old.txt"]);
    assert_eq!(change("a.txt").paths().collect::<Vec<_>>(), ["a.txt"]);
}

#[test]
fn a_file_with_staged_and_unstaged_changes_counts_once() {
    let changes = WorkTreeChanges {
        staged: vec![change("a.txt"), change("b.txt")],
        unstaged: vec![change("b.txt"), change("c.txt")],
        conflicts: Vec::new(),
    };
    assert_eq!(changes.file_count(), 3);
    assert!(!changes.is_empty());
}

#[test]
fn no_changes_are_empty() {
    assert!(WorkTreeChanges::default().is_empty());
    let only_unstaged = WorkTreeChanges {
        unstaged: vec![change("a.txt")],
        conflicts: Vec::new(),
        ..WorkTreeChanges::default()
    };
    assert!(!only_unstaged.is_empty());
}
