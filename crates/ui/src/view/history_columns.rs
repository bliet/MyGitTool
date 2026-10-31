//! Widths of the resizable columns of the history list. The message column
//! takes the space that is left.

pub const MIN_WIDTH: f64 = 40.0;
pub const MAX_WIDTH: f64 = 600.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryField {
    Author,
    Date,
    Hash,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistoryColumns {
    pub author: f64,
    pub date: f64,
    pub hash: f64,
}

impl Default for HistoryColumns {
    fn default() -> Self {
        Self {
            author: 150.0,
            date: 120.0,
            hash: 64.0,
        }
    }
}

impl HistoryColumns {
    /// Remembered widths, clamped to the allowed range.
    pub fn restored(author: f64, date: f64, hash: f64) -> Self {
        Self {
            author: author.clamp(MIN_WIDTH, MAX_WIDTH),
            date: date.clamp(MIN_WIDTH, MAX_WIDTH),
            hash: hash.clamp(MIN_WIDTH, MAX_WIDTH),
        }
    }

    pub fn width(&self, field: HistoryField) -> f64 {
        match field {
            HistoryField::Author => self.author,
            HistoryField::Date => self.date,
            HistoryField::Hash => self.hash,
        }
    }

    pub fn resize(&mut self, field: HistoryField, width: f64) {
        let width = width.clamp(MIN_WIDTH, MAX_WIDTH);
        match field {
            HistoryField::Author => self.author = width,
            HistoryField::Date => self.date = width,
            HistoryField::Hash => self.hash = width,
        }
    }

    /// The width a drag of a column's left edge gives: dragging left
    /// widens the column, as the message column gives up the space.
    pub fn dragged_width(start_width: f64, start_x: f64, x: f64) -> f64 {
        start_width - (x - start_x)
    }

    /// The CSS custom properties the rows and the header use.
    pub fn style(&self) -> String {
        format!(
            "--author-width: {}px; --date-width: {}px; --hash-width: {}px;",
            self.author, self.date, self.hash
        )
    }
}
