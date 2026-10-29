//! Widths of the resizable side columns.

pub const MIN_SIDE: f64 = 160.0;
pub const MIN_MIDDLE: f64 = 300.0;
/// The width of each splitter between columns.
pub const SPLITTER: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnLayout {
    pub left: f64,
    pub right: f64,
}

impl Default for ColumnLayout {
    fn default() -> Self {
        Self {
            left: 260.0,
            right: 380.0,
        }
    }
}

impl ColumnLayout {
    pub fn width(&self, side: Side) -> f64 {
        match side {
            Side::Left => self.left,
            Side::Right => self.right,
        }
    }

    /// Sets one side's width, clamped so both sides keep `MIN_SIDE` and the
    /// middle column keeps `MIN_MIDDLE` within `total` pixels.
    pub fn resize(&mut self, side: Side, width: f64, total: f64) {
        let other = match side {
            Side::Left => self.right,
            Side::Right => self.left,
        };
        let max = (total - other - MIN_MIDDLE - 2.0 * SPLITTER).max(MIN_SIDE);
        let width = width.clamp(MIN_SIDE, max);
        match side {
            Side::Left => self.left = width,
            Side::Right => self.right = width,
        }
    }

    /// The width a drag that started at `start_x` with the side `start_width`
    /// wide should give at `x`. The right column grows when dragged left.
    pub fn dragged_width(side: Side, start_width: f64, start_x: f64, x: f64) -> f64 {
        match side {
            Side::Left => start_width + (x - start_x),
            Side::Right => start_width - (x - start_x),
        }
    }

    /// The CSS `grid-template-columns` value.
    pub fn grid_template(&self) -> String {
        format!(
            "{}px {SPLITTER}px minmax(0, 1fr) {SPLITTER}px {}px",
            self.left, self.right
        )
    }
}
