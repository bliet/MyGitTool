use application::{DateFormat, FileViewMode, Theme};
use ui::view::settings::{DATE_FORMATS, FILE_VIEWS, THEMES, theme_attribute, theme_script};

#[test]
fn the_system_theme_sets_no_attribute() {
    assert_eq!(theme_attribute(Theme::System), None);
    assert_eq!(theme_attribute(Theme::Dark), Some("dark"));
    assert_eq!(theme_attribute(Theme::Light), Some("light"));
    assert!(theme_script(Theme::System).contains("delete"));
    assert!(theme_script(Theme::Dark).contains("\"dark\""));
}

#[test]
fn every_choice_is_offered_once() {
    let themes: Vec<Theme> = THEMES.iter().map(|(t, _)| *t).collect();
    assert_eq!(themes, [Theme::System, Theme::Light, Theme::Dark]);
    let formats: Vec<DateFormat> = DATE_FORMATS.iter().map(|(f, _)| *f).collect();
    assert_eq!(
        formats,
        [
            DateFormat::Iso,
            DateFormat::DayMonthYear,
            DateFormat::MonthDayYear,
            DateFormat::Relative
        ]
    );
    let views: Vec<FileViewMode> = FILE_VIEWS.iter().map(|(v, _)| *v).collect();
    assert_eq!(views, [FileViewMode::Tree, FileViewMode::Flat]);
}
