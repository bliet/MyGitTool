//! File-based adapters for app settings and state that live in the user's
//! config directory.

mod json_file;
mod recent;
mod settings;

pub use recent::{ConfigDirError, JsonRecentRepositoriesStore, config_dir};
pub use settings::JsonSettingsStore;
