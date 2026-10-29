//! Checks that the `TestRepo` fixture builds the repositories it promises.

mod common;

use common::TestRepo;
use git2::BranchType;

#[test]
fn new_repo_has_no_commits_and_main_as_initial_branch() {
    let repo = TestRepo::new();
    assert!(repo.git().head().is_err(), "unborn HEAD expected");
    let head = repo.git().find_reference("HEAD").unwrap();
    assert_eq!(head.symbolic_target().unwrap(), Some("refs/heads/main"));
}

#[test]
fn commits_are_chained_and_deterministic() {
    let repo = TestRepo::new().commit("a.txt", "1").commit("a.txt", "2");
    let head = repo.git().find_commit(repo.head()).unwrap();
    assert_eq!(head.summary().unwrap(), Some("Update a.txt"));
    assert_eq!(head.parent_count(), 1);
    assert!(head.time().seconds() > head.parent(0).unwrap().time().seconds());

    let again = TestRepo::new().commit("a.txt", "1").commit("a.txt", "2");
    assert_eq!(
        repo.head(),
        again.head(),
        "same content and times give same ids"
    );
}

#[test]
fn branch_and_checkout() {
    let repo = TestRepo::new()
        .commit("a.txt", "1")
        .branch("feature")
        .checkout("feature")
        .commit_with_message("b.txt", "2", "Add b");

    {
        let feature = repo
            .git()
            .find_branch("feature", BranchType::Local)
            .unwrap();
        assert!(feature.is_head());
        let main = repo.git().find_branch("main", BranchType::Local).unwrap();
        assert_ne!(main.get().target(), feature.get().target());
    }
    assert!(repo.path().join("b.txt").exists());

    let repo = repo.checkout("main");
    assert!(!repo.path().join("b.txt").exists());
}

#[test]
fn write_leaves_the_work_tree_dirty() {
    let repo = TestRepo::new()
        .commit("a.txt", "1")
        .write("a.txt", "changed");
    let statuses = repo.git().statuses(None).unwrap();
    assert_eq!(statuses.len(), 1);
    assert!(statuses.get(0).unwrap().status().is_wt_modified());
}

#[test]
fn bare_repo_has_no_work_tree() {
    let repo = TestRepo::bare();
    assert!(repo.git().is_bare());
}
