//! Stashes against real repositories.

mod common;

use application::{
    ApplyStash, DropStash, LoadBranches, OperationError, RepositoryReader, SaveStash, StashOutcome,
};
use common::TestRepo;
use domain::{RepositoryInfo, Stash};
use infra_git::{GitRepositoryReader, GitRepositoryWriter};
use std::fs;
use std::sync::Arc;

fn info(repo: &TestRepo) -> RepositoryInfo {
    RepositoryInfo::new(repo.path())
}

fn read(repo: &TestRepo, file: &str) -> String {
    fs::read_to_string(repo.path().join(file)).unwrap()
}

fn stashes(repo: &TestRepo) -> Vec<Stash> {
    GitRepositoryReader.stashes(&info(repo)).unwrap()
}

fn save() -> SaveStash {
    SaveStash::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
}

fn apply() -> ApplyStash {
    ApplyStash::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
}

fn drop_stash() -> DropStash {
    DropStash::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
}

#[test]
fn saving_stashes_the_changes_and_cleans_the_work_tree() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .write("a.txt", "2\n")
        .write("new.txt", "x\n");
    save().save(&info(&repo), "half done", false).unwrap();

    assert_eq!(read(&repo, "a.txt"), "1\n");
    assert!(repo.path().join("new.txt").exists(), "untracked files stay");
    let list = stashes(&repo);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name(), "stash@{0}");
    assert_eq!(list[0].message, "On main: half done");
}

#[test]
fn untracked_files_are_stashed_on_request() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .write("new.txt", "x\n");
    assert_eq!(
        save().save(&info(&repo), "", false),
        Err(OperationError::NothingToStash)
    );
    save().save(&info(&repo), "", true).unwrap();
    assert!(!repo.path().join("new.txt").exists());
    assert!(stashes(&repo)[0].message.starts_with("WIP on main:"));
}

#[test]
fn stashes_are_listed_newest_first_with_the_refs() {
    let repo = TestRepo::new().commit("a.txt", "1\n").write("a.txt", "2\n");
    save().save(&info(&repo), "first", false).unwrap();
    let repo = repo.write("a.txt", "3\n");
    save().save(&info(&repo), "second", false).unwrap();

    let refs = LoadBranches::new(Arc::new(GitRepositoryReader))
        .load(&info(&repo))
        .unwrap();
    let list: Vec<_> = refs
        .stashes
        .iter()
        .map(|s| (s.name(), s.message.as_str()))
        .collect();
    assert_eq!(
        list,
        [
            ("stash@{0}".to_owned(), "On main: second"),
            ("stash@{1}".to_owned(), "On main: first")
        ]
    );
    // The stash commit is made on HEAD.
    let commit = repo
        .git()
        .find_commit(git2::Oid::from_bytes(refs.stashes[0].id.as_bytes()).unwrap())
        .unwrap();
    assert_eq!(commit.parent_id(0).unwrap(), repo.head());
}

#[test]
fn apply_keeps_the_stash_and_pop_drops_it() {
    let repo = TestRepo::new().commit("a.txt", "1\n").write("a.txt", "2\n");
    save().save(&info(&repo), "", false).unwrap();
    let stash = stashes(&repo).remove(0);

    assert_eq!(
        apply().apply(&info(&repo), &stash).unwrap(),
        StashOutcome::Applied
    );
    assert_eq!(read(&repo, "a.txt"), "2\n");
    assert_eq!(stashes(&repo).len(), 1);

    fs::write(repo.path().join("a.txt"), "1\n").unwrap();
    assert_eq!(
        apply().pop(&info(&repo), &stash).unwrap(),
        StashOutcome::Applied
    );
    assert_eq!(read(&repo, "a.txt"), "2\n");
    assert!(stashes(&repo).is_empty());
}

#[test]
fn applying_on_a_dirty_work_tree_is_refused() {
    let repo = TestRepo::new().commit("a.txt", "1\n").write("a.txt", "2\n");
    save().save(&info(&repo), "", false).unwrap();
    let stash = stashes(&repo).remove(0);
    let repo = repo.write("a.txt", "local\n");
    assert!(matches!(
        apply().apply(&info(&repo), &stash),
        Err(OperationError::DirtyWorkTree(_))
    ));
    assert_eq!(read(&repo, "a.txt"), "local\n");
}

#[test]
fn a_conflicting_pop_writes_markers_and_keeps_the_stash() {
    let repo = TestRepo::new()
        .commit("a.txt", "base\n")
        .write("a.txt", "stashed\n");
    save().save(&info(&repo), "", false).unwrap();
    let stash = stashes(&repo).remove(0);
    let repo = repo.commit("a.txt", "committed\n");

    assert_eq!(
        apply().pop(&info(&repo), &stash).unwrap(),
        StashOutcome::Conflicts(vec!["a.txt".into()])
    );
    let content = read(&repo, "a.txt");
    assert!(content.contains("<<<<<<<"), "markers in {content:?}");
    assert_eq!(stashes(&repo).len(), 1);
}

#[test]
fn drops_a_stash_by_its_commit() {
    let repo = TestRepo::new().commit("a.txt", "1\n").write("a.txt", "2\n");
    save().save(&info(&repo), "old", false).unwrap();
    let old = stashes(&repo).remove(0);
    let repo = repo.write("a.txt", "3\n");
    save().save(&info(&repo), "new", false).unwrap();

    // `old` moved to index 1 meanwhile.
    drop_stash().drop_stash(&info(&repo), &old).unwrap();
    let left: Vec<_> = stashes(&repo).into_iter().map(|s| s.message).collect();
    assert_eq!(left, ["On main: new"]);
}
