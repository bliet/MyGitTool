use application::{LayoutStore, WindowLayout};
use infra_config::JsonLayoutStore;
use std::fs;

#[test]
fn load_without_file_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonLayoutStore::in_config_dir(dir.path());
    assert_eq!(store.load().unwrap(), None);
}

#[test]
fn save_then_load_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonLayoutStore::in_config_dir(&dir.path().join("nested"));
    let layout = WindowLayout {
        width: 1234.5,
        height: 777.0,
        maximized: true,
        left_column: 222.0,
        right_column: 333.0,
        author_column: 111.0,
        date_column: 99.0,
        hash_column: 77.0,
    };
    store.save(&layout).unwrap();
    assert_eq!(store.load().unwrap(), Some(layout));
}

#[test]
fn missing_fields_fall_back_to_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonLayoutStore::in_config_dir(dir.path());
    fs::write(store.file(), r#"{ "width": 1000.0 }"#).unwrap();
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.width, 1000.0);
    assert_eq!(loaded.height, WindowLayout::default().height);
}
