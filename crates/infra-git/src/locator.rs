use application::{LocateError, RepositoryLocator};
use domain::RepositoryInfo;
use git2::{ErrorCode, Repository};
use std::path::{Path, PathBuf};

/// Locates repositories with `git2::Repository::discover`, so any directory
/// inside a work tree resolves to its repository.
#[derive(Debug, Default, Clone, Copy)]
pub struct GitRepositoryLocator;

impl RepositoryLocator for GitRepositoryLocator {
    fn locate(&self, path: &Path) -> Result<RepositoryInfo, LocateError> {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        if !path.exists() {
            return Err(LocateError::NotFound { path });
        }
        match Repository::discover(&path) {
            Ok(repo) => {
                let root = repo.workdir().unwrap_or_else(|| repo.path());
                Ok(RepositoryInfo::new(normalize(root)))
            }
            Err(err) if err.code() == ErrorCode::NotFound => {
                Err(LocateError::NotARepository { path })
            }
            Err(err) => Err(LocateError::Unreadable {
                path,
                message: err.message().to_owned(),
            }),
        }
    }
}

/// Rebuilds the path from its components. This drops the trailing slash
/// libgit2 appends to directories and uses the platform's separator.
fn normalize(path: &Path) -> PathBuf {
    path.components().collect()
}
