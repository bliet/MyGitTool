//! git2-based adapters for the ports defined in `application`.
//!
//! No git2 type leaves this crate: everything is mapped to domain types at
//! the adapter boundary.

mod commit_graph;
mod credentials;
mod history;
mod locator;
mod map;
mod reader;
mod remote;
mod watcher;
mod writer;

pub use credentials::{AllowedCredentials, Attempt, CredentialChain, is_encrypted};
pub use locator::GitRepositoryLocator;
pub use reader::GitRepositoryReader;
pub use remote::GitRemoteOperations;
pub use watcher::{DEFAULT_DEBOUNCE, GitRepoWatcher};
pub use writer::GitRepositoryWriter;
