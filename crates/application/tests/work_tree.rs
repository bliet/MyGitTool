mod common;

use application::LoadWorkTreeChanges;
use common::FakeReader;
use domain::{ChangeKind, FileChange, RepositoryInfo};
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
fn work_tree_changes_are_sorted_by_path() {
    let reader = FakeReader {
        work_tree: vec![change("src/b.rs"), change("README.md"), change("src/a.rs")],
        ..FakeReader::default()
    };
    let changes = LoadWorkTreeChanges::new(Arc::new(reader))
        .load(&RepositoryInfo::new("/r"))
        .unwrap();
    let paths: Vec<_> = changes.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, ["README.md", "src/a.rs", "src/b.rs"]);
}
