use domain::{ChangeKind, FileChange};
use ui::view::file_tree::{FileViewMode, file_rows};

fn change(path: &str) -> FileChange {
    FileChange {
        path: path.into(),
        old_path: None,
        kind: ChangeKind::Modified,
        additions: 0,
        deletions: 0,
        binary: false,
    }
}

fn changes() -> Vec<FileChange> {
    [
        "src/ui/view/a.rs",
        "README.md",
        "src/main.rs",
        "docs/deep/only/file.md",
    ]
    .into_iter()
    .map(change)
    .collect()
}

fn render(mode: FileViewMode) -> Vec<String> {
    let changes = changes();
    file_rows(&changes, mode)
        .into_iter()
        .map(|row| {
            let suffix = if row.change.is_none() { "/" } else { "" };
            format!("{}{}{suffix}", "  ".repeat(row.depth), row.name)
        })
        .collect()
}

#[test]
fn tree_mode_nests_and_compresses_single_directory_chains() {
    assert_eq!(
        render(FileViewMode::Tree),
        [
            "docs/deep/only/",
            "  file.md",
            "src/",
            "  ui/view/",
            "    a.rs",
            "  main.rs",
            "README.md",
        ]
    );
}

#[test]
fn flat_mode_lists_full_paths_sorted() {
    assert_eq!(
        render(FileViewMode::Flat),
        [
            "README.md",
            "docs/deep/only/file.md",
            "src/main.rs",
            "src/ui/view/a.rs"
        ]
    );
}

#[test]
fn rows_point_back_to_their_change() {
    let changes = changes();
    let rows = file_rows(&changes, FileViewMode::Flat);
    assert_eq!(changes[rows[0].change.unwrap()].path, "README.md");
}
