use application::{LocateError, Startup, StorageError};
use domain::RepositoryInfo;
use std::path::{Path, PathBuf};
use ui::{AppState, RecentEntry, Screen};

fn repo(path: &str) -> RepositoryInfo {
    RepositoryInfo::new(path)
}

fn not_a_repo(path: &str) -> LocateError {
    LocateError::NotARepository { path: path.into() }
}

#[test]
fn startup_with_a_repository_shows_it() {
    let state = AppState::from_startup(Startup::Open(repo("/repos/app")));
    assert_eq!(state.screen, Screen::Repository(repo("/repos/app")));
    assert_eq!(state.error, None);
    assert_eq!(state.window_title(), "app — MyGitTool");
}

#[test]
fn startup_without_a_repository_shows_welcome_and_the_reason() {
    let state = AppState::from_startup(Startup::PickFolder {
        error: Some(not_a_repo("/tmp/x")),
    });
    assert_eq!(state.screen, Screen::Welcome);
    assert_eq!(
        state.error.as_deref(),
        Some("/tmp/x is not inside a Git repository")
    );
    assert_eq!(state.window_title(), "MyGitTool");
}

#[test]
fn opening_switches_the_screen_and_clears_the_error() {
    let mut state = AppState::from_startup(Startup::PickFolder {
        error: Some(not_a_repo("/tmp/x")),
    });
    state.opened(repo("/repos/app"));
    assert_eq!(state.current_repository(), Some(&repo("/repos/app")));
    assert_eq!(state.error, None);
}

#[test]
fn a_failed_open_keeps_the_current_repository() {
    let mut state = AppState::from_startup(Startup::Open(repo("/repos/app")));
    state.open_failed(&not_a_repo("/tmp/x"));
    assert_eq!(state.current_repository(), Some(&repo("/repos/app")));
    assert!(state.error.is_some());
}

#[test]
fn closing_returns_to_welcome() {
    let mut state = AppState::from_startup(Startup::Open(repo("/repos/app")));
    state.close_repository();
    assert_eq!(state.screen, Screen::Welcome);
    assert_eq!(state.current_repository(), None);
}

#[test]
fn recent_entries_show_name_and_location() {
    let mut state = AppState::from_startup(Startup::PickFolder { error: None });
    state.set_recent(&[PathBuf::from("/repos/app"), PathBuf::from("/work/lib")]);
    assert_eq!(
        state.recent,
        vec![
            RecentEntry::new(Path::new("/repos/app")),
            RecentEntry::new(Path::new("/work/lib")),
        ]
    );
    assert_eq!(state.recent[0].name, "app");
    assert_eq!(
        state.recent[0].location,
        Path::new("/repos/app").display().to_string()
    );
}

#[test]
fn recent_failures_are_shown_and_dismissible() {
    let mut state = AppState::from_startup(Startup::PickFolder { error: None });
    state.recent_failed(&StorageError::new("cannot read /cfg", "denied"));
    assert_eq!(
        state.error.as_deref(),
        Some("Recent repositories: cannot read /cfg: denied")
    );
    state.dismiss_error();
    assert_eq!(state.error, None);
}
