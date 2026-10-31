use application::{DateFormat, FileViewMode, Settings, SettingsStore, Theme};
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
        theme: Theme::Light,
        date_format: DateFormat::MonthDayYear,
        file_view: FileViewMode::Flat,
    };
    store.save(&settings).unwrap();
    assert_eq!(store.load().unwrap(), Some(settings));
}

#[test]
fn missing_and_unknown_fields_fall_back_to_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonSettingsStore::in_config_dir(dir.path());
    fs::write(store.file(), r#"{ "version": 7, "sparkles": true }"#).unwrap();
    assert_eq!(store.load().unwrap(), Some(Settings::default()));
}

#[test]
fn unknown_choices_fall_back_to_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonSettingsStore::in_config_dir(dir.path());
    fs::write(
        store.file(),
        r#"{ "theme": "solarized", "date_format": "relative", "file_view": "grid" }"#,
    )
    .unwrap();
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.theme, Theme::System);
    assert_eq!(loaded.date_format, DateFormat::Relative);
    assert_eq!(loaded.file_view, FileViewMode::Tree);
}

#[test]
fn every_choice_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonSettingsStore::in_config_dir(dir.path());
    let themes = [Theme::System, Theme::Light, Theme::Dark];
    let formats = [
        DateFormat::Iso,
        DateFormat::DayMonthYear,
        DateFormat::MonthDayYear,
        DateFormat::Relative,
    ];
    for (i, date_format) in formats.into_iter().enumerate() {
        let settings = Settings {
            indent_guides: i % 2 == 0,
            theme: themes[i % themes.len()],
            date_format,
            file_view: if i % 2 == 0 {
                FileViewMode::Tree
            } else {
                FileViewMode::Flat
            },
        };
        store.save(&settings).unwrap();
        assert_eq!(store.load().unwrap(), Some(settings));
    }
}

#[test]
fn corrupt_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonSettingsStore::in_config_dir(dir.path());
    fs::write(store.file(), "not json").unwrap();
    let err = store.load().unwrap_err();
    assert!(err.to_string().contains("cannot parse"));
}
