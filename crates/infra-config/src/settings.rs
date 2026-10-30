use crate::json_file;
use application::{Settings, SettingsStore, StorageError};
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
/// still load.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct SettingsFile {
    version: u32,
    indent_guides: bool,
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
        }
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
        Ok(file.map(|f| Settings {
            indent_guides: f.indent_guides,
        }))
    }

    fn save(&self, settings: &Settings) -> Result<(), StorageError> {
        json_file::write(&self.file, &SettingsFile::from(*settings))
    }
}
