use crate::json_file;
use application::{RecentRepositoriesStore, StorageError};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

const FILE_NAME: &str = "recent-repositories.json";
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Error)]
#[error("cannot determine the user config directory (no home directory found)")]
pub struct ConfigDirError;

/// The app's directory inside the platform's user config directory, e.g.
/// `~/.config/mygittool` on Linux.
pub fn config_dir() -> Result<PathBuf, ConfigDirError> {
    ProjectDirs::from("", "", "MyGitTool")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .ok_or(ConfigDirError)
}

/// Stores the recent repositories as a JSON file.
#[derive(Debug, Clone)]
pub struct JsonRecentRepositoriesStore {
    file: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct RecentFile {
    version: u32,
    repositories: Vec<PathBuf>,
}

impl JsonRecentRepositoriesStore {
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

impl RecentRepositoriesStore for JsonRecentRepositoriesStore {
    fn load(&self) -> Result<Vec<PathBuf>, StorageError> {
        let file: Option<RecentFile> = json_file::read(&self.file)?;
        Ok(file.map(|f| f.repositories).unwrap_or_default())
    }

    fn save(&self, paths: &[PathBuf]) -> Result<(), StorageError> {
        let data = RecentFile {
            version: FORMAT_VERSION,
            repositories: paths.to_vec(),
        };
        json_file::write(&self.file, &data)
    }
}
