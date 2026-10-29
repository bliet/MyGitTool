use application::{RecentRepositoriesStore, StorageError};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;
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
        let content = match fs::read_to_string(&self.file) {
            Ok(content) => content,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(StorageError::new(self.context("cannot read"), err)),
        };
        let parsed: RecentFile = serde_json::from_str(&content)
            .map_err(|err| StorageError::new(self.context("cannot parse"), err))?;
        Ok(parsed.repositories)
    }

    fn save(&self, paths: &[PathBuf]) -> Result<(), StorageError> {
        let data = RecentFile {
            version: FORMAT_VERSION,
            repositories: paths.to_vec(),
        };
        let json = serde_json::to_string_pretty(&data)
            .map_err(|err| StorageError::new(self.context("cannot serialize"), err))?;
        if let Some(parent) = self.file.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                StorageError::new(self.context("cannot create the directory for"), err)
            })?;
        }
        // Write to a sibling file and rename it, so a crash never leaves a
        // half-written file behind.
        let tmp = self.file.with_extension("json.tmp");
        fs::write(&tmp, json)
            .map_err(|err| StorageError::new(self.context("cannot write"), err))?;
        fs::rename(&tmp, &self.file)
            .map_err(|err| StorageError::new(self.context("cannot write"), err))
    }
}

impl JsonRecentRepositoriesStore {
    fn context(&self, what: &str) -> String {
        format!("{what} {}", self.file.display())
    }
}
