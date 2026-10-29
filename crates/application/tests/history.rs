mod common;

use application::{HistoryScope, LoadHistory};
use common::{FakeReader, commit, id};
use domain::RepositoryInfo;
use std::sync::Arc;

fn repo() -> RepositoryInfo {
    RepositoryInfo::new("/repo")
}

#[test]
fn pages_through_the_history_and_reports_the_end() {
    let reader = FakeReader {
        commits: vec![
            commit(5, &[4]),
            commit(4, &[3]),
            commit(3, &[2]),
            commit(2, &[1]),
            commit(1, &[]),
        ],
        ..FakeReader::default()
    };
    let mut pager = LoadHistory::new(Arc::new(reader))
        .open(&repo(), &HistoryScope::All)
        .unwrap();

    let first = pager.next_page(2).unwrap();
    assert_eq!(
        first.iter().map(|r| r.commit.id).collect::<Vec<_>>(),
        [id(5), id(4)]
    );
    assert!(!pager.is_exhausted());

    assert_eq!(pager.next_page(2).unwrap().len(), 2);
    let last = pager.next_page(2).unwrap();
    assert_eq!(last.len(), 1);
    assert!(pager.is_exhausted());
    assert!(pager.next_page(2).unwrap().is_empty());
}

#[test]
fn graph_layout_continues_across_pages() {
    let reader = FakeReader {
        commits: vec![
            commit(4, &[3, 2]),
            commit(3, &[1]),
            commit(2, &[1]),
            commit(1, &[]),
        ],
        ..FakeReader::default()
    };
    let mut pager = LoadHistory::new(Arc::new(reader))
        .open(&repo(), &HistoryScope::All)
        .unwrap();
    let mut rows = pager.next_page(2).unwrap();
    rows.extend(pager.next_page(2).unwrap());
    let nodes: Vec<_> = rows.iter().map(|r| r.graph.node).collect();
    assert_eq!(nodes, [0, 0, 1, 0]);
}

#[test]
fn passes_the_scope_to_the_reader() {
    let reader = Arc::new(FakeReader::default());
    let scope = HistoryScope::Ref("refs/heads/feature".into());
    LoadHistory::new(reader.clone())
        .open(&repo(), &scope)
        .unwrap();
    assert_eq!(*reader.scopes.lock().unwrap(), [scope]);
}
