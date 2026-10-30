mod common;

use application::{Settings, UserSettings};
use common::FakeSettingsStore;
use std::sync::Arc;

#[test]
fn defaults_apply_until_something_is_saved() {
    let settings = UserSettings::new(Arc::new(FakeSettingsStore::default()));
    assert_eq!(settings.load().unwrap(), Settings::default());
    assert!(!settings.load().unwrap().indent_guides);
}

#[test]
fn saved_settings_are_loaded_back() {
    let store = Arc::new(FakeSettingsStore::default());
    let settings = UserSettings::new(store.clone());
    let changed = Settings {
        indent_guides: true,
    };
    settings.save(&changed).unwrap();
    assert_eq!(settings.load().unwrap(), changed);
    assert_eq!(*store.settings.lock().unwrap(), Some(changed));
}
