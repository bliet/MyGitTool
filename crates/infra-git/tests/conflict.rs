//! Resolving conflicts against real repositories.

mod common;

use application::RepositoryReader;
use application::{
    ContinueOperation, IntegrationOutcome, LoadConflict, MergeIntoCurrent, ResolveConflict,
};
use common::TestRepo;
use domain::{
    ConflictContent, ConflictSide, MergeChunk, RepositoryInfo, Resolution, SideKind, merged_text,
};
use infra_git::{GitRepositoryReader, GitRepositoryWriter};
use std::fs;
use std::sync::Arc;

fn info(repo: &TestRepo) -> RepositoryInfo {
    RepositoryInfo::new(repo.path())
}

fn load() -> LoadConflict {
    LoadConflict::new(Arc::new(GitRepositoryReader))
}

fn resolve() -> ResolveConflict {
    ResolveConflict::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
}

/// Merges `feature` into `main`, which must conflict.
fn merge_feature(repo: &TestRepo) {
    let outcome =
        MergeIntoCurrent::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
            .merge(&info(repo), "refs/heads/feature")
            .unwrap();
    assert!(
        matches!(outcome, IntegrationOutcome::Conflicts { .. }),
        "{outcome:?}"
    );
}

/// `main` and `feature` both edit the second line of `a.txt`; `feature`
/// also edits its last line, which merges cleanly.
fn conflicting_edits() -> TestRepo {
    let repo = TestRepo::new()
        .commit("a.txt", "one\ntwo\nthree\nfour\nfive\n")
        .branch("feature")
        .checkout("feature")
        .commit("a.txt", "one\nTWO (feature)\nthree\nfour\nFIVE\n")
        .checkout("main")
        .commit("a.txt", "one\nTWO (main)\nthree\nfour\nfive\n");
    merge_feature(&repo);
    repo
}

#[test]
fn a_merge_conflict_loads_as_chunks_and_resolves() {
    let repo = conflicting_edits();
    let changes = GitRepositoryReader.work_tree_changes(&info(&repo)).unwrap();
    assert_eq!(changes.conflicts, ["a.txt"]);

    let conflict = load().load(&info(&repo), "a.txt").unwrap();
    let ConflictContent::Text(chunks) = conflict.content else {
        panic!("expected a text conflict");
    };
    let hunks: Vec<_> = chunks
        .iter()
        .filter_map(|c| match c {
            MergeChunk::Conflict(h) => Some((h.ours.as_str(), h.theirs.as_str())),
            MergeChunk::Clean(_) => None,
        })
        .collect();
    assert_eq!(hunks, [("TWO (main)\n", "TWO (feature)\n")]);

    let merged = merged_text(&chunks, &[Some(Resolution::OursThenTheirs)]).unwrap();
    assert_eq!(
        merged,
        "one\nTWO (main)\nTWO (feature)\nthree\nfour\nFIVE\n"
    );
    resolve()
        .resolve_text(&info(&repo), "a.txt", &merged)
        .unwrap();

    assert_eq!(
        fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        merged
    );
    assert!(
        GitRepositoryReader
            .conflicted_files(&info(&repo))
            .unwrap()
            .is_empty()
    );
    let outcome =
        ContinueOperation::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
            .resume(&info(&repo))
            .unwrap();
    assert_eq!(outcome, IntegrationOutcome::Merged);
    let head = repo.git().head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_count(), 2);
}

#[test]
fn a_file_deleted_on_one_side_is_resolved_by_taking_a_side() {
    let repo = TestRepo::new()
        .commit("a.txt", "keep\n")
        .commit("b.txt", "base\n")
        .branch("feature")
        .checkout("feature")
        .delete("b.txt")
        .commit_all("Delete b")
        .checkout("main")
        .commit("b.txt", "edited\n");
    merge_feature(&repo);

    let conflict = load().load(&info(&repo), "b.txt").unwrap();
    assert_eq!(
        conflict.content,
        ConflictContent::WholeFile {
            ours: SideKind::Text,
            theirs: SideKind::Deleted
        }
    );
    resolve()
        .take_side(&info(&repo), "b.txt", ConflictSide::Theirs)
        .unwrap();
    assert!(!repo.path().join("b.txt").exists());
    assert!(
        GitRepositoryReader
            .conflicted_files(&info(&repo))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn the_work_tree_file_can_be_marked_resolved_as_it_is() {
    let repo = conflicting_edits();
    fs::write(repo.path().join("a.txt"), "edited elsewhere\n").unwrap();
    resolve().keep_work_tree(&info(&repo), "a.txt").unwrap();
    assert!(
        GitRepositoryReader
            .conflicted_files(&info(&repo))
            .unwrap()
            .is_empty()
    );
    let mut index = repo.git().index().unwrap();
    // The handle caches the index; the resolution changed it on disk.
    index.read(true).unwrap();
    let entry = index.get_path(std::path::Path::new("a.txt"), 0).unwrap();
    let blob = repo.git().find_blob(entry.id).unwrap();
    assert_eq!(blob.content(), b"edited elsewhere\n");
}

#[test]
fn a_file_without_conflict_has_no_sides() {
    let repo = TestRepo::new().commit("a.txt", "1\n");
    assert_eq!(
        GitRepositoryReader
            .conflict_sides(&info(&repo), "a.txt", 1024)
            .unwrap(),
        None
    );
}
