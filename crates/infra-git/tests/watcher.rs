mod common;

use application::{RepoChanges, RepoWatcher, WatchGuard};
use common::TestRepo;
use domain::RepositoryInfo;
use infra_git::GitRepoWatcher;
use std::fs;
use std::sync::mpsc::{Receiver, channel};
use std::thread::sleep;
use std::time::Duration;

const DEBOUNCE: Duration = Duration::from_millis(100);
/// Generous, as some backends (e.g. FSEvents) report with a delay.
const EXPECT: Duration = Duration::from_secs(5);
/// How long to wait to be fairly sure nothing is reported.
const QUIET: Duration = Duration::from_millis(1000);

fn watch(repo: &TestRepo) -> (WatchGuard, Receiver<RepoChanges>) {
    let (sender, receiver) = channel();
    let guard = GitRepoWatcher::new(DEBOUNCE)
        .watch(
            &RepositoryInfo::new(repo.path()),
            Box::new(move |changes| {
                let _ = sender.send(changes);
            }),
        )
        .unwrap();
    // Let the backend settle and drop anything it reports for the setup.
    sleep(Duration::from_millis(300));
    while receiver.recv_timeout(DEBOUNCE * 3).is_ok() {}
    (guard, receiver)
}

fn tracked() -> TestRepo {
    TestRepo::new()
        .commit(".gitignore", "build/\n")
        .commit("a.txt", "1")
}

#[test]
fn editing_a_file_is_a_work_tree_change() {
    let repo = tracked();
    let (_guard, changes) = watch(&repo);
    fs::write(repo.path().join("a.txt"), "2").unwrap();
    let reported = changes.recv_timeout(EXPECT).unwrap();
    assert!(reported.work_tree);
}

#[test]
fn creating_a_branch_is_a_git_dir_change() {
    let repo = tracked();
    let (_guard, changes) = watch(&repo);
    let _repo = repo.branch("feature");
    let reported = changes.recv_timeout(EXPECT).unwrap();
    assert!(reported.git_dir);
}

#[test]
fn ignored_files_are_not_reported() {
    let repo = tracked().mkdir("build");
    let (_guard, changes) = watch(&repo);
    fs::write(repo.path().join("build/out.o"), "binary").unwrap();
    assert!(changes.recv_timeout(QUIET).is_err());
}

#[test]
fn a_burst_of_changes_is_reported_once() {
    let repo = tracked();
    let (_guard, changes) = watch(&repo);
    for i in 0..5 {
        fs::write(repo.path().join(format!("new{i}.txt")), "x").unwrap();
        sleep(DEBOUNCE / 5);
    }
    assert!(changes.recv_timeout(EXPECT).unwrap().work_tree);
    let more = std::iter::from_fn(|| changes.recv_timeout(QUIET).ok()).count();
    assert!(more <= 1, "{more} more reports");
}

#[test]
fn dropping_the_guard_stops_the_watch() {
    let repo = tracked();
    let (guard, changes) = watch(&repo);
    drop(guard);
    sleep(DEBOUNCE);
    fs::write(repo.path().join("a.txt"), "2").unwrap();
    assert!(changes.recv_timeout(QUIET).is_err());
}

#[test]
fn a_path_that_is_no_repository_fails() {
    let dir = tempfile::tempdir().unwrap();
    let result =
        GitRepoWatcher::default().watch(&RepositoryInfo::new(dir.path()), Box::new(|_| {}));
    assert!(result.is_err());
}
