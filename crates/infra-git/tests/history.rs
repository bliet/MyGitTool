//! The history walk with and without a commit-graph. The graphs are written
//! by the git command line.

mod common;

use application::{HistoryScope, RepositoryReader};
use common::TestRepo;
use domain::{Commit, CommitId, RepositoryInfo};
use git2::Sort;
use infra_git::GitRepositoryReader;
use std::collections::{HashMap, HashSet};
use std::fs;

/// Small pages, so walks cross page boundaries.
const PAGE: usize = 3;

fn walk(repo: &TestRepo, scope: &HistoryScope) -> Vec<Commit> {
    let info = RepositoryInfo::new(repo.path());
    let mut cursor = GitRepositoryReader.history(&info, scope).unwrap();
    let mut commits = Vec::new();
    loop {
        let page = cursor.next_page(PAGE).unwrap();
        let done = page.len() < PAGE;
        commits.extend(page);
        if done {
            return commits;
        }
    }
}

fn all(repo: &TestRepo) -> Vec<Commit> {
    walk(repo, &HistoryScope::All)
}

/// The commits git2 reaches from `refs`, a ref name or a glob.
fn reachable(repo: &TestRepo, refs: &str) -> HashSet<CommitId> {
    let mut revwalk = repo.git().revwalk().unwrap();
    revwalk.set_sorting(Sort::NONE).unwrap();
    if refs.contains('*') {
        revwalk.push_glob(refs).unwrap();
    } else {
        revwalk.push_ref(refs).unwrap();
    }
    revwalk
        .map(|oid| CommitId::from_slice(oid.unwrap().as_bytes()).unwrap())
        .collect()
}

/// Every commit comes once, and before all of its parents.
fn assert_topological(commits: &[Commit]) {
    let index: HashMap<CommitId, usize> =
        commits.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
    assert_eq!(index.len(), commits.len(), "a commit came twice");
    for (i, commit) in commits.iter().enumerate() {
        for parent in &commit.parents {
            if let Some(&p) = index.get(parent) {
                assert!(p > i, "{} came before its child {}", parent, commit.id);
            }
        }
    }
}

fn ids(commits: &[Commit]) -> HashSet<CommitId> {
    commits.iter().map(|c| c.id).collect()
}

/// Two lines of work merged back and forth, plus a tag on a side branch.
fn branchy() -> TestRepo {
    TestRepo::new()
        .commit("a.txt", "1")
        .branch("feature")
        .commit("a.txt", "2")
        .checkout("feature")
        .commit("b.txt", "1")
        .commit("b.txt", "2")
        .checkout("main")
        .merge("feature", "merge feature")
        .commit("a.txt", "3")
        .checkout("feature")
        .commit("b.txt", "3")
        .branch("side")
        .checkout("side")
        .commit("c.txt", "1")
        .tag("v1")
        .checkout("main")
        .merge("feature", "merge feature again")
}

#[test]
fn walk_with_a_graph_covers_every_reachable_commit_in_order() {
    let repo = branchy().write_commit_graph(false);
    let commits = all(&repo);
    assert_topological(&commits);
    assert_eq!(ids(&commits), reachable(&repo, "refs/*"));
    assert_eq!(commits[0].summary, "merge feature again");
}

#[test]
fn walk_with_a_graph_matches_the_walk_without_one() {
    let repo = branchy();
    let without = all(&repo);
    let repo = repo.write_commit_graph(false);
    let with = all(&repo);
    assert_topological(&without);
    assert_eq!(ids(&with), ids(&without));
}

#[test]
fn commits_newer_than_the_graph_are_included() {
    let repo = branchy()
        .write_commit_graph(false)
        .commit("a.txt", "4")
        .branch("later")
        .checkout("later")
        .commit("d.txt", "1")
        .checkout("main")
        .merge("later", "merge later");
    let commits = all(&repo);
    assert_topological(&commits);
    assert_eq!(ids(&commits), reachable(&repo, "refs/*"));
    assert_eq!(commits[0].summary, "merge later");
}

#[test]
fn split_graph_chains_are_read() {
    let repo = branchy()
        .write_commit_graph(true)
        .commit("a.txt", "4")
        .checkout("side")
        .commit("c.txt", "2")
        .checkout("main")
        .merge("side", "merge side")
        .write_commit_graph(true)
        .commit("a.txt", "5");
    let chain = repo
        .path()
        .join(".git/objects/info/commit-graphs/commit-graph-chain");
    assert_eq!(fs::read_to_string(chain).unwrap().lines().count(), 2);
    let commits = all(&repo);
    assert_topological(&commits);
    assert_eq!(ids(&commits), reachable(&repo, "refs/*"));
}

#[test]
fn octopus_merges_reach_every_parent() {
    let repo = TestRepo::new()
        .commit("a.txt", "1")
        .branch("one")
        .branch("two")
        .branch("three")
        .checkout("one")
        .commit("one.txt", "1")
        .checkout("two")
        .commit("two.txt", "1")
        .checkout("three")
        .commit("three.txt", "1")
        .checkout("main")
        .merge_all(&["one", "two", "three"], "octopus")
        .write_commit_graph(false);
    // Only the merge leads to the three side commits.
    let commits = walk(&repo, &HistoryScope::Ref("refs/heads/main".into()));
    assert_topological(&commits);
    assert_eq!(commits.len(), 5);
    assert_eq!(commits[0].parents.len(), 4);
}

#[test]
fn a_child_older_than_its_parent_still_comes_first() {
    let repo = TestRepo::new()
        .commit("a.txt", "1")
        .commit("a.txt", "2")
        .branch("feature")
        .shift_clock(-3600)
        .commit("a.txt", "3")
        .checkout("feature")
        .shift_clock(3600)
        .commit("b.txt", "1")
        .checkout("main")
        .write_commit_graph(false)
        .shift_clock(-7200)
        .commit("a.txt", "4");
    let commits = all(&repo);
    assert_topological(&commits);
    assert_eq!(commits.len(), 5);
}

#[test]
fn a_ref_scope_with_a_graph_stays_on_its_branch() {
    let repo = branchy().write_commit_graph(false);
    let commits = walk(&repo, &HistoryScope::Ref("refs/heads/side".into()));
    assert_topological(&commits);
    assert_eq!(ids(&commits), reachable(&repo, "refs/heads/side"));
}

#[test]
fn a_corrupt_graph_is_ignored() {
    let repo = branchy();
    fs::write(
        repo.path().join(".git/objects/info/commit-graph"),
        b"CGPH garbage",
    )
    .unwrap();
    let commits = all(&repo);
    assert_topological(&commits);
    assert_eq!(ids(&commits), reachable(&repo, "refs/*"));
}
