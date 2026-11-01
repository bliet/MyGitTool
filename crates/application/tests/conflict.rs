mod common;

use application::{LoadConflict, OperationError, ResolveConflict};
use common::{FakeReader, FakeWriter};
use domain::{
    ConflictContent, ConflictSide, ConflictSides, FileContent, MergeChunk, RepositoryInfo, SideKind,
};
use std::sync::Arc;

fn repo() -> RepositoryInfo {
    RepositoryInfo::new("/repo")
}

fn text(content: &str) -> Option<FileContent> {
    Some(FileContent::Bytes(content.as_bytes().to_vec()))
}

fn reader_with(path: &str, sides: ConflictSides) -> FakeReader {
    let mut reader = FakeReader {
        conflicts: vec![path.to_owned()],
        ..FakeReader::default()
    };
    reader.conflict_sides.insert(path.to_owned(), sides);
    reader
}

fn edited_both_ways() -> FakeReader {
    reader_with(
        "a.txt",
        ConflictSides {
            base: text("top\nbase\n"),
            ours: text("top\nours\n"),
            theirs: text("top\ntheirs\n"),
        },
    )
}

#[test]
fn text_versions_are_merged_line_by_line() {
    let conflict = LoadConflict::new(Arc::new(edited_both_ways()))
        .load(&repo(), "a.txt")
        .unwrap();
    let ConflictContent::Text(chunks) = conflict.content else {
        panic!("expected a text conflict");
    };
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0], MergeChunk::Clean("top\n".into()));
    assert!(matches!(&chunks[1], MergeChunk::Conflict(h) if h.ours == "ours\n"));
}

#[test]
fn a_file_deleted_on_one_side_is_a_whole_file_choice() {
    let reader = reader_with(
        "a.txt",
        ConflictSides {
            base: text("base\n"),
            ours: text("edited\n"),
            theirs: None,
        },
    );
    let conflict = LoadConflict::new(Arc::new(reader))
        .load(&repo(), "a.txt")
        .unwrap();
    assert_eq!(
        conflict.content,
        ConflictContent::WholeFile {
            ours: SideKind::Text,
            theirs: SideKind::Deleted
        }
    );
}

#[test]
fn binary_and_large_files_are_whole_file_choices() {
    let reader = reader_with(
        "img.bin",
        ConflictSides {
            base: Some(FileContent::Bytes(b"a\0".to_vec())),
            ours: Some(FileContent::Bytes(b"b\0".to_vec())),
            theirs: text("0123456789"),
        },
    );
    let conflict = LoadConflict::new(Arc::new(reader))
        .with_size_limit(5)
        .load(&repo(), "img.bin")
        .unwrap();
    assert_eq!(
        conflict.content,
        ConflictContent::WholeFile {
            ours: SideKind::Binary,
            theirs: SideKind::TooLarge
        }
    );
}

#[test]
fn a_file_without_conflict_is_reported() {
    let result = LoadConflict::new(Arc::new(FakeReader::default())).load(&repo(), "a.txt");
    assert_eq!(result, Err(OperationError::NotConflicted("a.txt".into())));
}

#[test]
fn resolving_writes_and_stages_the_merged_text() {
    let writer = Arc::new(FakeWriter::default());
    ResolveConflict::new(Arc::new(edited_both_ways()), writer.clone())
        .resolve_text(&repo(), "a.txt", "top\nmerged\n")
        .unwrap();
    assert_eq!(writer.calls(), [r#"resolve a.txt "top\nmerged\n""#]);
}

#[test]
fn taking_a_side_uses_its_whole_content_or_deletes() {
    let reader = Arc::new(reader_with(
        "a.txt",
        ConflictSides {
            base: text("base\n"),
            ours: text("edited\n"),
            theirs: None,
        },
    ));
    let writer = Arc::new(FakeWriter::default());
    let resolve = ResolveConflict::new(reader, writer.clone());
    resolve
        .take_side(&repo(), "a.txt", ConflictSide::Ours)
        .unwrap();
    resolve
        .take_side(&repo(), "a.txt", ConflictSide::Theirs)
        .unwrap();
    assert_eq!(
        writer.calls(),
        [r#"resolve a.txt "edited\n""#, "resolve a.txt deleted"]
    );
}

#[test]
fn keeping_the_work_tree_file_stages_it() {
    let writer = Arc::new(FakeWriter::default());
    ResolveConflict::new(Arc::new(edited_both_ways()), writer.clone())
        .keep_work_tree(&repo(), "a.txt")
        .unwrap();
    assert_eq!(writer.calls(), ["stage a.txt"]);
}

#[test]
fn a_file_resolved_meanwhile_is_not_overwritten() {
    let writer = Arc::new(FakeWriter::default());
    let result = ResolveConflict::new(Arc::new(FakeReader::default()), writer.clone())
        .resolve_text(&repo(), "a.txt", "text");
    assert_eq!(result, Err(OperationError::NotConflicted("a.txt".into())));
    assert!(writer.calls().is_empty());
}
