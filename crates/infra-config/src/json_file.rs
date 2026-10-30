//! Reading and writing one JSON file, shared by the stores.

use application::StorageError;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

/// The parsed file, or `None` if it does not exist.
pub fn read<T: DeserializeOwned>(file: &Path) -> Result<Option<T>, StorageError> {
    let content = match fs::read_to_string(file) {
        Ok(content) => content,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(StorageError::new(context("cannot read", file), err)),
    };
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|err| StorageError::new(context("cannot parse", file), err))
}

/// Writes `data`, creating missing directories.
pub fn write<T: Serialize>(file: &Path, data: &T) -> Result<(), StorageError> {
    let json = serde_json::to_string_pretty(data)
        .map_err(|err| StorageError::new(context("cannot serialize", file), err))?;
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            StorageError::new(context("cannot create the directory for", file), err)
        })?;
    }
    // Write to a sibling file and rename it, so a crash never leaves a
    // half-written file behind.
    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|err| StorageError::new(context("cannot write", file), err))?;
    fs::rename(&tmp, file).map_err(|err| StorageError::new(context("cannot write", file), err))
}

fn context(what: &str, file: &Path) -> String {
    format!("{what} {}", file.display())
}
