mod common;

use application::{DateFormat, FileViewMode, Settings, Theme, UserSettings};
use common::FakeSettingsStore;
use std::sync::Arc;

#[test]
fn defaults_apply_until_something_is_saved() {
    let settings = UserSettings::new(Arc::new(FakeSettingsStore::default()));
    let loaded = settings.load().unwrap();
    assert_eq!(loaded, Settings::default());
    assert!(!loaded.indent_guides);
    assert_eq!(loaded.theme, Theme::System);
    assert_eq!(loaded.date_format, DateFormat::Iso);
    assert_eq!(loaded.file_view, FileViewMode::Tree);
}

#[test]
fn saved_settings_are_loaded_back() {
    let store = Arc::new(FakeSettingsStore::default());
    let settings = UserSettings::new(store.clone());
    let changed = Settings {
        indent_guides: true,
        theme: Theme::Dark,
        date_format: DateFormat::Relative,
        file_view: FileViewMode::Flat,
    };
    settings.save(&changed).unwrap();
    assert_eq!(settings.load().unwrap(), changed);
    assert_eq!(*store.settings.lock().unwrap(), Some(changed));
}
