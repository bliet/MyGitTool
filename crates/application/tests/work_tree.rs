mod common;

use application::LoadWorkTreeChanges;
use common::FakeReader;
use domain::{ChangeKind, FileChange, RepositoryInfo, WorkTreeChanges};
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
