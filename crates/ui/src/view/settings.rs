//! The choices of the settings dialog and how the theme is applied.

use application::{DateFormat, FileViewMode, Theme};

pub const THEMES: [(Theme, &str); 3] = [
    (Theme::System, "System"),
    (Theme::Light, "Light"),
    (Theme::Dark, "Dark"),
];

pub const DATE_FORMATS: [(DateFormat, &str); 4] = [
    (DateFormat::Iso, "2023-11-14 22:13"),
    (DateFormat::DayMonthYear, "14.11.2023 22:13"),
    (DateFormat::MonthDayYear, "11/14/2023 10:13 PM"),
    (DateFormat::Relative, "3 days ago"),
];

pub const FILE_VIEWS: [(FileViewMode, &str); 2] = [
    (FileViewMode::Tree, "Tree"),
    (FileViewMode::Flat, "Flat list"),
];

/// The `data-theme` attribute of the document root, or `None` to follow
/// the system.
pub fn theme_attribute(theme: Theme) -> Option<&'static str> {
    match theme {
        Theme::System => None,
        Theme::Light => Some("light"),
        Theme::Dark => Some("dark"),
    }
}

/// JavaScript that applies `theme` to the document.
pub fn theme_script(theme: Theme) -> String {
    match theme_attribute(theme) {
        Some(name) => format!("document.documentElement.dataset.theme = \"{name}\";"),
        None => "delete document.documentElement.dataset.theme;".to_owned(),
    }
}
