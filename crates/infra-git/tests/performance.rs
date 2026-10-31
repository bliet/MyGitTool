//! Timings for a large repository. Point `MYGITTOOL_PERF_REPO` at one (e.g. a
//! clone of the Linux kernel) and run
//! `cargo test --release -p infra-git --test performance -- --ignored --nocapture`.

use application::{CommitQuery, HistoryScope, LoadBranches, LoadHistory, LoadWorkTreeChanges};
use domain::{CommitId, RepositoryInfo};
use infra_git::GitRepositoryReader;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

const PAGE: usize = 500;

fn repo() -> Option<RepositoryInfo> {
    std::env::var_os("MYGITTOOL_PERF_REPO").map(RepositoryInfo::new)
}

#[test]
#[ignore = "needs MYGITTOOL_PERF_REPO"]
fn large_repository_timings() {
    let Some(repo) = repo() else {
        eprintln!("MYGITTOOL_PERF_REPO is not set");
        return;
    };
    let reader = Arc::new(GitRepositoryReader);

    let start = Instant::now();
    let refs = LoadBranches::new(reader.clone()).load(&repo).unwrap();
    eprintln!(
        "refs: {} local, {} tags in {:?}",
        refs.local.len(),
        refs.tags.len(),
        start.elapsed()
    );

    let start = Instant::now();
    let changes = LoadWorkTreeChanges::new(reader.clone())
        .load(&repo)
        .unwrap();
    eprintln!(
        "work tree: {} changes in {:?}",
        changes.len(),
        start.elapsed()
    );

    let start = Instant::now();
    let mut pager = LoadHistory::new(reader.clone())
        .open(&repo, &HistoryScope::All)
        .unwrap();
    let opened = start.elapsed();
    let first = pager.next_page(PAGE).unwrap();
    eprintln!(
        "history: opened in {opened:?}, first page of {} after {:?}",
        first.len(),
        start.elapsed()
    );

    // The rest of the walk, checking that no commit comes after a parent.
    let start = Instant::now();
    let mut seen: HashSet<CommitId> = first.iter().map(|r| r.commit.id).collect();
    let mut rows = 0;
    let mut widest = 0;
    while !pager.is_exhausted() {
        for row in pager.next_page(PAGE).unwrap() {
            assert!(
                row.commit.parents.iter().all(|p| !seen.contains(p)),
                "{} came after a parent",
                row.commit.id
            );
            seen.insert(row.commit.id);
            widest = widest.max(row.graph.width);
            rows += 1;
        }
    }
    eprintln!(
        "history: {rows} further rows in {:?}, at most {widest} lanes",
        start.elapsed()
    );

    // A search for something rare looks at the whole history.
    let start = Instant::now();
    let mut search = LoadHistory::new(reader)
        .search(
            &repo,
            &HistoryScope::All,
            CommitQuery::new("no such text anywhere").unwrap(),
        )
        .unwrap();
    let mut pages = 0;
    let mut first_page = None;
    while !search.is_exhausted() {
        search.next_page(PAGE).unwrap();
        pages += 1;
        first_page.get_or_insert(start.elapsed());
    }
    eprintln!(
        "search: first page after {:?}, whole history in {:?} ({pages} pages)",
        first_page.unwrap_or_default(),
        start.elapsed()
    );
}
