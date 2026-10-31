//! The changed-files list of the details column, as a tree or flat list.

pub use application::FileViewMode;
use domain::FileChange;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    pub depth: usize,
    /// A directory name (possibly `a/b` for a compressed chain), a file name
    /// in tree mode, or the full path in flat mode.
    pub name: String,
    /// The index into the changes for a file; `None` for a directory.
    pub change: Option<usize>,
}

pub fn file_rows(changes: &[FileChange], mode: FileViewMode) -> Vec<FileRow> {
    match mode {
        FileViewMode::Flat => {
            let mut order: Vec<usize> = (0..changes.len()).collect();
            order.sort_by(|&a, &b| changes[a].path.cmp(&changes[b].path));
            order
                .into_iter()
                .map(|i| FileRow {
                    depth: 0,
                    name: changes[i].path.clone(),
                    change: Some(i),
                })
                .collect()
        }
        FileViewMode::Tree => {
            let mut root = Dir::default();
            for (i, change) in changes.iter().enumerate() {
                let mut dir = &mut root;
                let mut segments = change.path.split('/').peekable();
                while let Some(segment) = segments.next() {
                    if segments.peek().is_none() {
                        dir.files.insert(segment.to_owned(), i);
                    } else {
                        dir = dir.dirs.entry(segment.to_owned()).or_default();
                    }
                }
            }
            let mut rows = Vec::new();
            push_dir(&mut rows, &root, 0);
            rows
        }
    }
}

#[derive(Default)]
struct Dir {
    dirs: BTreeMap<String, Dir>,
    files: BTreeMap<String, usize>,
}

fn push_dir(rows: &mut Vec<FileRow>, dir: &Dir, depth: usize) {
    for (name, child) in &dir.dirs {
        // Compress chains of directories that only contain one directory.
        let mut name = name.clone();
        let mut child = child;
        while child.files.is_empty() && child.dirs.len() == 1 {
            let Some((sub, next)) = child.dirs.iter().next() else {
                break;
            };
            name = format!("{name}/{sub}");
            child = next;
        }
        rows.push(FileRow {
            depth,
            name,
            change: None,
        });
        push_dir(rows, child, depth + 1);
    }
    for (name, &index) in &dir.files {
        rows.push(FileRow {
            depth,
            name: name.clone(),
            change: Some(index),
        });
    }
}
