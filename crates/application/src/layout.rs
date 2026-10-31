use crate::ports::{LayoutStore, StorageError, WindowLayout};
use std::sync::Arc;

/// The smallest main window, in logical pixels.
pub const MIN_WINDOW_WIDTH: f64 = 800.0;
pub const MIN_WINDOW_HEIGHT: f64 = 500.0;

/// Remembers the main window's layout across sessions.
#[derive(Clone)]
pub struct RememberLayout {
    store: Arc<dyn LayoutStore>,
}

impl RememberLayout {
    pub fn new(store: Arc<dyn LayoutStore>) -> Self {
        Self { store }
    }

    /// The stored layout, or the default if none was stored yet. Sizes that
    /// make no sense (e.g. from an edited file) fall back to the default,
    /// and a window smaller than the minimum is enlarged.
    pub fn load(&self) -> Result<WindowLayout, StorageError> {
        Ok(self.store.load()?.map(sanitize).unwrap_or_default())
    }

    pub fn save(&self, layout: &WindowLayout) -> Result<(), StorageError> {
        self.store.save(&sanitize(*layout))
    }
}

fn sanitize(layout: WindowLayout) -> WindowLayout {
    let default = WindowLayout::default();
    let valid = |value: f64, fallback: f64| {
        if value.is_finite() && value > 0.0 {
            value
        } else {
            fallback
        }
    };
    WindowLayout {
        width: valid(layout.width, default.width).max(MIN_WINDOW_WIDTH),
        height: valid(layout.height, default.height).max(MIN_WINDOW_HEIGHT),
        maximized: layout.maximized,
        left_column: valid(layout.left_column, default.left_column),
        right_column: valid(layout.right_column, default.right_column),
        author_column: valid(layout.author_column, default.author_column),
        date_column: valid(layout.date_column, default.date_column),
        hash_column: valid(layout.hash_column, default.hash_column),
    }
}
