mod common;

use application::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, RememberLayout, WindowLayout};
use common::FakeLayoutStore;
use std::sync::Arc;

#[test]
fn the_default_applies_until_something_is_saved() {
    let layouts = RememberLayout::new(Arc::new(FakeLayoutStore::default()));
    assert_eq!(layouts.load().unwrap(), WindowLayout::default());
}

#[test]
fn a_saved_layout_is_loaded_back() {
    let layouts = RememberLayout::new(Arc::new(FakeLayoutStore::default()));
    let layout = WindowLayout {
        width: 1600.0,
        height: 1000.0,
        maximized: true,
        left_column: 300.0,
        right_column: 420.0,
    };
    layouts.save(&layout).unwrap();
    assert_eq!(layouts.load().unwrap(), layout);
}

#[test]
fn nonsense_sizes_fall_back_and_tiny_windows_grow() {
    let store = Arc::new(FakeLayoutStore::default());
    *store.layout.lock().unwrap() = Some(WindowLayout {
        width: 100.0,
        height: f64::NAN,
        maximized: false,
        left_column: -5.0,
        right_column: f64::INFINITY,
    });
    let loaded = RememberLayout::new(store).load().unwrap();
    let default = WindowLayout::default();
    assert_eq!(loaded.width, MIN_WINDOW_WIDTH);
    assert_eq!(loaded.height, default.height);
    assert_eq!(loaded.left_column, default.left_column);
    assert_eq!(loaded.right_column, default.right_column);
    assert!(MIN_WINDOW_HEIGHT < default.height);
}
