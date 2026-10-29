//! Use cases and port traits of MyGitTool.
//!
//! The ports are implemented by the infrastructure crates and injected into
//! the use cases by the composition root (`app`).

mod open_repository;
mod ports;
mod recent_repositories;

pub use open_repository::{OpenRepository, Startup};
pub use ports::{LocateError, RecentRepositoriesStore, RepositoryLocator, StorageError};
pub use recent_repositories::{DEFAULT_CAPACITY, RecentRepositories};
