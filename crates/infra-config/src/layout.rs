use crate::json_file;
use application::{LayoutStore, StorageError, WindowLayout};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "window-layout.json";
const FORMAT_VERSION: u32 = 1;

/// Stores the main window's layout as a JSON file, apart from the settings
/// because it changes with every resize.
#[derive(Debug, Clone)]
pub struct JsonLayoutStore {
    file: PathBuf,
}

/// Missing fields take the default, so older and newer files still load.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct LayoutFile {
    version: u32,
    width: f64,
    height: f64,
    maximized: bool,
    left_column: f64,
    right_column: f64,
}

impl Default for LayoutFile {
    fn default() -> Self {
        Self::from(WindowLayout::default())
    }
}

impl From<WindowLayout> for LayoutFile {
    fn from(layout: WindowLayout) -> Self {
        Self {
            version: FORMAT_VERSION,
            width: layout.width,
            height: layout.height,
            maximized: layout.maximized,
            left_column: layout.left_column,
            right_column: layout.right_column,
        }
    }
}

impl JsonLayoutStore {
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

impl LayoutStore for JsonLayoutStore {
    fn load(&self) -> Result<Option<WindowLayout>, StorageError> {
        let file: Option<LayoutFile> = json_file::read(&self.file)?;
        Ok(file.map(|f| WindowLayout {
            width: f.width,
            height: f.height,
            maximized: f.maximized,
            left_column: f.left_column,
            right_column: f.right_column,
        }))
    }

    fn save(&self, layout: &WindowLayout) -> Result<(), StorageError> {
        json_file::write(&self.file, &LayoutFile::from(*layout))
    }
}
