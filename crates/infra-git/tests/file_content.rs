mod common;

use application::{ComputeFileDiff, ReadError, RepositoryReader};
use common::TestRepo;
use domain::{
    ChangeKind, CommitId, DiffBody, DiffSource, FileContent, RepositoryInfo, Revision, RowKind,
};
use infra_git::GitRepositoryReader;
use std::sync::Arc;

const LIMIT: u64 = 1024;

fn info(repo: &TestRepo) -> RepositoryInfo {
    RepositoryInfo::new(repo.path())
}

fn id(oid: git2::Oid) -> CommitId {
    CommitId::from_slice(oid.as_bytes()).unwrap()
}

fn read(repo: &TestRepo, revision: Revision, path: &str) -> Option<FileContent> {
    GitRepositoryReader
        .file_content(&info(repo), revision, path, LIMIT)
        .unwrap()
}

fn bytes(content: &str) -> Option<FileContent> {
    Some(FileContent::Bytes(content.as_bytes().to_vec()))
}

#[test]
fn reads_a_file_of_a_commit() {
    let repo = TestRepo::new().commit("dir/a.txt", "one\n");
    let first = repo.head();
    let repo = repo.commit("dir/a.txt", "two\n");

    assert_eq!(
        read(&repo, Revision::Commit(id(first)), "dir/a.txt"),
        bytes("one\n")
    );
    assert_eq!(
        read(&repo, Revision::Commit(id(repo.head())), "dir/a.txt"),
        bytes("two\n")
    );
    assert_eq!(read(&repo, Revision::Head, "dir/a.txt"), bytes("two\n"));
}

#[test]
fn missing_paths_and_directories_are_no_file() {
    let repo = TestRepo::new().commit("dir/a.txt", "1");
    assert_eq!(read(&repo, Revision::Head, "nope.txt"), None);
    assert_eq!(read(&repo, Revision::Head, "dir"), None);
    assert_eq!(read(&repo, Revision::WorkTree, "nope.txt"), None);
    assert_eq!(read(&repo, Revision::WorkTree, "dir"), None);
}

#[test]
fn head_of_an_empty_repository_has_no_files() {
    let repo = TestRepo::new().write("a.txt", "1");
    assert_eq!(read(&repo, Revision::Head, "a.txt"), None);
    assert_eq!(read(&repo, Revision::WorkTree, "a.txt"), bytes("1"));
}

#[test]
fn reads_the_work_tree_without_staging() {
    let repo = TestRepo::new()
        .commit("a.txt", "committed")
        .write("a.txt", "edited");
    assert_eq!(read(&repo, Revision::Head, "a.txt"), bytes("committed"));
    assert_eq!(read(&repo, Revision::WorkTree, "a.txt"), bytes("edited"));
}

#[test]
fn large_files_are_not_loaded() {
    let big = "x".repeat(LIMIT as usize + 1);
    let repo = TestRepo::new().commit("big.txt", &big);
    let too_large = Some(FileContent::TooLarge { size: LIMIT + 1 });
    assert_eq!(read(&repo, Revision::Head, "big.txt"), too_large);
    assert_eq!(read(&repo, Revision::WorkTree, "big.txt"), too_large);
}

#[test]
fn unknown_commit_is_an_error() {
    let repo = TestRepo::new().commit("a.txt", "1");
    let unknown = CommitId::from_bytes([7; 20]);
    let result =
        GitRepositoryReader.file_content(&info(&repo), Revision::Commit(unknown), "a.txt", LIMIT);
    assert_eq!(result, Err(ReadError::CommitNotFound(unknown)));
}

#[cfg(unix)]
#[test]
fn work_tree_symlink_reads_as_its_target() {
    let repo = TestRepo::new().commit("a.txt", "1");
    std::os::unix::fs::symlink("a.txt", repo.path().join("link")).unwrap();
    assert_eq!(read(&repo, Revision::WorkTree, "link"), bytes("a.txt"));
}

#[test]
fn computes_a_work_tree_diff_end_to_end() {
    let repo = TestRepo::new()
        .commit("a.txt", "keep\nold\n")
        .write("a.txt", "keep\nnew\nmore\n");
    let changes = GitRepositoryReader.work_tree_changes(&info(&repo)).unwrap();
    let diff = ComputeFileDiff::new(Arc::new(GitRepositoryReader))
        .compute(&info(&repo), DiffSource::Unstaged, &changes.unstaged[0])
        .unwrap();
    let DiffBody::Text(text) = diff.body else {
        panic!("expected a text diff, got {:?}", diff.body);
    };
    let kinds: Vec<_> = text.rows.iter().map(|r| r.kind).collect();
    assert_eq!(kinds, [RowKind::Context, RowKind::Changed, RowKind::Added]);
}

#[test]
fn computes_a_renamed_file_diff_from_a_commit() {
    let repo = TestRepo::new().commit("old.txt", "a\nb\nc\nd\n");
    let parent = repo.head();
    let repo = repo.rename("old.txt", "new.txt").commit_all("Rename");
    let details = GitRepositoryReader
        .commit_details(&info(&repo), id(repo.head()))
        .unwrap();
    let change = &details.changes[0];
    assert_eq!(change.kind, ChangeKind::Renamed);
    let source = DiffSource::Commit {
        id: id(repo.head()),
        parent: Some(id(parent)),
    };
    let diff = ComputeFileDiff::new(Arc::new(GitRepositoryReader))
        .compute(&info(&repo), source, change)
        .unwrap();
    assert_eq!(diff.old_path.as_deref(), Some("old.txt"));
    assert_eq!(diff.new_path.as_deref(), Some("new.txt"));
    let DiffBody::Text(text) = diff.body else {
        panic!("expected a text diff");
    };
    assert!(text.hunks.is_empty());
}

#[test]
fn binary_files_are_reported_as_binary() {
    let repo = TestRepo::new()
        .commit("img.bin", "a\0b")
        .write("img.bin", "a\0c");
    let changes = GitRepositoryReader.work_tree_changes(&info(&repo)).unwrap();
    let diff = ComputeFileDiff::new(Arc::new(GitRepositoryReader))
        .compute(&info(&repo), DiffSource::Unstaged, &changes.unstaged[0])
        .unwrap();
    assert_eq!(diff.body, DiffBody::Binary);
}

#[test]
fn reads_a_staged_file_from_the_index() {
    let repo = TestRepo::new()
        .commit("a.txt", "committed")
        .write("a.txt", "staged")
        .stage("a.txt")
        .write("a.txt", "edited");
    assert_eq!(read(&repo, Revision::Head, "a.txt"), bytes("committed"));
    assert_eq!(read(&repo, Revision::Index, "a.txt"), bytes("staged"));
    assert_eq!(read(&repo, Revision::WorkTree, "a.txt"), bytes("edited"));
    assert_eq!(read(&repo, Revision::Index, "nope.txt"), None);
}

#[test]
fn staged_and_unstaged_diffs_split_at_the_index() {
    let repo = TestRepo::new()
        .commit("a.txt", "one\n")
        .write("a.txt", "one\ntwo\n")
        .stage("a.txt")
        .write("a.txt", "one\ntwo\nthree\n");
    let changes = GitRepositoryReader.work_tree_changes(&info(&repo)).unwrap();
    let compute = ComputeFileDiff::new(Arc::new(GitRepositoryReader));
    let rows = |source, change| {
        let diff = compute.compute(&info(&repo), source, change).unwrap();
        let DiffBody::Text(text) = diff.body else {
            panic!("expected a text diff, got {:?}", diff.body);
        };
        text.rows
            .iter()
            .map(|r| r.new.as_ref().map(|l| l.text.clone()).unwrap_or_default())
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(DiffSource::Staged, &changes.staged[0]), ["one", "two"]);
    assert_eq!(
        rows(DiffSource::Unstaged, &changes.unstaged[0]),
        ["one", "two", "three"]
    );
}

#[test]
fn the_work_tree_patch_applies_to_a_clean_copy() {
    let committed = || {
        TestRepo::new()
            .commit("a.txt", "one\ntwo\n")
            .commit("gone.txt", "bye\n")
    };
    let repo = committed()
        .write("a.txt", "one\nTWO\n")
        .stage("a.txt")
        .write("a.txt", "one\nTWO\nthree\n")
        .delete("gone.txt")
        .write("dir/new.txt", "fresh\n");
    let patch = GitRepositoryReader.work_tree_patch(&info(&repo)).unwrap();

    let copy = committed();
    let diff = git2::Diff::from_buffer(&patch).unwrap();
    copy.git()
        .apply(&diff, git2::ApplyLocation::WorkDir, None)
        .unwrap();
    let read = |file: &str| std::fs::read_to_string(copy.path().join(file)).ok();
    assert_eq!(read("a.txt").as_deref(), Some("one\nTWO\nthree\n"));
    assert_eq!(read("dir/new.txt").as_deref(), Some("fresh\n"));
    assert_eq!(read("gone.txt"), None);
}

#[test]
fn a_clean_work_tree_has_an_empty_patch() {
    let repo = TestRepo::new().commit("a.txt", "1\n");
    assert!(
        GitRepositoryReader
            .work_tree_patch(&info(&repo))
            .unwrap()
            .is_empty()
    );
}
