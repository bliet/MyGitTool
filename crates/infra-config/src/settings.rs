use crate::json_file;
use application::{DateFormat, FileViewMode, Settings, SettingsStore, StorageError, Theme};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "settings.json";
const FORMAT_VERSION: u32 = 1;

/// Stores the user's settings as a JSON file.
#[derive(Debug, Clone)]
pub struct JsonSettingsStore {
    file: PathBuf,
}

/// Every field has a default, so files written by older or newer versions
/// still load. Choices are stored as names, and a name this version does not
/// know falls back to the default.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct SettingsFile {
    version: u32,
    indent_guides: bool,
    theme: String,
    date_format: String,
    file_view: String,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self::from(Settings::default())
    }
}

impl From<Settings> for SettingsFile {
    fn from(settings: Settings) -> Self {
        Self {
            version: FORMAT_VERSION,
            indent_guides: settings.indent_guides,
            theme: theme_name(settings.theme).to_owned(),
            date_format: date_format_name(settings.date_format).to_owned(),
            file_view: file_view_name(settings.file_view).to_owned(),
        }
    }
}

impl From<SettingsFile> for Settings {
    fn from(file: SettingsFile) -> Self {
        Self {
            indent_guides: file.indent_guides,
            theme: match file.theme.as_str() {
                "light" => Theme::Light,
                "dark" => Theme::Dark,
                _ => Theme::System,
            },
            date_format: match file.date_format.as_str() {
                "day-month-year" => DateFormat::DayMonthYear,
                "month-day-year" => DateFormat::MonthDayYear,
                "relative" => DateFormat::Relative,
                _ => DateFormat::Iso,
            },
            file_view: match file.file_view.as_str() {
                "flat" => FileViewMode::Flat,
                _ => FileViewMode::Tree,
            },
        }
    }
}

fn theme_name(theme: Theme) -> &'static str {
    match theme {
        Theme::System => "system",
        Theme::Light => "light",
        Theme::Dark => "dark",
    }
}

fn date_format_name(format: DateFormat) -> &'static str {
    match format {
        DateFormat::Iso => "iso",
        DateFormat::DayMonthYear => "day-month-year",
        DateFormat::MonthDayYear => "month-day-year",
        DateFormat::Relative => "relative",
    }
}

fn file_view_name(mode: FileViewMode) -> &'static str {
    match mode {
        FileViewMode::Tree => "tree",
        FileViewMode::Flat => "flat",
    }
}

impl JsonSettingsStore {
    /// A store that reads and writes `file`.
    pub fn new(file: impl Into<PathBuf>) -> Self {
        Self { file: file.into() }
    }

    /// A store for the file in the app's user config directory.
    pub fn in_config_dir(dir: &Path) -> Self {
        Self::new(dir.join(FILE_NAME))
    }

    pub fn file(&self) -> &Path {
        &self.file
    }
}

impl SettingsStore for JsonSettingsStore {
    fn load(&self) -> Result<Option<Settings>, StorageError> {
        let file: Option<SettingsFile> = json_file::read(&self.file)?;
        Ok(file.map(Settings::from))
    }

    fn save(&self, settings: &Settings) -> Result<(), StorageError> {
        json_file::write(&self.file, &SettingsFile::from(*settings))
    }
}
