use crate::ports::{LocateError, RepositoryLocator};
use domain::RepositoryInfo;
use std::path::Path;
use std::sync::Arc;

/// Opens a repository from a path chosen by the user, and decides which
/// repository to show when the app starts.
#[derive(Clone)]
pub struct OpenRepository {
    locator: Arc<dyn RepositoryLocator>,
}

/// What the app shows after startup.
#[derive(Debug)]
pub enum Startup {
    Open(RepositoryInfo),
    /// No repository could be opened, so the user has to pick a folder.
    /// `error` explains why an explicitly requested path failed.
    PickFolder {
        error: Option<LocateError>,
    },
}

impl OpenRepository {
    pub fn new(locator: Arc<dyn RepositoryLocator>) -> Self {
        Self { locator }
    }

    pub fn open(&self, path: &Path) -> Result<RepositoryInfo, LocateError> {
        self.locator.locate(path)
    }

    /// Resolves the startup repository: the `requested` path (from the command
    /// line) if given, else the `current_dir` if it is inside a repository,
    /// else a folder picker.
    ///
    /// A requested path that fails does not fall back to the current
    /// directory, because the user asked for that specific repository.
    pub fn resolve_startup(&self, requested: Option<&Path>, current_dir: Option<&Path>) -> Startup {
        if let Some(path) = requested {
            return match self.open(path) {
                Ok(repo) => Startup::Open(repo),
                Err(error) => Startup::PickFolder { error: Some(error) },
            };
        }
        match current_dir.map(|dir| self.open(dir)) {
            Some(Ok(repo)) => Startup::Open(repo),
            Some(Err(LocateError::NotARepository { .. } | LocateError::NotFound { .. })) | None => {
                Startup::PickFolder { error: None }
            }
            Some(Err(error)) => Startup::PickFolder { error: Some(error) },
        }
    }
}
