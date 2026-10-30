mod common;

use application::ComputeFileDiff;
use common::{FakeReader, id};
use domain::{ChangeKind, DiffBody, DiffSource, FileChange, RepositoryInfo, Revision, RowKind};
use std::sync::Arc;

fn change(path: &str, kind: ChangeKind) -> FileChange {
    FileChange {
        path: path.into(),
        old_path: None,
        kind,
        additions: 0,
        deletions: 0,
        binary: false,
    }
}

fn commit_source() -> DiffSource {
    DiffSource::Commit {
        id: id(2),
        parent: Some(id(1)),
    }
}

fn repo() -> RepositoryInfo {
    RepositoryInfo::new("/r")
}

fn text_rows(body: &DiffBody) -> Vec<RowKind> {
    match body {
        DiffBody::Text(diff) => diff.rows.iter().map(|r| r.kind).collect(),
        other => panic!("expected a text diff, got {other:?}"),
    }
}

#[test]
fn modified_file_in_a_commit_is_diffed_against_the_first_parent() {
    let reader = Arc::new(
        FakeReader::default()
            .with_file(Revision::Commit(id(1)), "a.txt", "one\n")
            .with_file(Revision::Commit(id(2)), "a.txt", "two\n"),
    );
    let diff = ComputeFileDiff::new(reader.clone())
        .compute(
            &repo(),
            commit_source(),
            &change("a.txt", ChangeKind::Modified),
        )
        .unwrap();
    assert_eq!(diff.old_path.as_deref(), Some("a.txt"));
    assert_eq!(diff.new_path.as_deref(), Some("a.txt"));
    assert_eq!(text_rows(&diff.body), [RowKind::Changed]);
}

#[test]
fn added_file_has_no_old_side_and_is_not_read_there() {
    let reader =
        Arc::new(FakeReader::default().with_file(Revision::Commit(id(2)), "new.txt", "a\nb\n"));
    let diff = ComputeFileDiff::new(reader.clone())
        .compute(
            &repo(),
            commit_source(),
            &change("new.txt", ChangeKind::Added),
        )
        .unwrap();
    assert_eq!(diff.old_path, None);
    assert_eq!(text_rows(&diff.body), [RowKind::Added, RowKind::Added]);
    assert_eq!(
        reader.reads(),
        [(Revision::Commit(id(2)), "new.txt".to_owned())]
    );
}

#[test]
fn deleted_file_has_no_new_side() {
    let reader =
        Arc::new(FakeReader::default().with_file(Revision::Commit(id(1)), "gone.txt", "x\n"));
    let diff = ComputeFileDiff::new(reader)
        .compute(
            &repo(),
            commit_source(),
            &change("gone.txt", ChangeKind::Deleted),
        )
        .unwrap();
    assert_eq!(diff.new_path, None);
    assert_eq!(text_rows(&diff.body), [RowKind::Removed]);
}

#[test]
fn renamed_file_reads_the_old_path_on_the_old_side() {
    let reader = Arc::new(
        FakeReader::default()
            .with_file(Revision::Commit(id(1)), "old.txt", "same\n")
            .with_file(Revision::Commit(id(2)), "new.txt", "same\n"),
    );
    let mut renamed = change("new.txt", ChangeKind::Renamed);
    renamed.old_path = Some("old.txt".into());
    let diff = ComputeFileDiff::new(reader)
        .compute(&repo(), commit_source(), &renamed)
        .unwrap();
    assert_eq!(diff.old_path.as_deref(), Some("old.txt"));
    assert_eq!(diff.new_path.as_deref(), Some("new.txt"));
    assert_eq!(text_rows(&diff.body), [RowKind::Context]);
}

#[test]
fn root_commit_diffs_against_nothing() {
    let reader = Arc::new(FakeReader::default().with_file(Revision::Commit(id(1)), "a.txt", "a\n"));
    let source = DiffSource::Commit {
        id: id(1),
        parent: None,
    };
    let diff = ComputeFileDiff::new(reader.clone())
        .compute(&repo(), source, &change("a.txt", ChangeKind::Added))
        .unwrap();
    assert_eq!(text_rows(&diff.body), [RowKind::Added]);
    assert_eq!(reader.reads().len(), 1);
}

#[test]
fn work_tree_diff_compares_head_with_the_work_tree() {
    let reader = Arc::new(
        FakeReader::default()
            .with_file(Revision::Head, "a.txt", "a\n")
            .with_file(Revision::WorkTree, "a.txt", "a\nb\n"),
    );
    let diff = ComputeFileDiff::new(reader)
        .compute(
            &repo(),
            DiffSource::WorkTree,
            &change("a.txt", ChangeKind::Modified),
        )
        .unwrap();
    assert_eq!(text_rows(&diff.body), [RowKind::Context, RowKind::Added]);
}

#[test]
fn binary_content_is_not_diffed() {
    let reader = Arc::new(
        FakeReader::default()
            .with_file(Revision::Commit(id(1)), "img.png", "PNG\0a")
            .with_file(Revision::Commit(id(2)), "img.png", "PNG\0b"),
    );
    let diff = ComputeFileDiff::new(reader)
        .compute(
            &repo(),
            commit_source(),
            &change("img.png", ChangeKind::Modified),
        )
        .unwrap();
    assert_eq!(diff.body, DiffBody::Binary);
}

#[test]
fn file_marked_binary_by_git_is_not_even_read() {
    let reader = Arc::new(FakeReader::default());
    let mut binary = change("img.png", ChangeKind::Modified);
    binary.binary = true;
    let diff = ComputeFileDiff::new(reader.clone())
        .compute(&repo(), commit_source(), &binary)
        .unwrap();
    assert_eq!(diff.body, DiffBody::Binary);
    assert!(reader.reads().is_empty());
}

#[test]
fn file_over_the_size_limit_is_too_large() {
    let reader = Arc::new(
        FakeReader::default()
            .with_file(Revision::Commit(id(1)), "big.txt", "small\n")
            .with_file(Revision::Commit(id(2)), "big.txt", "0123456789\n"),
    );
    let diff = ComputeFileDiff::new(reader)
        .with_size_limit(8)
        .compute(
            &repo(),
            commit_source(),
            &change("big.txt", ChangeKind::Modified),
        )
        .unwrap();
    assert_eq!(diff.body, DiffBody::TooLarge { size: 11 });
}

#[test]
fn invalid_utf8_is_shown_lossily() {
    let mut reader = FakeReader::default();
    reader.files.insert(
        (Revision::Commit(id(2)), "l1.txt".into()),
        b"caf\xe9\n".to_vec(),
    );
    let diff = ComputeFileDiff::new(Arc::new(reader))
        .compute(
            &repo(),
            commit_source(),
            &change("l1.txt", ChangeKind::Added),
        )
        .unwrap();
    let DiffBody::Text(text) = diff.body else {
        panic!("expected text");
    };
    assert_eq!(text.rows[0].new.as_ref().unwrap().text, "caf\u{fffd}");
}
