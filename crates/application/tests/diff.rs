// Expected hunks and highlights are lists of ranges, not ranges to expand.
#![allow(clippy::single_range_in_vec_init)]

use application::{is_binary, side_by_side};
use domain::{DiffRow, RowKind, SideBySide};

/// A line as (number, text).
type Line<'a> = Option<(usize, &'a str)>;

/// Each row as (kind, old line, new line).
fn rows(diff: &SideBySide) -> Vec<(RowKind, Line<'_>, Line<'_>)> {
    diff.rows
        .iter()
        .map(|DiffRow { kind, old, new }| {
            (
                *kind,
                old.as_ref().map(|l| (l.number, l.text.as_str())),
                new.as_ref().map(|l| (l.number, l.text.as_str())),
            )
        })
        .collect()
}

fn hunks(diff: &SideBySide) -> Vec<std::ops::Range<usize>> {
    diff.hunks.iter().map(|h| h.rows.clone()).collect()
}

#[test]
fn equal_texts_have_only_context_rows() {
    let diff = side_by_side("a\nb\n", "a\nb\n");
    assert_eq!(
        rows(&diff),
        [
            (RowKind::Context, Some((1, "a")), Some((1, "a"))),
            (RowKind::Context, Some((2, "b")), Some((2, "b"))),
        ]
    );
    assert!(diff.hunks.is_empty());
}

#[test]
fn added_lines_leave_the_old_side_empty() {
    let diff = side_by_side("a\nd\n", "a\nb\nc\nd\n");
    assert_eq!(
        rows(&diff),
        [
            (RowKind::Context, Some((1, "a")), Some((1, "a"))),
            (RowKind::Added, None, Some((2, "b"))),
            (RowKind::Added, None, Some((3, "c"))),
            (RowKind::Context, Some((2, "d")), Some((4, "d"))),
        ]
    );
    assert_eq!(hunks(&diff), [1..3]);
}

#[test]
fn removed_lines_leave_the_new_side_empty() {
    let diff = side_by_side("a\nb\nc\n", "a\nc\n");
    assert_eq!(
        rows(&diff),
        [
            (RowKind::Context, Some((1, "a")), Some((1, "a"))),
            (RowKind::Removed, Some((2, "b")), None),
            (RowKind::Context, Some((3, "c")), Some((2, "c"))),
        ]
    );
}

#[test]
fn replaced_lines_are_paired_and_the_surplus_stands_alone() {
    let diff = side_by_side("a\nold 1\nold 2\nz\n", "a\nnew 1\nz\n");
    assert_eq!(
        rows(&diff),
        [
            (RowKind::Context, Some((1, "a")), Some((1, "a"))),
            (RowKind::Changed, Some((2, "old 1")), Some((2, "new 1"))),
            (RowKind::Removed, Some((3, "old 2")), None),
            (RowKind::Context, Some((4, "z")), Some((3, "z"))),
        ]
    );
    assert_eq!(hunks(&diff), [1..3]);
}

#[test]
fn changed_rows_highlight_the_changed_words() {
    let diff = side_by_side("let x = 1;\n", "let y = 1;\n");
    let row = &diff.rows[0];
    assert_eq!(row.kind, RowKind::Changed);
    let old = row.old.as_ref().unwrap();
    let new = row.new.as_ref().unwrap();
    assert_eq!(old.highlights, [4..5]);
    assert_eq!(new.highlights, [4..5]);
    assert_eq!(&new.text[new.highlights[0].clone()], "y");
}

#[test]
fn changed_words_separated_by_spaces_form_one_highlight() {
    let diff = side_by_side("line: some text here\n", "line: work tree edit\n");
    let new = diff.rows[0].new.as_ref().unwrap();
    assert_eq!(new.highlights, [6..20]);
}

#[test]
fn context_rows_have_no_highlights() {
    let diff = side_by_side("a\nb\n", "a\nc\n");
    assert!(diff.rows[0].old.as_ref().unwrap().highlights.is_empty());
    assert!(diff.rows[0].new.as_ref().unwrap().highlights.is_empty());
}

#[test]
fn separate_changes_form_separate_hunks() {
    let diff = side_by_side("1\n2\n3\n4\n5\n", "1\nX\n3\n4\nY\n");
    assert_eq!(hunks(&diff), [1..2, 4..5]);
}

#[test]
fn a_change_at_the_end_closes_the_last_hunk() {
    let diff = side_by_side("a\n", "a\nb");
    assert_eq!(hunks(&diff), [1..2]);
}

#[test]
fn line_endings_do_not_count_as_changes() {
    let diff = side_by_side("a\r\nb\r\n", "a\nb\n");
    assert!(diff.hunks.is_empty());
    assert_eq!(diff.rows[0].old.as_ref().unwrap().text, "a");
}

#[test]
fn empty_old_side_makes_every_line_added() {
    let diff = side_by_side("", "a\nb\n");
    assert!(diff.rows.iter().all(|r| r.kind == RowKind::Added));
    assert_eq!(hunks(&diff), [0..2]);
}

#[test]
fn binary_detection_looks_for_nul_bytes() {
    assert!(is_binary(b"PNG\0\x01"));
    assert!(!is_binary("plain text, ümlauts".as_bytes()));
    assert!(!is_binary(b""));
}
