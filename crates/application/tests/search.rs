mod common;

use application::{CommitQuery, HistoryScope, LoadHistory, SEARCH_BATCH};
use common::{FakeReader, commit, id};
use domain::{Commit, RepositoryInfo};
use std::sync::Arc;

fn named(n: u8, summary: &str, author: &str) -> Commit {
    let mut commit = commit(n, &[]);
    commit.summary = summary.into();
    commit.author.name = author.into();
    commit.author.email = format!("{}@example.com", author.to_lowercase());
    commit
}

fn query(text: &str) -> CommitQuery {
    CommitQuery::new(text).unwrap()
}

#[test]
fn blank_queries_are_no_search() {
    assert_eq!(CommitQuery::new(""), None);
    assert_eq!(CommitQuery::new("  \t "), None);
}

#[test]
fn matches_summary_author_and_email_ignoring_case() {
    let commit = named(1, "Fix the Parser", "Ann Lee");
    assert!(query("parser").matches(&commit));
    assert!(query("ANN").matches(&commit));
    assert!(query("ann lee@example").matches(&commit));
    assert!(!query("lexer").matches(&commit));
}

#[test]
fn every_word_has_to_match_somewhere() {
    let commit = named(1, "Fix the parser", "Ann");
    assert!(query("fix ann").matches(&commit));
    assert!(!query("fix bob").matches(&commit));
}

#[test]
fn hashes_match_by_their_start() {
    let commit = named(0xab, "x", "Ann");
    let hex = commit.id.to_hex();
    assert!(query(&hex[..7]).matches(&commit));
    assert!(query(&hex[..7].to_uppercase()).matches(&commit));
    assert!(!query("ffff").matches(&commit));
}

#[test]
fn a_search_hands_out_only_matches_without_graph_edges() {
    let reader = FakeReader {
        commits: vec![
            named(4, "Add search", "Ann"),
            named(3, "Fix typo", "Bob"),
            named(2, "Search faster", "Bob"),
            named(1, "Initial", "Ann"),
        ],
        ..FakeReader::default()
    };
    let mut pager = LoadHistory::new(Arc::new(reader))
        .search(
            &RepositoryInfo::new("/repo"),
            &HistoryScope::All,
            query("search"),
        )
        .unwrap();
    let rows = pager.next_page(10).unwrap();
    assert_eq!(
        rows.iter().map(|r| r.commit.id).collect::<Vec<_>>(),
        [id(4), id(2)]
    );
    assert!(rows.iter().all(|r| r.graph.edges.is_empty()));
    assert!(pager.is_exhausted());
}

#[test]
fn a_page_stops_at_the_maximum_and_continues_later() {
    let commits = (1..=5).rev().map(|n| named(n, "match", "Ann")).collect();
    let reader = FakeReader {
        commits,
        ..FakeReader::default()
    };
    let mut pager = LoadHistory::new(Arc::new(reader))
        .search(
            &RepositoryInfo::new("/repo"),
            &HistoryScope::All,
            query("match"),
        )
        .unwrap();
    assert_eq!(pager.next_page(2).unwrap().len(), 2);
    assert!(!pager.is_exhausted());
    assert_eq!(pager.next_page(2).unwrap().len(), 2);
    assert_eq!(pager.next_page(2).unwrap().len(), 1);
    assert!(pager.is_exhausted());
}

#[test]
fn a_rare_match_is_found_across_several_short_pages() {
    // More commits than one page looks at, with the only match at the end.
    let total = SEARCH_BATCH * 2 + 10;
    let mut commits: Vec<Commit> = (0..total).map(|_| named(1, "noise", "Ann")).collect();
    commits.push(named(9, "needle", "Ann"));
    let reader = FakeReader {
        commits,
        ..FakeReader::default()
    };
    let mut pager = LoadHistory::new(Arc::new(reader))
        .search(
            &RepositoryInfo::new("/repo"),
            &HistoryScope::All,
            query("needle"),
        )
        .unwrap();
    let mut found = Vec::new();
    let mut pages = 0;
    while !pager.is_exhausted() {
        found.extend(pager.next_page(500).unwrap());
        pages += 1;
    }
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].commit.id, id(9));
    assert!(pages >= 3, "{pages} pages");
}
