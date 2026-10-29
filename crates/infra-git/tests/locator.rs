mod common;

use application::{LocateError, RepositoryLocator};
use common::TestRepo;
use infra_git::GitRepositoryLocator;
use std::fs;
use std::path::Path;

fn same_dir(a: &Path, b: &Path) -> bool {
    fs::canonicalize(a).unwrap() == fs::canonicalize(b).unwrap()
}

#[test]
fn locates_a_repository_from_its_root() {
    let repo = TestRepo::new().commit("a.txt", "1");
    let info = GitRepositoryLocator.locate(repo.path()).unwrap();
    assert!(same_dir(info.root(), repo.path()));
}

#[test]
fn locates_a_repository_from_a_subdirectory() {
    let repo = TestRepo::new().mkdir("src/deep");
    let info = GitRepositoryLocator
        .locate(&repo.path().join("src/deep"))
        .unwrap();
    assert!(same_dir(info.root(), repo.path()));
}

#[test]
fn root_has_no_trailing_separator_and_names_the_directory() {
    let repo = TestRepo::new();
    let info = GitRepositoryLocator.locate(repo.path()).unwrap();
    let expected = repo.path().file_name().unwrap().to_string_lossy();
    assert_eq!(info.name(), expected);
    let root = info.root().to_string_lossy();
    assert!(!root.ends_with('/') && !root.ends_with('\\'), "{root}");
}

#[test]
fn locates_a_bare_repository() {
    let repo = TestRepo::bare();
    let info = GitRepositoryLocator.locate(repo.path()).unwrap();
    assert!(same_dir(info.root(), repo.path()));
}

#[test]
fn reports_a_directory_outside_any_repository() {
    let dir = tempfile::tempdir().unwrap();
    let err = GitRepositoryLocator.locate(dir.path()).unwrap_err();
    assert!(matches!(err, LocateError::NotARepository { .. }), "{err:?}");
}

#[test]
fn reports_a_missing_path() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    match GitRepositoryLocator.locate(&missing).unwrap_err() {
        LocateError::NotFound { path } => assert_eq!(path, missing),
        other => panic!("expected NotFound, got {other:?}"),
    }
}
