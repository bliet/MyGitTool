mod common;

use application::{RepoChanges, WatchRepository};
use common::FakeWatcher;
use domain::RepositoryInfo;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[test]
fn changes_reach_the_callback_until_the_guard_is_dropped() {
    let watcher = Arc::new(FakeWatcher::default());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let guard = WatchRepository::new(watcher.clone())
        .watch(&RepositoryInfo::new("/repo"), {
            let seen = seen.clone();
            move |changes| seen.lock().unwrap().push(changes)
        })
        .unwrap();
    assert_eq!(*watcher.watched.lock().unwrap(), [PathBuf::from("/repo")]);

    let changes = RepoChanges {
        git_dir: true,
        work_tree: false,
    };
    watcher.report(changes);
    drop(guard);
    watcher.report(RepoChanges::ALL);
    assert_eq!(*seen.lock().unwrap(), [changes]);
}

#[test]
fn a_failing_watch_is_an_error() {
    let watcher = Arc::new(FakeWatcher {
        failing: true,
        ..FakeWatcher::default()
    });
    let result = WatchRepository::new(watcher).watch(&RepositoryInfo::new("/repo"), |_| {});
    assert!(result.is_err());
}

#[test]
fn changes_merge() {
    let git = RepoChanges {
        git_dir: true,
        work_tree: false,
    };
    let work = RepoChanges {
        git_dir: false,
        work_tree: true,
    };
    assert!(!RepoChanges::default().any());
    assert_eq!(git.merge(work), RepoChanges::ALL);
    assert_eq!(git.merge(RepoChanges::default()), git);
}
