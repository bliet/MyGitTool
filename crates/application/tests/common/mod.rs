//! Hand-written fakes of the port traits, shared by the use case tests.
#![allow(dead_code)]

use application::{LocateError, RecentRepositoriesStore, RepositoryLocator, StorageError};
use domain::RepositoryInfo;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A locator that knows a fixed set of repositories. Any path inside a known
/// root resolves to that root. `unreadable` paths fail with `Unreadable`.
#[derive(Default)]
pub struct FakeLocator {
    roots: Vec<PathBuf>,
    unreadable: Vec<PathBuf>,
}

impl FakeLocator {
    pub fn with_repo(mut self, root: impl Into<PathBuf>) -> Self {
        self.roots.push(root.into());
        self
    }

    pub fn with_unreadable(mut self, path: impl Into<PathBuf>) -> Self {
        self.unreadable.push(path.into());
        self
    }
}

impl RepositoryLocator for FakeLocator {
    fn locate(&self, path: &Path) -> Result<RepositoryInfo, LocateError> {
        if self.unreadable.iter().any(|p| p == path) {
            return Err(LocateError::Unreadable {
                path: path.to_path_buf(),
                message: "permission denied".into(),
            });
        }
        self.roots
            .iter()
            .find(|root| path.starts_with(root))
            .map(|root| RepositoryInfo::new(root.clone()))
            .ok_or_else(|| LocateError::NotARepository {
                path: path.to_path_buf(),
            })
    }
}

/// An in-memory store. Set `fail_saves` to make every save fail.
#[derive(Default)]
pub struct FakeStore {
    paths: Mutex<Vec<PathBuf>>,
    fail_saves: bool,
}

impl FakeStore {
    pub fn with_paths<I, P>(paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        Self {
            paths: Mutex::new(paths.into_iter().map(Into::into).collect()),
            ..Self::default()
        }
    }

    pub fn failing_saves(mut self) -> Self {
        self.fail_saves = true;
        self
    }

    pub fn stored(&self) -> Vec<PathBuf> {
        self.paths.lock().unwrap().clone()
    }
}

impl RecentRepositoriesStore for FakeStore {
    fn load(&self) -> Result<Vec<PathBuf>, StorageError> {
        Ok(self.stored())
    }

    fn save(&self, paths: &[PathBuf]) -> Result<(), StorageError> {
        if self.fail_saves {
            return Err(StorageError::new("cannot write", "disk full"));
        }
        *self.paths.lock().unwrap() = paths.to_vec();
        Ok(())
    }
}
