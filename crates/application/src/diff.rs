//! Line diffs aligned for a side-by-side view.

use domain::{DiffHunk, DiffLine, DiffRow, RowKind, SideBySide};
use similar::{Algorithm, ChangeTag, DiffTag, TextDiff, capture_diff_slices_deadline};
use std::ops::Range;
use std::time::{Duration, Instant};

/// How long diffing may take before `similar` settles for a coarser result.
const TIMEOUT: Duration = Duration::from_secs(2);
/// Lines longer than this many bytes get no highlights within the line.
const MAX_INLINE_LEN: usize = 2_000;
/// Git's heuristic: a NUL byte within this many leading bytes marks a file
/// as binary.
const BINARY_PROBE_LEN: usize = 8_000;

/// Whether content is binary rather than text, the way Git decides it.
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_PROBE_LEN)].contains(&0)
}

/// Diffs two texts line by line and aligns the result in rows. A block of
/// removed lines followed by a block of added lines is paired up into
/// `Changed` rows, with the changed words highlighted; the surplus lines of
/// the longer block become `Removed` or `Added` rows.
pub fn side_by_side(old: &str, new: &str) -> SideBySide {
    let deadline = Instant::now().checked_add(TIMEOUT);
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let ops = capture_diff_slices_deadline(Algorithm::Patience, &old_lines, &new_lines, deadline);

    let mut rows = Vec::with_capacity(old_lines.len().max(new_lines.len()));
    let line = |lines: &[&str], index: usize| DiffLine {
        number: index + 1,
        text: lines[index].to_owned(),
        highlights: Vec::new(),
    };
    for op in ops {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        match tag {
            DiffTag::Equal => {
                rows.extend(old_range.zip(new_range).map(|(o, n)| DiffRow {
                    kind: RowKind::Context,
                    old: Some(line(&old_lines, o)),
                    new: Some(line(&new_lines, n)),
                }));
            }
            DiffTag::Delete | DiffTag::Insert | DiffTag::Replace => {
                let paired = old_range.len().min(new_range.len());
                for (o, n) in old_range.clone().zip(new_range.clone()) {
                    let mut old_line = line(&old_lines, o);
                    let mut new_line = line(&new_lines, n);
                    (old_line.highlights, new_line.highlights) =
                        highlights(&old_line.text, &new_line.text, deadline);
                    rows.push(DiffRow {
                        kind: RowKind::Changed,
                        old: Some(old_line),
                        new: Some(new_line),
                    });
                }
                rows.extend(old_range.skip(paired).map(|o| DiffRow {
                    kind: RowKind::Removed,
                    old: Some(line(&old_lines, o)),
                    new: None,
                }));
                rows.extend(new_range.skip(paired).map(|n| DiffRow {
                    kind: RowKind::Added,
                    old: None,
                    new: Some(line(&new_lines, n)),
                }));
            }
        }
    }
    let hunks = hunks(&rows);
    SideBySide { rows, hunks }
}

/// The changed byte ranges of two versions of a line, by words.
fn highlights(
    old: &str,
    new: &str,
    deadline: Option<Instant>,
) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let mut old_ranges = Vec::new();
    let mut new_ranges = Vec::new();
    if old.len() > MAX_INLINE_LEN || new.len() > MAX_INLINE_LEN {
        return (old_ranges, new_ranges);
    }
    let mut config = TextDiff::configure();
    if let Some(deadline) = deadline {
        config.deadline(deadline);
    }
    let diff = config.diff_words(old, new);
    let (mut old_pos, mut new_pos) = (0, 0);
    for change in diff.iter_all_changes() {
        let len = change.value().len();
        match change.tag() {
            ChangeTag::Equal => {
                old_pos += len;
                new_pos += len;
            }
            ChangeTag::Delete => {
                extend(&mut old_ranges, old_pos..old_pos + len);
                old_pos += len;
            }
            ChangeTag::Insert => {
                extend(&mut new_ranges, new_pos..new_pos + len);
                new_pos += len;
            }
        }
    }
    (
        join_across_spaces(old, old_ranges),
        join_across_spaces(new, new_ranges),
    )
}

/// Appends `range`, merging it into the last range if they touch.
fn extend(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    match ranges.last_mut() {
        Some(last) if last.end == range.start => last.end = range.end,
        _ => ranges.push(range),
    }
}

/// Merges ranges that only whitespace separates, so a run of changed words
/// reads as one highlight.
fn join_across_spaces(text: &str, ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut joined: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match joined.last_mut() {
            Some(last)
                if text
                    .get(last.end..range.start)
                    .is_some_and(|gap| gap.chars().all(char::is_whitespace)) =>
            {
                last.end = range.end;
            }
            _ => joined.push(range),
        }
    }
    joined
}

/// Groups consecutive non-context rows.
fn hunks(rows: &[DiffRow]) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut start = None;
    for (index, row) in rows.iter().enumerate() {
        match (row.kind == RowKind::Context, start) {
            (false, None) => start = Some(index),
            (true, Some(first)) => {
                hunks.push(DiffHunk { rows: first..index });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(first) = start {
        hunks.push(DiffHunk {
            rows: first..rows.len(),
        });
    }
    hunks
}
