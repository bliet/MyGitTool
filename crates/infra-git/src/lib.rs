//! git2-based adapters for the ports defined in `application`.
//!
//! No git2 type leaves this crate: everything is mapped to domain types at
//! the adapter boundary.

mod credentials;
mod locator;
mod map;
mod reader;
mod remote;
mod writer;

pub use credentials::{AllowedCredentials, Attempt, CredentialChain, is_encrypted};
pub use locator::GitRepositoryLocator;
pub use reader::GitRepositoryReader;
pub use remote::GitRemoteOperations;
pub use writer::GitRepositoryWriter;
