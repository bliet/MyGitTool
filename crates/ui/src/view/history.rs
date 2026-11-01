//! The loaded part of the history list and its virtualization.

use application::HistoryRow;
use domain::CommitId;
use std::collections::HashMap;
use std::ops::Range;

/// Row height in pixels. All rows have the same height, which keeps
/// virtualization a matter of arithmetic.
pub const ROW_HEIGHT: f64 = 26.0;
/// Height of the column header, which stays at the top of the scrolled list.
/// The rows start below it, so the viewport for them is this much shorter.
pub const HEADER_HEIGHT: f64 = 24.0;
/// Rows rendered above and below the viewport.
pub const OVERSCAN: usize = 15;
/// Commits fetched per page.
pub const PAGE_SIZE: usize = 500;

/// What the details column shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Selection {
    Uncommitted,
    Commit(CommitId),
    /// A stash, chosen in the left column, by its commit.
    Stash(CommitId),
}

/// The rows rendered for a scroll position, including the overscan.
pub fn visible_range(scroll_top: f64, viewport_height: f64, total: usize) -> Range<usize> {
    visible_rows(ROW_HEIGHT, scroll_top, viewport_height, total)
}

/// Like [`visible_range`], for a list with rows of `row_height` pixels.
pub fn visible_rows(
    row_height: f64,
    scroll_top: f64,
    viewport_height: f64,
    total: usize,
) -> Range<usize> {
    // Float-to-int casts saturate; negative values become 0.
    let first = (scroll_top / row_height).floor().max(0.0) as usize;
    let last = ((scroll_top + viewport_height) / row_height)
        .ceil()
        .max(0.0) as usize;
    let start = first.saturating_sub(OVERSCAN).min(total);
    let end = last.saturating_add(OVERSCAN).min(total);
    start..end
}

/// The scroll position that shows `row`, or `None` if it is already fully
/// visible. A row outside the viewport is centred.
pub fn scroll_to_reveal(row: usize, scroll_top: f64, viewport_height: f64) -> Option<f64> {
    let top = row as f64 * ROW_HEIGHT;
    let bottom = top + ROW_HEIGHT;
    if top >= scroll_top && bottom <= scroll_top + viewport_height {
        return None;
    }
    Some((top - (viewport_height - ROW_HEIGHT) / 2.0).max(0.0))
}

/// The commits loaded so far.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct HistoryList {
    rows: Vec<HistoryRow>,
    positions: HashMap<CommitId, usize>,
    exhausted: bool,
}

impl HistoryList {
    pub fn rows(&self) -> &[HistoryRow] {
        &self.rows
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    pub fn append(&mut self, rows: Vec<HistoryRow>, exhausted: bool) {
        for row in rows {
            self.positions.insert(row.commit.id, self.rows.len());
            self.rows.push(row);
        }
        self.exhausted = exhausted;
    }

    /// The index of a loaded commit.
    pub fn position(&self, id: &CommitId) -> Option<usize> {
        self.positions.get(id).copied()
    }

    /// Whether the next page should be fetched because the rendered rows
    /// (ending at `visible_end`) come close to the end of what is loaded.
    pub fn needs_more(&self, visible_end: usize) -> bool {
        !self.exhausted && visible_end + PAGE_SIZE / 2 >= self.rows.len()
    }
}
