use application::merge3;
use domain::{ConflictHunk, MergeChunk};

fn clean(text: &str) -> MergeChunk {
    MergeChunk::Clean(text.into())
}

fn conflict(base: &str, ours: &str, theirs: &str) -> MergeChunk {
    MergeChunk::Conflict(ConflictHunk {
        base: base.into(),
        ours: ours.into(),
        theirs: theirs.into(),
    })
}

const BASE: &str = "a\nb\nc\nd\ne\n";

#[test]
fn changes_on_different_lines_merge_cleanly() {
    let ours = "A\nb\nc\nd\ne\n";
    let theirs = "a\nb\nc\nd\nE\n";
    assert_eq!(merge3(BASE, ours, theirs), [clean("A\nb\nc\nd\nE\n")]);
}

#[test]
fn different_changes_to_the_same_line_conflict() {
    let ours = "a\nb\nOURS\nd\ne\n";
    let theirs = "a\nb\nTHEIRS\nd\ne\n";
    assert_eq!(
        merge3(BASE, ours, theirs),
        [
            clean("a\nb\n"),
            conflict("c\n", "OURS\n", "THEIRS\n"),
            clean("d\ne\n"),
        ]
    );
}

#[test]
fn the_same_change_on_both_sides_is_clean() {
    let both = "a\nb\nX\nd\ne\n";
    assert_eq!(merge3(BASE, both, both), [clean(both)]);
}

#[test]
fn changes_to_adjacent_lines_conflict_as_in_git() {
    let ours = "a\nB\nc\nd\ne\n";
    let theirs = "a\nb\nC\nd\ne\n";
    assert_eq!(
        merge3(BASE, ours, theirs),
        [
            clean("a\n"),
            conflict("b\nc\n", "B\nc\n", "b\nC\n"),
            clean("d\ne\n"),
        ]
    );
}

#[test]
fn insertions_and_deletions_keep_the_sides_aligned() {
    // Ours inserts two lines at the top; theirs deletes `c` and edits `e`.
    let ours = "x\ny\na\nb\nc\nd\ne\n";
    let theirs = "a\nb\nd\nE\n";
    assert_eq!(merge3(BASE, ours, theirs), [clean("x\ny\na\nb\nd\nE\n")]);
}

#[test]
fn insertions_at_the_same_place_conflict() {
    let ours = "a\nb\nc\nd\ne\nours\n";
    let theirs = "a\nb\nc\nd\ne\ntheirs\n";
    assert_eq!(
        merge3(BASE, ours, theirs),
        [clean(BASE), conflict("", "ours\n", "theirs\n")]
    );
}

#[test]
fn line_endings_and_a_missing_final_newline_are_kept() {
    let base = "a\r\nb\r\nc\r\nd";
    let ours = "A\r\nb\r\nc\r\nd";
    let theirs = "a\r\nb\r\nc\r\nD";
    assert_eq!(merge3(base, ours, theirs), [clean("A\r\nb\r\nc\r\nD")]);
}

#[test]
fn a_file_added_on_both_sides_conflicts_as_a_whole() {
    assert_eq!(
        merge3("", "one\n", "two\n"),
        [conflict("", "one\n", "two\n")]
    );
}
