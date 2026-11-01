//! Staging, unstaging and committing against real repositories.

mod common;

use application::{CreateCommit, OperationError, RepositoryReader, RepositoryWriter, StageChanges};
use common::TestRepo;
use domain::{ChangeKind, FileChange, RepositoryInfo, WorkTreeChanges};
use infra_git::{GitRepositoryReader, GitRepositoryWriter};
use std::sync::Arc;

fn info(repo: &TestRepo) -> RepositoryInfo {
    RepositoryInfo::new(repo.path())
}

fn status(repo: &TestRepo) -> WorkTreeChanges {
    let mut changes = GitRepositoryReader.work_tree_changes(&info(repo)).unwrap();
    changes.staged.sort_by(|a, b| a.path.cmp(&b.path));
    changes.unstaged.sort_by(|a, b| a.path.cmp(&b.path));
    changes
}

/// Each change as `<letter> <path>`, e.g. `M a.txt`.
fn summary(changes: &[FileChange]) -> Vec<String> {
    changes
        .iter()
        .map(|c| format!("{} {}", c.kind.letter(), c.path))
        .collect()
}

fn stage(repo: &TestRepo, paths: &[&str]) {
    let paths: Vec<String> = paths.iter().map(|p| (*p).to_owned()).collect();
    GitRepositoryWriter.stage(&info(repo), &paths).unwrap();
}

fn unstage(repo: &TestRepo, paths: &[&str]) {
    let paths: Vec<String> = paths.iter().map(|p| (*p).to_owned()).collect();
    GitRepositoryWriter.unstage(&info(repo), &paths).unwrap();
}

fn create_commit() -> CreateCommit {
    CreateCommit::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
}

#[test]
fn changes_are_split_into_staged_and_unstaged() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .commit("b.txt", "1\n")
        .write("a.txt", "2\n")
        .stage("a.txt")
        .write("a.txt", "3\n")
        .write("b.txt", "2\n")
        .write("new.txt", "x\n");
    let changes = status(&repo);
    assert_eq!(summary(&changes.staged), ["M a.txt"]);
    assert_eq!(
        summary(&changes.unstaged),
        ["M a.txt", "M b.txt", "A new.txt"]
    );
}

#[test]
fn stages_modified_new_and_deleted_files() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .commit("gone.txt", "1\n")
        .write("a.txt", "2\n")
        .write("dir/new.txt", "x\n")
        .delete("gone.txt");
    stage(&repo, &["a.txt", "dir/new.txt", "gone.txt"]);
    let changes = status(&repo);
    assert_eq!(
        summary(&changes.staged),
        ["M a.txt", "A dir/new.txt", "D gone.txt"]
    );
    assert!(changes.unstaged.is_empty());
}

#[test]
fn unstages_back_to_head_and_keeps_the_work_tree() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .commit("gone.txt", "1\n")
        .write("a.txt", "2\n")
        .write("new.txt", "x\n")
        .delete("gone.txt");
    stage(&repo, &["a.txt", "new.txt", "gone.txt"]);
    unstage(&repo, &["a.txt", "new.txt", "gone.txt"]);
    let changes = status(&repo);
    assert!(changes.staged.is_empty());
    assert_eq!(
        summary(&changes.unstaged),
        ["M a.txt", "D gone.txt", "A new.txt"]
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "2\n"
    );
}

#[test]
fn unstaging_takes_paths_literally() {
    let repo = TestRepo::new()
        .commit("a*.txt", "1\n")
        .commit("ab.txt", "1\n")
        .write("a*.txt", "2\n")
        .write("ab.txt", "2\n")
        .stage("a*.txt")
        .stage("ab.txt");
    unstage(&repo, &["a*.txt"]);
    assert_eq!(summary(&status(&repo).staged), ["M ab.txt"]);
}

#[test]
fn unstaging_in_an_unborn_branch_removes_the_file_from_the_index() {
    let repo = TestRepo::new().write("a.txt", "1\n").stage("a.txt");
    assert_eq!(summary(&status(&repo).staged), ["A a.txt"]);
    unstage(&repo, &["a.txt"]);
    let changes = status(&repo);
    assert!(changes.staged.is_empty());
    assert_eq!(summary(&changes.unstaged), ["A a.txt"]);
}

#[test]
fn unstaging_a_staged_rename_restores_both_paths() {
    let repo = TestRepo::new()
        .commit("old.txt", "a\nb\nc\nd\n")
        .rename("old.txt", "new.txt")
        .stage("old.txt")
        .stage("new.txt");
    let staged = status(&repo).staged;
    assert_eq!(staged[0].kind, ChangeKind::Renamed);
    StageChanges::new(Arc::new(GitRepositoryWriter))
        .unstage(&info(&repo), &staged)
        .unwrap();
    let changes = status(&repo);
    assert!(changes.staged.is_empty());
    assert_eq!(summary(&changes.unstaged), ["A new.txt", "D old.txt"]);
}

#[test]
fn commits_only_what_is_staged() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .write("a.txt", "2\n")
        .write("b.txt", "x\n")
        .stage("a.txt");
    let parent = repo.head();
    let id = create_commit()
        .commit(&info(&repo), "Change a\n\nDetails.")
        .unwrap();

    let commit = repo.git().head().unwrap().peel_to_commit().unwrap();
    assert_eq!(commit.id().as_bytes(), id.as_bytes());
    assert_eq!(commit.message().ok(), Some("Change a\n\nDetails.\n"));
    assert_eq!(commit.parent_ids().collect::<Vec<_>>(), [parent]);
    assert_eq!(commit.author().name().ok(), Some("Test User"));
    let changes = status(&repo);
    assert!(changes.staged.is_empty());
    assert_eq!(summary(&changes.unstaged), ["A b.txt"]);
}

#[test]
fn the_first_commit_of_an_unborn_branch_has_no_parent() {
    let repo = TestRepo::new().write("a.txt", "1\n").stage("a.txt");
    create_commit().commit(&info(&repo), "Initial").unwrap();
    let commit = repo.git().head().unwrap().peel_to_commit().unwrap();
    assert_eq!(commit.parent_count(), 0);
    assert_eq!(repo.git().head().unwrap().shorthand().ok(), Some("main"));
}

#[test]
fn committing_without_staged_changes_is_refused() {
    let repo = TestRepo::new().commit("a.txt", "1\n").write("a.txt", "2\n");
    assert_eq!(
        create_commit().commit(&info(&repo), "Message"),
        Err(OperationError::NothingStaged)
    );
}

#[test]
fn staging_a_conflicted_file_resolves_it() {
    let repo = TestRepo::new()
        .commit("a.txt", "base\n")
        .branch("feature")
        .checkout("feature")
        .commit("a.txt", "feature\n")
        .checkout("main")
        .commit("a.txt", "main\n");
    let theirs = repo.head_of("feature");
    {
        let git = repo.git();
        let annotated = git.find_annotated_commit(theirs).unwrap();
        git.merge(&[&annotated], None, None).unwrap();
    }
    let changes = status(&repo);
    assert!(changes.staged.is_empty(), "conflicts are not staged");
    assert_eq!(summary(&changes.unstaged), ["M a.txt"]);

    std::fs::write(repo.path().join("a.txt"), "resolved\n").unwrap();
    stage(&repo, &["a.txt"]);
    assert!(
        GitRepositoryReader
            .conflicted_files(&info(&repo))
            .unwrap()
            .is_empty()
    );
    assert_eq!(summary(&status(&repo).staged), ["M a.txt"]);
}

fn apply_patch() -> application::ApplyPatch {
    application::ApplyPatch::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
}

#[test]
fn a_created_patch_applies_unstaged_to_another_clone() {
    let committed = || {
        TestRepo::new()
            .commit("a.txt", "one\n")
            .commit("b.txt", "b\n")
    };
    let source = committed()
        .write("a.txt", "one\ntwo\n")
        .stage("a.txt")
        .delete("b.txt")
        .write("new.txt", "fresh\n");
    let patch = application::CreatePatch::new(Arc::new(GitRepositoryReader))
        .patch(&info(&source))
        .unwrap();

    let target = committed();
    apply_patch().apply(&info(&target), &patch).unwrap();
    let changes = status(&target);
    assert!(changes.staged.is_empty());
    assert_eq!(
        summary(&changes.unstaged),
        ["M a.txt", "D b.txt", "A new.txt"]
    );
}

#[test]
fn a_patch_that_does_not_fit_changes_nothing() {
    let source = TestRepo::new()
        .commit("a.txt", "one\n")
        .commit("b.txt", "b\n")
        .write("a.txt", "one\ntwo\n")
        .write("b.txt", "B\n");
    let patch = application::CreatePatch::new(Arc::new(GitRepositoryReader))
        .patch(&info(&source))
        .unwrap();

    // `a.txt` differs here, so its hunk cannot apply; `b.txt` would.
    let target = TestRepo::new()
        .commit("a.txt", "other\n")
        .commit("b.txt", "b\n");
    let result = apply_patch().apply(&info(&target), &patch);
    assert!(
        matches!(&result, Err(OperationError::Write(err)) if err.to_string().contains("does not apply")),
        "{result:?}"
    );
    assert!(status(&target).is_empty());
}

#[test]
fn garbage_is_not_a_patch() {
    let repo = TestRepo::new().commit("a.txt", "1\n");
    let result = apply_patch().apply(&info(&repo), b"hello\n");
    assert!(
        matches!(result, Err(OperationError::Write(_))),
        "{result:?}"
    );
}
