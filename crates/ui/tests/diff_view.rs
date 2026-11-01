// Highlights are lists of ranges, not ranges to expand.
#![allow(clippy::single_range_in_vec_init)]

use domain::{CommitId, DiffHunk, DiffLine, DiffRow, DiffSource, RowKind, SideBySide};
use ui::view::diff::{
    CONTEXT_ROWS, DIFF_ROW_HEIGHT, Direction, Jump, Step, at_end_hint, content_width,
    display_width, format_size, jump, rows_on_screen, scroll_top_for, segments, side_class,
    side_labels, status, step, window_title,
};

fn hunks(ranges: &[std::ops::Range<usize>]) -> Vec<DiffHunk> {
    ranges
        .iter()
        .map(|r| DiffHunk { rows: r.clone() })
        .collect()
}

fn at(index: usize) -> Option<Step> {
    Some(Step {
        index,
        wrapped: false,
    })
}

fn wrapped(index: usize) -> Option<Step> {
    Some(Step {
        index,
        wrapped: true,
    })
}

#[test]
fn next_and_previous_move_from_the_current_change_while_it_is_on_screen() {
    let h = hunks(&[2..3, 10..12, 40..41]);
    assert_eq!(step(&h, Some(1), 5..30, Direction::Next), at(2));
    assert_eq!(step(&h, Some(1), 5..30, Direction::Previous), at(0));
}

#[test]
fn navigation_wraps_around() {
    let h = hunks(&[2..3, 10..12, 40..41]);
    assert_eq!(step(&h, Some(2), 30..60, Direction::Next), wrapped(0));
    assert_eq!(step(&h, Some(0), 0..30, Direction::Previous), wrapped(2));
}

#[test]
fn after_scrolling_away_navigation_starts_from_the_screen() {
    let h = hunks(&[2..3, 10..12, 40..41]);
    // Change 0 is current but the user scrolled down to rows 20..35.
    assert_eq!(step(&h, Some(0), 20..35, Direction::Next), at(2));
    assert_eq!(step(&h, Some(0), 20..35, Direction::Previous), at(1));
    // Nothing below the screen: wrap to the first change.
    assert_eq!(step(&h, None, 45..60, Direction::Next), wrapped(0));
    // Nothing above the screen: wrap to the last change.
    assert_eq!(step(&h, None, 0..5, Direction::Previous), wrapped(2));
}

#[test]
fn a_change_at_the_top_of_the_screen_is_the_next_one() {
    let h = hunks(&[2..3, 10..12]);
    assert_eq!(step(&h, None, 10..30, Direction::Next), at(1));
}

#[test]
fn no_changes_means_no_step() {
    assert_eq!(step(&[], None, 0..10, Direction::Next), None);
}

#[test]
fn passing_the_last_change_first_stops_and_then_wraps() {
    let h = hunks(&[2..3, 10..12, 40..41]);
    let first = jump(&h, Some(2), 30..60, Direction::Next, None);
    assert_eq!(first, Some(Jump::AtEnd(Direction::Next)));
    let second = jump(&h, Some(2), 30..60, Direction::Next, Some(Direction::Next));
    assert_eq!(second, wrapped(0).map(Jump::To));
}

#[test]
fn passing_the_first_change_first_stops_and_then_wraps() {
    let h = hunks(&[2..3, 10..12]);
    let first = jump(&h, Some(0), 0..30, Direction::Previous, None);
    assert_eq!(first, Some(Jump::AtEnd(Direction::Previous)));
    let second = jump(
        &h,
        Some(0),
        0..30,
        Direction::Previous,
        Some(Direction::Previous),
    );
    assert_eq!(second, wrapped(1).map(Jump::To));
}

#[test]
fn stopping_at_one_end_does_not_arm_the_other_direction() {
    let h = hunks(&[2..3, 10..12]);
    let back = jump(
        &h,
        Some(1),
        0..30,
        Direction::Previous,
        Some(Direction::Next),
    );
    assert_eq!(back, at(0).map(Jump::To));
    let at_start = jump(
        &h,
        Some(0),
        0..30,
        Direction::Previous,
        Some(Direction::Next),
    );
    assert_eq!(at_start, Some(Jump::AtEnd(Direction::Previous)));
}

#[test]
fn jumps_within_the_file_need_one_press() {
    let h = hunks(&[2..3, 10..12]);
    assert_eq!(
        jump(&h, Some(0), 0..30, Direction::Next, None),
        at(1).map(Jump::To)
    );
}

#[test]
fn at_end_hints_name_the_key_to_press_again() {
    assert!(at_end_hint(Direction::Next).contains("F7 again"));
    assert!(at_end_hint(Direction::Previous).contains("F6 again"));
}

#[test]
fn status_texts() {
    assert_eq!(status(0, None, Direction::Next), "No changes");
    assert_eq!(status(1, None, Direction::Next), "1 change");
    assert_eq!(status(4, None, Direction::Next), "4 changes");
    assert_eq!(status(4, at(1), Direction::Next), "Change 2 of 4");
    assert_eq!(
        status(4, wrapped(0), Direction::Next),
        "Change 1 of 4 — wrapped to the first change"
    );
    assert_eq!(
        status(4, wrapped(3), Direction::Previous),
        "Change 4 of 4 — wrapped to the last change"
    );
}

#[test]
fn scrolling_to_a_change_keeps_context_above_it() {
    let hunk = DiffHunk { rows: 10..12 };
    assert_eq!(
        scroll_top_for(&hunk),
        (10 - CONTEXT_ROWS) as f64 * DIFF_ROW_HEIGHT
    );
    assert_eq!(scroll_top_for(&DiffHunk { rows: 1..2 }), 0.0);
}

#[test]
fn rows_on_screen_cover_partial_rows_and_clamp() {
    assert_eq!(rows_on_screen(0.0, 100.0, 1000), 0..5);
    assert_eq!(rows_on_screen(30.0, 100.0, 1000), 1..7);
    assert_eq!(rows_on_screen(0.0, 100.0, 3), 0..3);
}

#[test]
fn segments_split_a_line_at_its_highlights() {
    assert_eq!(
        segments("let y = 1;", &[4..5]),
        [("let ", false), ("y", true), (" = 1;", false)]
    );
    assert_eq!(segments("abc", &[]), [("abc", false)]);
    assert_eq!(segments("abc", &[0..3]), [("abc", true)]);
}

#[test]
fn segments_ignore_invalid_ranges() {
    // "ä" is two bytes; 1..2 is not on a character boundary.
    assert_eq!(segments("äb", &[1..2]), [("äb", false)]);
    assert_eq!(segments("ab", &[1..9]), [("ab", false)]);
}

#[test]
fn display_width_expands_tabs_to_the_next_stop() {
    assert_eq!(display_width("abc"), 3);
    assert_eq!(display_width("\tx"), 5);
    assert_eq!(display_width("ab\tx"), 5);
    assert_eq!(display_width("äöü"), 3);
}

#[test]
fn content_width_is_the_widest_line_of_both_sides() {
    let line = |text: &str| {
        Some(DiffLine {
            number: 1,
            text: text.into(),
            highlights: vec![],
        })
    };
    let diff = SideBySide {
        rows: vec![
            DiffRow {
                kind: RowKind::Removed,
                old: line("short"),
                new: None,
            },
            DiffRow {
                kind: RowKind::Added,
                old: None,
                new: line("a much longer line"),
            },
        ],
        hunks: vec![],
    };
    assert_eq!(content_width(&diff), 18);
    assert_eq!(content_width(&SideBySide::default()), 0);
}

#[test]
fn side_classes_colour_each_side() {
    assert_eq!(side_class(RowKind::Context, true), "");
    assert_eq!(side_class(RowKind::Removed, true), "del");
    assert_eq!(side_class(RowKind::Removed, false), "filler");
    assert_eq!(side_class(RowKind::Added, true), "filler");
    assert_eq!(side_class(RowKind::Added, false), "add");
    assert_eq!(side_class(RowKind::Changed, true), "del chg");
    assert_eq!(side_class(RowKind::Changed, false), "add chg");
}

#[test]
fn labels_and_titles_name_the_revisions() {
    let id = CommitId::from_bytes([0xab; 20]);
    let parent = CommitId::from_bytes([0x12; 20]);
    let commit = DiffSource::Commit {
        id,
        parent: Some(parent),
    };
    assert_eq!(
        side_labels(commit),
        ("1212121".to_owned(), "abababa".to_owned())
    );
    assert_eq!(
        side_labels(DiffSource::Commit { id, parent: None }).0,
        "(no parent)"
    );
    assert_eq!(
        side_labels(DiffSource::Staged),
        ("HEAD".to_owned(), "Index".to_owned())
    );
    assert_eq!(
        side_labels(DiffSource::Unstaged),
        ("Index".to_owned(), "Working tree".to_owned())
    );
    assert_eq!(
        window_title("src/main.rs", commit),
        "main.rs @ abababa — MyGitTool"
    );
    assert_eq!(
        window_title("README.md", DiffSource::Unstaged),
        "README.md (working tree) — MyGitTool"
    );
    assert_eq!(
        window_title("README.md", DiffSource::Staged),
        "README.md (staged) — MyGitTool"
    );
}

#[test]
fn sizes_are_formatted_for_people() {
    assert_eq!(format_size(512), "512 bytes");
    assert_eq!(format_size(2048), "2.0 KB");
    assert_eq!(format_size(13 * 1024 * 1024 + 400 * 1024), "13.4 MB");
}
