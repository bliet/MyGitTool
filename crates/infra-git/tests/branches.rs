//! Branch operations against real repositories.

mod common;

use application::{
    Checkout, CheckoutTarget, ContinueOperation, CreateBranch, CredentialProvider, DeleteBranch,
    IntegrationOutcome, LoadOngoing, MergeIntoCurrent, OperationError, ProgressReporter,
    RebaseCurrentOnto, RepositoryReader,
};
use common::TestRepo;
use domain::{
    CommitId, Credential, CredentialRequest, DirtyWorkTree, OngoingOperation, Progress,
    RepositoryInfo,
};
use infra_git::{GitRemoteOperations, GitRepositoryReader, GitRepositoryWriter};
use std::fs;
use std::path::Path;
use std::sync::Arc;

struct Silent;

impl ProgressReporter for Silent {
    fn report(&self, _: Progress) {}
}

impl CredentialProvider for Silent {
    fn credentials(&self, _: &CredentialRequest) -> Option<Credential> {
        None
    }
}

fn info(repo: &TestRepo) -> RepositoryInfo {
    RepositoryInfo::new(repo.path())
}

fn id(oid: git2::Oid) -> CommitId {
    CommitId::from_slice(oid.as_bytes()).unwrap()
}

fn read(repo: &TestRepo, file: &str) -> String {
    fs::read_to_string(repo.path().join(file)).unwrap()
}

fn head_name(repo: &TestRepo) -> String {
    repo.git().head().unwrap().shorthand().unwrap().to_owned()
}

fn checkout(repo: &TestRepo, target: CheckoutTarget) -> Result<String, OperationError> {
    Checkout::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
        .checkout(&info(repo), &target)
}

fn merge(repo: &TestRepo, refname: &str) -> Result<IntegrationOutcome, OperationError> {
    MergeIntoCurrent::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
        .merge(&info(repo), refname)
}

fn rebase(repo: &TestRepo, refname: &str) -> Result<IntegrationOutcome, OperationError> {
    RebaseCurrentOnto::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
        .rebase(&info(repo), refname)
}

fn resume(repo: &TestRepo) -> Result<IntegrationOutcome, OperationError> {
    ContinueOperation::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
        .resume(&info(repo))
}

fn delete() -> DeleteBranch {
    DeleteBranch::new(
        Arc::new(GitRepositoryReader),
        Arc::new(GitRepositoryWriter),
        Arc::new(GitRemoteOperations),
    )
}

/// Stages `file` with `content`, as resolving a conflict does.
fn resolve(repo: &TestRepo, file: &str, content: &str) {
    fs::write(repo.path().join(file), content).unwrap();
    let mut index = repo.git().index().unwrap();
    // The handle caches the index; the operation under test changed it on disk.
    index.read(true).unwrap();
    index.add_path(Path::new(file)).unwrap();
    index.write().unwrap();
}

/// `main` and `feature` both change `a.txt` after a common base.
fn conflicting_branches() -> TestRepo {
    TestRepo::new()
        .commit("a.txt", "base\n")
        .branch("feature")
        .checkout("feature")
        .commit("a.txt", "feature\n")
        .checkout("main")
        .commit("a.txt", "main\n")
}

// Checkout

#[test]
fn checkout_switches_branch_and_work_tree() {
    let repo = TestRepo::new()
        .commit("a.txt", "main\n")
        .branch("feature")
        .checkout("feature")
        .commit("a.txt", "feature\n")
        .checkout("main");
    assert_eq!(
        checkout(&repo, CheckoutTarget::Local("feature".into())),
        Ok("feature".into())
    );
    assert_eq!(head_name(&repo), "feature");
    assert_eq!(read(&repo, "a.txt"), "feature\n");
}

#[test]
fn checkout_carries_over_changes_to_untouched_files() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .commit("notes.txt", "n\n")
        .branch("feature")
        .checkout("feature")
        .commit("a.txt", "2\n")
        .checkout("main")
        .write("notes.txt", "edited\n");
    checkout(&repo, CheckoutTarget::Local("feature".into())).unwrap();
    assert_eq!(head_name(&repo), "feature");
    assert_eq!(read(&repo, "notes.txt"), "edited\n");
    assert_eq!(read(&repo, "a.txt"), "2\n");
}

#[test]
fn checkout_refuses_to_touch_modified_files() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .branch("feature")
        .checkout("feature")
        .commit("a.txt", "2\n")
        .checkout("main")
        .write("a.txt", "local edit\n");
    assert_eq!(
        checkout(&repo, CheckoutTarget::Local("feature".into())),
        Err(OperationError::DirtyWorkTree(DirtyWorkTree {
            files: vec!["a.txt".into()]
        }))
    );
    assert_eq!(head_name(&repo), "main");
    assert_eq!(read(&repo, "a.txt"), "local edit\n");
}

#[test]
fn checkout_of_a_remote_branch_creates_the_tracking_branch() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .remote("origin", "https://example.com/r.git")
        .commit("b.txt", "remote\n")
        .remote_branch("origin", "topic");
    let remote_tip = repo.head();
    let repo = repo.branch("old").checkout("old");
    // Move `old` away from the remote tip so the switch is visible.
    let repo = repo.commit("c.txt", "other\n");

    let target = CheckoutTarget::Remote {
        remote: "origin".into(),
        branch: "topic".into(),
    };
    assert_eq!(checkout(&repo, target), Ok("topic".into()));
    assert_eq!(head_name(&repo), "topic");
    assert_eq!(repo.head(), remote_tip);
    let tracking = GitRepositoryReader
        .tracking(&info(&repo), "topic")
        .unwrap()
        .unwrap();
    assert_eq!(tracking.tracking_ref, "refs/remotes/origin/topic");
}

// Create and delete

#[test]
fn create_branch_at_a_commit_and_switch_to_it() {
    let repo = TestRepo::new().commit("a.txt", "1\n");
    let first = repo.head();
    let repo = repo.commit("a.txt", "2\n");
    CreateBranch::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
        .create(&info(&repo), "fix/old", id(first), true)
        .unwrap();
    assert_eq!(head_name(&repo), "fix/old");
    assert_eq!(repo.head(), first);
    assert_eq!(read(&repo, "a.txt"), "1\n");
}

#[test]
fn delete_refuses_unmerged_branches_unless_forced() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .branch("merged")
        .branch("topic")
        .checkout("topic")
        .commit("b.txt", "only here\n")
        .checkout("main");
    let delete = delete();
    delete.delete_local(&info(&repo), "merged", false).unwrap();
    assert_eq!(
        delete.delete_local(&info(&repo), "topic", false),
        Err(OperationError::Unmerged {
            branch: "topic".into(),
            commits: 1
        })
    );
    delete.delete_local(&info(&repo), "topic", true).unwrap();
    let names: Vec<_> = repo
        .git()
        .branches(Some(git2::BranchType::Local))
        .unwrap()
        .map(|b| b.unwrap().0.name().unwrap().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["main"]);
}

#[test]
fn delete_on_the_remote_removes_branch_and_tracking_ref() {
    let remote = TestRepo::bare();
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .remote("origin", remote.path().to_str().unwrap())
        .branch("topic");
    repo.git()
        .find_remote("origin")
        .unwrap()
        .push(&["refs/heads/topic:refs/heads/topic"], None)
        .unwrap();
    repo.git()
        .find_remote("origin")
        .unwrap()
        .fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)
        .unwrap();
    assert!(
        repo.git()
            .find_reference("refs/remotes/origin/topic")
            .is_ok()
    );

    delete()
        .delete_remote(&info(&repo), "origin", "topic", &Silent, &Silent)
        .unwrap();
    assert!(remote.git().find_reference("refs/heads/topic").is_err());
    assert!(
        repo.git()
            .find_reference("refs/remotes/origin/topic")
            .is_err()
    );
}

// Merge and rebase

#[test]
fn merge_fast_forwards_or_creates_a_merge_commit() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .branch("ahead")
        .checkout("ahead")
        .commit("b.txt", "2\n")
        .checkout("main");
    let ahead = repo.head_of("ahead");
    assert_eq!(
        merge(&repo, "refs/heads/ahead"),
        Ok(IntegrationOutcome::FastForwarded { to: id(ahead) })
    );
    assert_eq!(read(&repo, "b.txt"), "2\n");

    let repo = repo
        .branch("side")
        .checkout("side")
        .commit("c.txt", "side\n")
        .checkout("main")
        .commit("d.txt", "main\n");
    assert_eq!(
        merge(&repo, "refs/heads/side"),
        Ok(IntegrationOutcome::Merged)
    );
    let head = repo.git().find_commit(repo.head()).unwrap();
    assert_eq!(head.parent_count(), 2);
    assert_eq!(head.message_bytes(), b"Merge branch 'side'");
}

#[test]
fn merge_conflict_then_continue_after_resolving() {
    let repo = conflicting_branches();
    let ours = repo.head();
    assert_eq!(
        merge(&repo, "refs/heads/feature"),
        Ok(IntegrationOutcome::Conflicts {
            operation: OngoingOperation::Merge,
            files: vec!["a.txt".into()]
        })
    );
    let ongoing = LoadOngoing::new(Arc::new(GitRepositoryReader))
        .load(&info(&repo))
        .unwrap()
        .unwrap();
    assert_eq!(ongoing.operation, OngoingOperation::Merge);
    assert_eq!(ongoing.conflicts, ["a.txt"]);
    assert_eq!(
        resume(&repo),
        Err(OperationError::UnresolvedConflicts(vec!["a.txt".into()]))
    );

    resolve(&repo, "a.txt", "both\n");
    assert_eq!(resume(&repo), Ok(IntegrationOutcome::Merged));
    let head = repo.git().find_commit(repo.head()).unwrap();
    assert_eq!(head.parent_ids().next(), Some(ours));
    assert_eq!(head.parent_count(), 2);
    assert!(
        head.message()
            .unwrap()
            .starts_with("Merge branch 'feature'")
    );
    assert_eq!(repo.git().state(), git2::RepositoryState::Clean);
    assert_eq!(read(&repo, "a.txt"), "both\n");
}

#[test]
fn rebase_replays_the_current_branch_onto_another() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .branch("base")
        .checkout("base")
        .commit("b.txt", "base\n")
        .checkout("main")
        .commit("c.txt", "mine\n");
    let base = repo.head_of("base");
    assert_eq!(
        rebase(&repo, "refs/heads/base"),
        Ok(IntegrationOutcome::Rebased)
    );
    let head = repo.git().find_commit(repo.head()).unwrap();
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), [base]);
    assert_eq!(head_name(&repo), "main");
}

#[test]
fn rebase_conflict_then_continue_after_resolving() {
    let repo = conflicting_branches().commit("e.txt", "after\n");
    assert_eq!(
        rebase(&repo, "refs/heads/feature"),
        Ok(IntegrationOutcome::Conflicts {
            operation: OngoingOperation::Rebase,
            files: vec!["a.txt".into()]
        })
    );
    resolve(&repo, "a.txt", "resolved\n");
    assert_eq!(resume(&repo), Ok(IntegrationOutcome::Rebased));
    assert_eq!(repo.git().state(), git2::RepositoryState::Clean);
    assert_eq!(head_name(&repo), "main");
    // Both of main's commits sit on top of feature now.
    let head = repo.git().find_commit(repo.head()).unwrap();
    let parent = head.parent(0).unwrap();
    assert_eq!(parent.parent_ids().next(), Some(repo.head_of("feature")));
    assert_eq!(read(&repo, "a.txt"), "resolved\n");
    assert_eq!(read(&repo, "e.txt"), "after\n");
}

#[test]
fn changed_paths_include_both_sides_of_a_rename() {
    let repo = TestRepo::new()
        .commit("old.txt", "x\n")
        .commit("keep.txt", "k\n");
    let before = repo.head();
    let repo = repo.rename("old.txt", "new.txt").commit_all("Rename");
    assert_eq!(
        GitRepositoryReader.changed_paths(&info(&repo), id(before), id(repo.head())),
        Ok(vec!["new.txt".to_owned(), "old.txt".to_owned()])
    );
}
