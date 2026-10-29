mod common;

use application::{DEFAULT_CAPACITY, RecentRepositories};
use common::FakeStore;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn paths(items: &[&str]) -> Vec<PathBuf> {
    items.iter().map(PathBuf::from).collect()
}

#[test]
fn list_is_empty_initially() {
    let recent = RecentRepositories::new(Arc::new(FakeStore::default()));
    assert!(recent.list().unwrap().is_empty());
}

#[test]
fn record_puts_the_path_first_and_persists_it() {
    let store = Arc::new(FakeStore::with_paths(["/a", "/b"]));
    let recent = RecentRepositories::new(store.clone());

    let list = recent.record(Path::new("/c")).unwrap();

    assert_eq!(list, paths(&["/c", "/a", "/b"]));
    assert_eq!(store.stored(), list);
}

#[test]
fn record_moves_an_existing_path_to_the_front_without_duplicating_it() {
    let store = Arc::new(FakeStore::with_paths(["/a", "/b", "/c"]));
    let recent = RecentRepositories::new(store);
    assert_eq!(
        recent.record(Path::new("/b")).unwrap(),
        paths(&["/b", "/a", "/c"])
    );
}

#[test]
fn record_drops_the_oldest_entries_beyond_capacity() {
    let store = Arc::new(FakeStore::with_paths(["/a", "/b"]));
    let recent = RecentRepositories::with_capacity(store, 2);
    assert_eq!(
        recent.record(Path::new("/c")).unwrap(),
        paths(&["/c", "/a"])
    );
}

#[test]
fn list_is_capped_even_if_the_store_holds_more() {
    let many: Vec<String> = (0..DEFAULT_CAPACITY + 5)
        .map(|i| format!("/r{i}"))
        .collect();
    let recent = RecentRepositories::new(Arc::new(FakeStore::with_paths(many)));
    assert_eq!(recent.list().unwrap().len(), DEFAULT_CAPACITY);
}

#[test]
fn forget_removes_the_path() {
    let store = Arc::new(FakeStore::with_paths(["/a", "/b"]));
    let recent = RecentRepositories::new(store.clone());
    assert_eq!(recent.forget(Path::new("/a")).unwrap(), paths(&["/b"]));
    assert_eq!(store.stored(), paths(&["/b"]));
}

#[test]
fn record_reports_a_failing_store() {
    let recent = RecentRepositories::new(Arc::new(FakeStore::default().failing_saves()));
    let err = recent.record(Path::new("/a")).unwrap_err();
    assert_eq!(err.to_string(), "cannot write: disk full");
}
