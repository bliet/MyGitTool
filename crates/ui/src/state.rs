use application::{LocateError, Startup, StorageError};
use domain::RepositoryInfo;
use std::path::{Path, PathBuf};

const APP_NAME: &str = "MyGitTool";

/// Which top-level screen the main window shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    /// No repository is open: show the folder picker and the recent list.
    Welcome,
    Repository(RepositoryInfo),
}

/// One entry of the recent-repositories list, ready for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentEntry {
    pub path: PathBuf,
    pub name: String,
    pub location: String,
}

impl RecentEntry {
    pub fn new(path: &Path) -> Self {
        let repo = RepositoryInfo::new(path);
        Self {
            path: path.to_path_buf(),
            name: repo.name(),
            location: path.display().to_string(),
        }
    }
}

/// View state of the main window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppState {
    pub screen: Screen,
    pub recent: Vec<RecentEntry>,
    /// A message shown in a dismissible banner.
    pub error: Option<String>,
}

impl AppState {
    pub fn from_startup(startup: Startup) -> Self {
        let (screen, error) = match startup {
            Startup::Open(repo) => (Screen::Repository(repo), None),
            Startup::PickFolder { error } => (Screen::Welcome, error.map(|e| e.to_string())),
        };
        Self {
            screen,
            recent: Vec::new(),
            error,
        }
    }

    pub fn current_repository(&self) -> Option<&RepositoryInfo> {
        match &self.screen {
            Screen::Repository(repo) => Some(repo),
            Screen::Welcome => None,
        }
    }

    pub fn window_title(&self) -> String {
        match &self.screen {
            Screen::Repository(repo) => format!("{} — {APP_NAME}", repo.name()),
            Screen::Welcome => APP_NAME.to_owned(),
        }
    }

    pub fn opened(&mut self, repo: RepositoryInfo) {
        self.screen = Screen::Repository(repo);
        self.error = None;
    }

    /// Keeps the current screen, so a failed open never closes the
    /// repository that is already shown.
    pub fn open_failed(&mut self, error: &LocateError) {
        self.error = Some(error.to_string());
    }

    pub fn close_repository(&mut self) {
        self.screen = Screen::Welcome;
    }

    pub fn set_recent(&mut self, paths: &[PathBuf]) {
        self.recent = paths.iter().map(|p| RecentEntry::new(p)).collect();
    }

    pub fn recent_failed(&mut self, error: &StorageError) {
        self.error = Some(format!("Recent repositories: {error}"));
    }

    pub fn dismiss_error(&mut self) {
        self.error = None;
    }
}
