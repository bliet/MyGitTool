//! File-based adapters for app settings and state that live in the user's
//! config directory.

mod recent;

pub use recent::{ConfigDirError, JsonRecentRepositoriesStore, config_dir};
