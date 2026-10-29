//! git2-based adapters for the ports defined in `application`.
//!
//! No git2 type leaves this crate: everything is mapped to domain types at
//! the adapter boundary.

mod locator;
mod map;
mod reader;

pub use locator::GitRepositoryLocator;
pub use reader::GitRepositoryReader;
