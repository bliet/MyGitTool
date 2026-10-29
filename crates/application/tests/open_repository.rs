mod common;

use application::{LocateError, OpenRepository, Startup};
use common::FakeLocator;
use std::path::Path;
use std::sync::Arc;

fn use_case(locator: FakeLocator) -> OpenRepository {
    OpenRepository::new(Arc::new(locator))
}

fn opened_root(startup: Startup) -> std::path::PathBuf {
    match startup {
        Startup::Open(repo) => repo.root().to_path_buf(),
        other => panic!("expected an opened repository, got {other:?}"),
    }
}

#[test]
fn open_resolves_a_subdirectory_to_the_repository_root() {
    let open = use_case(FakeLocator::default().with_repo("/repos/a"));
    let repo = open.open(Path::new("/repos/a/src/deep")).unwrap();
    assert_eq!(repo.root(), Path::new("/repos/a"));
}

#[test]
fn open_reports_a_path_outside_any_repository() {
    let open = use_case(FakeLocator::default().with_repo("/repos/a"));
    let err = open.open(Path::new("/tmp")).unwrap_err();
    assert!(matches!(err, LocateError::NotARepository { .. }));
}

#[test]
fn startup_prefers_the_requested_path() {
    let open = use_case(
        FakeLocator::default()
            .with_repo("/repos/a")
            .with_repo("/repos/b"),
    );
    let startup = open.resolve_startup(Some(Path::new("/repos/a")), Some(Path::new("/repos/b")));
    assert_eq!(opened_root(startup), Path::new("/repos/a"));
}

#[test]
fn startup_does_not_fall_back_to_current_dir_when_requested_path_fails() {
    let open = use_case(FakeLocator::default().with_repo("/repos/b"));
    let startup = open.resolve_startup(Some(Path::new("/nowhere")), Some(Path::new("/repos/b")));
    match startup {
        Startup::PickFolder {
            error: Some(LocateError::NotARepository { path }),
        } => {
            assert_eq!(path, Path::new("/nowhere"));
        }
        other => panic!("expected a folder picker with an error, got {other:?}"),
    }
}

#[test]
fn startup_uses_current_dir_without_requested_path() {
    let open = use_case(FakeLocator::default().with_repo("/repos/b"));
    let startup = open.resolve_startup(None, Some(Path::new("/repos/b/sub")));
    assert_eq!(opened_root(startup), Path::new("/repos/b"));
}

#[test]
fn startup_shows_picker_quietly_when_current_dir_is_no_repository() {
    let open = use_case(FakeLocator::default());
    let startup = open.resolve_startup(None, Some(Path::new("/home/me")));
    assert!(matches!(startup, Startup::PickFolder { error: None }));
}

#[test]
fn startup_shows_picker_without_any_path() {
    let open = use_case(FakeLocator::default());
    assert!(matches!(
        open.resolve_startup(None, None),
        Startup::PickFolder { error: None }
    ));
}

#[test]
fn startup_reports_an_unreadable_current_dir_repository() {
    let open = use_case(FakeLocator::default().with_unreadable("/repos/locked"));
    let startup = open.resolve_startup(None, Some(Path::new("/repos/locked")));
    assert!(matches!(
        startup,
        Startup::PickFolder {
            error: Some(LocateError::Unreadable { .. })
        }
    ));
}
