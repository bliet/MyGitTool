use application::RecentRepositoriesStore;
use infra_config::JsonRecentRepositoriesStore;
use std::fs;
use std::path::PathBuf;

#[test]
fn load_without_file_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonRecentRepositoriesStore::in_config_dir(dir.path());
    assert!(store.load().unwrap().is_empty());
}

#[test]
fn save_then_load_round_trips_and_creates_missing_directories() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonRecentRepositoriesStore::in_config_dir(&dir.path().join("nested/config"));
    let paths = vec![PathBuf::from("/repos/a"), PathBuf::from("/repos/b")];

    store.save(&paths).unwrap();

    assert_eq!(store.load().unwrap(), paths);
    assert_eq!(
        JsonRecentRepositoriesStore::new(store.file())
            .load()
            .unwrap(),
        paths,
        "a fresh store reads the same file"
    );
}

#[test]
fn save_overwrites_the_previous_list() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonRecentRepositoriesStore::in_config_dir(dir.path());
    store
        .save(&[PathBuf::from("/a"), PathBuf::from("/b")])
        .unwrap();
    store.save(&[PathBuf::from("/c")]).unwrap();
    assert_eq!(store.load().unwrap(), vec![PathBuf::from("/c")]);
}

#[test]
fn file_is_versioned_json() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonRecentRepositoriesStore::in_config_dir(dir.path());
    store.save(&[PathBuf::from("/a")]).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(store.file()).unwrap()).unwrap();
    assert_eq!(json["version"], 1);
    assert_eq!(json["repositories"][0], "/a");
}

#[test]
fn load_reports_a_corrupt_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonRecentRepositoriesStore::in_config_dir(dir.path());
    fs::write(store.file(), "not json").unwrap();
    let err = store.load().unwrap_err();
    assert!(err.to_string().starts_with("cannot parse"), "{err}");
}
