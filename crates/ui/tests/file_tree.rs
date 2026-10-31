use domain::{ChangeKind, FileChange};
use std::collections::HashSet;
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
    render_collapsed(mode, &[])
}

fn render_collapsed(mode: FileViewMode, collapsed: &[&str]) -> Vec<String> {
    let changes = changes();
    let collapsed: HashSet<String> = collapsed.iter().map(|d| d.to_string()).collect();
    file_rows(&changes, mode, &collapsed)
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
    let rows = file_rows(&changes, FileViewMode::Flat, &HashSet::new());
    assert_eq!(changes[rows[0].change.unwrap()].path, "README.md");
}

#[test]
fn collapsed_directories_hide_their_contents() {
    assert_eq!(
        render_collapsed(FileViewMode::Tree, &["src"]),
        ["docs/deep/only/", "  file.md", "src/", "README.md"]
    );
    // A compressed chain is collapsed by its full path.
    assert_eq!(
        render_collapsed(FileViewMode::Tree, &["src/ui/view"]),
        [
            "docs/deep/only/",
            "  file.md",
            "src/",
            "  ui/view/",
            "  main.rs",
            "README.md",
        ]
    );
}

#[test]
fn directory_rows_carry_their_path_and_state() {
    let changes = changes();
    let collapsed: HashSet<String> = ["src".to_owned()].into();
    let rows = file_rows(&changes, FileViewMode::Tree, &collapsed);
    let dirs: Vec<(&str, bool)> = rows
        .iter()
        .filter(|r| r.change.is_none())
        .map(|r| (r.dir.as_str(), r.collapsed))
        .collect();
    assert_eq!(dirs, [("docs/deep/only", false), ("src", true)]);
}

#[test]
fn flat_mode_ignores_collapsed_directories() {
    assert_eq!(
        render_collapsed(FileViewMode::Flat, &["src"]).len(),
        changes().len()
    );
}
