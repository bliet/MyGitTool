//! The logic behind the diff window: change navigation, status text and
//! the layout of the side-by-side rows.

use domain::{DiffHunk, DiffSource, RowKind, SideBySide};
use std::ops::Range;
use std::path::PathBuf;

/// Row height in pixels; all rows have the same height.
pub const DIFF_ROW_HEIGHT: f64 = 20.0;
/// Context rows kept above a change that is scrolled to.
pub const CONTEXT_ROWS: usize = 3;
/// Columns a tab advances to the next multiple of.
pub const TAB_WIDTH: usize = 4;

/// Identifies a diff window, so each file opens at most one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DiffKey {
    pub repo: PathBuf,
    pub source: DiffSource,
    pub path: String,
}

/// The result of jumping to the next or previous change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub index: usize,
    /// The jump went past the last (or first) change and started over.
    pub wrapped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Next,
    Previous,
}

/// The change to jump to. It counts from `current` while that change is
/// still on screen, and otherwise from the first row on screen, so a jump
/// after scrolling by hand continues where the user is looking.
pub fn step(
    hunks: &[DiffHunk],
    current: Option<usize>,
    visible: Range<usize>,
    direction: Direction,
) -> Option<Step> {
    let count = hunks.len();
    if count == 0 {
        return None;
    }
    let anchored = current
        .filter(|&i| i < count)
        .filter(|&i| overlaps(&hunks[i].rows, &visible));
    let target = match (direction, anchored) {
        (Direction::Next, Some(i)) => i.checked_add(1).filter(|&n| n < count),
        (Direction::Previous, Some(i)) => i.checked_sub(1),
        (Direction::Next, None) => hunks.iter().position(|h| h.rows.start >= visible.start),
        (Direction::Previous, None) => hunks.iter().rposition(|h| h.rows.start < visible.start),
    };
    Some(match (target, direction) {
        (Some(index), _) => Step {
            index,
            wrapped: false,
        },
        (None, Direction::Next) => Step {
            index: 0,
            wrapped: true,
        },
        (None, Direction::Previous) => Step {
            index: count - 1,
            wrapped: true,
        },
    })
}

/// What pressing F7 or F6 does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    To(Step),
    /// There is no further change in this direction. Nothing moves; the
    /// next press in the same direction wraps around.
    AtEnd(Direction),
}

/// Like [`step`], but a wrap needs two presses: the first one only reports
/// [`Jump::AtEnd`], and the wrap happens if the press before was that same
/// report (`at_end`).
pub fn jump(
    hunks: &[DiffHunk],
    current: Option<usize>,
    visible: Range<usize>,
    direction: Direction,
    at_end: Option<Direction>,
) -> Option<Jump> {
    let next = step(hunks, current, visible, direction)?;
    Some(if next.wrapped && at_end != Some(direction) {
        Jump::AtEnd(direction)
    } else {
        Jump::To(next)
    })
}

/// The hint shown after [`Jump::AtEnd`].
pub fn at_end_hint(direction: Direction) -> &'static str {
    match direction {
        Direction::Next => {
            "No more changes in this file — press F7 again to go to the first change"
        }
        Direction::Previous => {
            "No earlier changes in this file — press F6 again to go to the last change"
        }
    }
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

/// The status-bar text after a jump, or before any jump with `step` `None`.
pub fn status(count: usize, step: Option<Step>, direction: Direction) -> String {
    match (count, step) {
        (0, _) => "No changes".to_owned(),
        (1, None) => "1 change".to_owned(),
        (n, None) => format!("{n} changes"),
        (n, Some(Step { index, wrapped })) => {
            let position = format!("Change {} of {n}", index + 1);
            match (wrapped, direction) {
                (false, _) => position,
                (true, Direction::Next) => format!("{position} — wrapped to the first change"),
                (true, Direction::Previous) => format!("{position} — wrapped to the last change"),
            }
        }
    }
}

/// The rows fully or partly on screen, without any overscan.
pub fn rows_on_screen(scroll_top: f64, viewport_height: f64, total: usize) -> Range<usize> {
    // Float-to-int casts saturate; negative values become 0.
    let first = (scroll_top / DIFF_ROW_HEIGHT).floor().max(0.0) as usize;
    let end = ((scroll_top + viewport_height) / DIFF_ROW_HEIGHT)
        .ceil()
        .max(0.0) as usize;
    first.min(total)..end.min(total)
}

/// The scroll position that shows a change with some context above it.
pub fn scroll_top_for(hunk: &DiffHunk) -> f64 {
    hunk.rows.start.saturating_sub(CONTEXT_ROWS) as f64 * DIFF_ROW_HEIGHT
}

/// Splits a line into runs of plain and highlighted text. Ranges that are
/// out of bounds or not on character boundaries are ignored.
pub fn segments<'a>(text: &'a str, highlights: &[Range<usize>]) -> Vec<(&'a str, bool)> {
    let mut segments = Vec::new();
    let mut pos = 0;
    for range in highlights {
        let (Some(_), Some(marked)) = (text.get(pos..range.start), text.get(range.clone())) else {
            continue;
        };
        if range.start > pos {
            segments.push((&text[pos..range.start], false));
        }
        if !marked.is_empty() {
            segments.push((marked, true));
        }
        pos = range.end;
    }
    if pos < text.len() {
        segments.push((&text[pos..], false));
    }
    segments
}

/// The width of a line in character cells, with tabs expanded.
pub fn display_width(text: &str) -> usize {
    text.chars().fold(0, |width, c| match c {
        '\t' => (width / TAB_WIDTH + 1) * TAB_WIDTH,
        _ => width + 1,
    })
}

/// The widest line on either side, so both sides can share one width and
/// scroll horizontally in step.
pub fn content_width(diff: &SideBySide) -> usize {
    diff.rows
        .iter()
        .flat_map(|row| [&row.old, &row.new])
        .flatten()
        .map(|line| display_width(&line.text))
        .max()
        .unwrap_or(0)
}

/// The CSS classes of one side of a row: `old` is the left side.
pub fn side_class(kind: RowKind, old: bool) -> &'static str {
    match (kind, old) {
        (RowKind::Context, _) => "",
        (RowKind::Removed, true) => "del",
        (RowKind::Added, false) => "add",
        (RowKind::Removed, false) | (RowKind::Added, true) => "filler",
        (RowKind::Changed, true) => "del chg",
        (RowKind::Changed, false) => "add chg",
    }
}

/// What the two sides show: the old and the new revision.
pub fn side_labels(source: DiffSource) -> (String, String) {
    match source {
        DiffSource::Commit { id, parent } => (
            parent.map_or_else(|| "(no parent)".to_owned(), |p| p.short()),
            id.short(),
        ),
        DiffSource::WorkTree => ("HEAD".to_owned(), "Working tree".to_owned()),
    }
}

/// The diff window's title, e.g. `main.rs @ 1a2b3c4 — MyGitTool`.
pub fn window_title(path: &str, source: DiffSource) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match source {
        DiffSource::Commit { id, .. } => format!("{name} @ {} — MyGitTool", id.short()),
        DiffSource::WorkTree => format!("{name} (working tree) — MyGitTool"),
    }
}

/// A byte count for people, e.g. `12.4 MB`.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} bytes");
    }
    // Precision loss is irrelevant for one decimal place.
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}
