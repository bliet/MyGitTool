use application::{Settings, SettingsStore};
use infra_config::JsonSettingsStore;
use std::fs;

#[test]
fn load_without_file_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonSettingsStore::in_config_dir(dir.path());
    assert_eq!(store.load().unwrap(), None);
}

#[test]
fn save_then_load_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonSettingsStore::in_config_dir(&dir.path().join("nested"));
    let settings = Settings {
        indent_guides: true,
    };
    store.save(&settings).unwrap();
    assert_eq!(store.load().unwrap(), Some(settings));
}

#[test]
fn missing_and_unknown_fields_fall_back_to_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonSettingsStore::in_config_dir(dir.path());
    fs::write(store.file(), r#"{ "version": 7, "theme": "dark" }"#).unwrap();
    assert_eq!(store.load().unwrap(), Some(Settings::default()));
}

#[test]
fn corrupt_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonSettingsStore::in_config_dir(dir.path());
    fs::write(store.file(), "not json").unwrap();
    let err = store.load().unwrap_err();
    assert!(err.to_string().contains("cannot parse"));
}
