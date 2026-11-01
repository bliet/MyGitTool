mod common;

use application::{LoadBranches, RefIndex, RefLabelKind};
use common::{FakeReader, id, local, remote_branch};
use domain::{Head, Remote, RepositoryInfo, Tag};
use std::sync::Arc;

fn remote(name: &str) -> Remote {
    Remote {
        name: name.into(),
        url: Some(format!("git@example.com:{name}.git")),
    }
}

fn reader() -> FakeReader {
    FakeReader {
        head: Head::Branch {
            name: "main".into(),
            target: id(1),
        },
        branches: vec![
            remote_branch("upstream", "main", 1),
            local("zeta", 2, false),
            remote_branch("origin", "main", 1),
            local("main", 1, true),
            remote_branch("gone", "old", 3),
            remote_branch("origin", "dev", 2),
        ],
        remotes: vec![remote("upstream"), remote("origin"), remote("empty")],
        tags: vec![
            Tag {
                name: "v2".into(),
                target: id(2),
            },
            Tag {
                name: "v1".into(),
                target: id(1),
            },
        ],
        ..FakeReader::default()
    }
}

#[test]
fn groups_and_sorts_branches() {
    let refs = LoadBranches::new(Arc::new(reader()))
        .load(&RepositoryInfo::new("/r"))
        .unwrap();

    let local: Vec<_> = refs.local.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(local, ["main", "zeta"]);

    let remotes: Vec<_> = refs
        .remotes
        .iter()
        .map(|r| {
            let branches: Vec<_> = r.branches.iter().map(|b| b.name.as_str()).collect();
            (r.remote.name.as_str(), branches)
        })
        .collect();
    assert_eq!(
        remotes,
        [
            ("empty", vec![]),
            ("gone", vec!["gone/old"]),
            ("origin", vec!["origin/dev", "origin/main"]),
            ("upstream", vec!["upstream/main"]),
        ]
    );
    assert_eq!(
        refs.remotes[1].remote.url, None,
        "unconfigured remote has no URL"
    );

    let tags: Vec<_> = refs.tags.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(tags, ["v1", "v2"]);
    assert_eq!(
        refs.find_branch("refs/remotes/origin/dev")
            .map(|b| b.target),
        Some(id(2))
    );
}

#[test]
fn ref_index_orders_labels_per_commit() {
    let refs = LoadBranches::new(Arc::new(reader()))
        .load(&RepositoryInfo::new("/r"))
        .unwrap();
    let index = RefIndex::new(&refs);

    let labels: Vec<_> = index
        .labels(&id(1))
        .iter()
        .map(|l| (l.name.as_str(), l.kind))
        .collect();
    assert_eq!(
        labels,
        [
            ("main", RefLabelKind::Local { is_head: true }),
            ("origin/main", RefLabelKind::Remote),
            ("upstream/main", RefLabelKind::Remote),
            ("v1", RefLabelKind::Tag),
        ]
    );
    assert!(index.labels(&id(9)).is_empty());
}

#[test]
fn detached_head_gets_its_own_label() {
    let reader = FakeReader {
        head: Head::Detached(id(7)),
        ..FakeReader::default()
    };
    let refs = LoadBranches::new(Arc::new(reader))
        .load(&RepositoryInfo::new("/r"))
        .unwrap();
    let index = RefIndex::new(&refs);
    assert_eq!(index.labels(&id(7))[0].kind, RefLabelKind::DetachedHead);
}

#[test]
fn stashes_come_newest_first() {
    let reader = FakeReader {
        stashes: vec![common::stash(1, 11), common::stash(0, 10)],
        ..reader()
    };
    let refs = LoadBranches::new(Arc::new(reader))
        .load(&RepositoryInfo::new("/r"))
        .unwrap();
    let names: Vec<_> = refs.stashes.iter().map(|s| s.name()).collect();
    assert_eq!(names, ["stash@{0}", "stash@{1}"]);
}
