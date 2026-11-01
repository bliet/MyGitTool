use application::{RemoteBranches, RepositoryRefs};
use domain::{Branch, BranchKind, CommitId, Head, Remote, Stash, Tag, Upstream};
use std::cmp::Ordering;
use std::collections::HashSet;
use ui::view::branch_tree::{
    BranchRowKind, TAGS_KEY, branch_rows, natural_cmp, remote_key, stash_summary,
};

fn local(name: &str, is_head: bool, upstream: Option<Upstream>) -> Branch {
    Branch {
        name: name.into(),
        refname: format!("refs/heads/{name}"),
        target: CommitId::from_bytes([1; 20]),
        kind: BranchKind::Local { is_head, upstream },
    }
}

fn remote(remote: &str, name: &str) -> Branch {
    Branch {
        name: format!("{remote}/{name}"),
        refname: format!("refs/remotes/{remote}/{name}"),
        target: CommitId::from_bytes([2; 20]),
        kind: BranchKind::Remote {
            remote: remote.into(),
        },
    }
}

fn refs() -> RepositoryRefs {
    RepositoryRefs {
        head: Head::Branch {
            name: "main".into(),
            target: CommitId::from_bytes([1; 20]),
        },
        local: vec![
            local("feature/a", false, None),
            local(
                "feature/b",
                false,
                Some(Upstream {
                    name: "origin/feature/b".into(),
                    ahead: None,
                    behind: None,
                }),
            ),
            local(
                "main",
                true,
                Some(Upstream {
                    name: "origin/main".into(),
                    ahead: Some(2),
                    behind: Some(1),
                }),
            ),
        ],
        remotes: vec![RemoteBranches {
            remote: Remote {
                name: "origin".into(),
                url: None,
            },
            branches: vec![remote("origin", "fix/x"), remote("origin", "main")],
        }],
        tags: vec![],
        stashes: vec![],
    }
}

/// Rows rendered as indented text, e.g. `  📁 feature` or `  main*`.
fn render(collapsed: &HashSet<String>) -> Vec<String> {
    branch_rows(&refs(), collapsed)
        .into_iter()
        .map(|row| {
            let indent = "  ".repeat(row.depth);
            match row.kind {
                BranchRowKind::Section(title) => format!("[{title}]"),
                BranchRowKind::Folder {
                    name, collapsed, ..
                } => {
                    format!("{indent}{}{name}", if collapsed { "+" } else { "-" })
                }
                BranchRowKind::Branch(item) => {
                    format!(
                        "{indent}{}{}",
                        item.label,
                        if item.is_head { "*" } else { "" }
                    )
                }
                BranchRowKind::Tag(tag) => format!("{indent}#{}", tag.name),
                BranchRowKind::Stash(stash) => format!("{indent}~{}", stash.name()),
            }
        })
        .collect()
}

#[test]
fn groups_by_prefix_with_folders_first() {
    assert_eq!(
        render(&HashSet::new()),
        [
            "[Local]",
            "-feature",
            "  a",
            "  b",
            "main*",
            "[Remotes]",
            "-origin",
            "  -fix",
            "    x",
            "  main",
        ]
    );
}

#[test]
fn collapsed_folders_hide_their_children() {
    let collapsed: HashSet<String> = ["local:feature".to_owned(), remote_key("origin")].into();
    assert_eq!(
        render(&collapsed),
        ["[Local]", "+feature", "main*", "[Remotes]", "+origin"]
    );
}

#[test]
fn items_carry_tracking_information() {
    let rows = branch_rows(&refs(), &HashSet::new());
    let items: Vec<_> = rows
        .iter()
        .filter_map(|row| match &row.kind {
            BranchRowKind::Branch(item) => Some(item),
            _ => None,
        })
        .collect();
    let main = items.iter().find(|i| i.name == "main").unwrap();
    assert_eq!(
        (main.ahead, main.behind, main.upstream_gone),
        (Some(2), Some(1), false)
    );
    let b = items.iter().find(|i| i.name == "feature/b").unwrap();
    assert!(b.upstream_gone);
    let remote = items.iter().find(|i| i.name == "origin/fix/x").unwrap();
    assert_eq!(remote.label, "x");
    assert_eq!(remote.refname, "refs/remotes/origin/fix/x");
}

#[test]
fn remotes_on_known_hosters_link_to_their_pages() {
    let mut refs = refs();
    refs.remotes[0].remote.url = Some("git@github.com:owner/repo.git".into());
    let rows = branch_rows(&refs, &HashSet::new());
    let folder_web = rows.iter().find_map(|row| match &row.kind {
        BranchRowKind::Folder { name, web, .. } if name == "origin" => Some(web.clone()),
        _ => None,
    });
    assert_eq!(
        folder_web.flatten().map(|w| w.url).as_deref(),
        Some("https://github.com/owner/repo")
    );
    let branch_urls: Vec<String> = rows
        .iter()
        .filter_map(|row| match &row.kind {
            BranchRowKind::Branch(item) => item.web.as_ref().map(|w| w.url.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        branch_urls,
        [
            "https://github.com/owner/repo/tree/fix/x",
            "https://github.com/owner/repo/tree/main"
        ]
    );
}

#[test]
fn local_branches_and_unknown_hosts_have_no_link() {
    let mut refs = refs();
    refs.remotes[0].remote.url = Some("git@example.com:owner/repo.git".into());
    let linked = branch_rows(&refs, &HashSet::new())
        .into_iter()
        .filter(|row| match &row.kind {
            BranchRowKind::Folder { web, .. } => web.is_some(),
            BranchRowKind::Branch(item) => item.web.is_some(),
            BranchRowKind::Section(_) | BranchRowKind::Tag(_) | BranchRowKind::Stash(_) => false,
        })
        .count();
    assert_eq!(linked, 0);
}

fn with_tags(names: &[&str]) -> application::RepositoryRefs {
    let mut refs = refs();
    refs.tags = names
        .iter()
        .map(|name| Tag {
            name: (*name).into(),
            target: CommitId::from_bytes([3; 20]),
        })
        .collect();
    refs
}

fn tag_lines(refs: &application::RepositoryRefs, collapsed: &HashSet<String>) -> Vec<String> {
    let rows = branch_rows(refs, collapsed);
    let start = rows
        .iter()
        .position(|row| row.kind == BranchRowKind::Section("Tags"))
        .unwrap();
    rows[start + 1..]
        .iter()
        .map(|row| match &row.kind {
            BranchRowKind::Folder {
                name, collapsed, ..
            } => {
                format!("{}{name}", if *collapsed { "+" } else { "-" })
            }
            BranchRowKind::Tag(tag) => format!("  {}", tag.name),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn tags_come_last_newest_version_first() {
    let refs = with_tags(&["v1.9", "v1.10", "v1.2", "beta"]);
    assert_eq!(
        tag_lines(&refs, &HashSet::new()),
        ["-4 tags", "  v1.10", "  v1.9", "  v1.2", "  beta"]
    );
}

#[test]
fn the_tags_folder_collapses() {
    let refs = with_tags(&["v1"]);
    let collapsed: HashSet<String> = [TAGS_KEY.to_owned()].into();
    assert_eq!(tag_lines(&refs, &collapsed), ["+1 tag"]);
}

#[test]
fn no_tags_no_section() {
    let rows = branch_rows(&refs(), &HashSet::new());
    assert!(
        !rows
            .iter()
            .any(|r| r.kind == BranchRowKind::Section("Tags"))
    );
}

#[test]
fn natural_order_compares_numbers_by_value() {
    assert_eq!(natural_cmp("v1.10", "v1.9"), Ordering::Greater);
    assert_eq!(natural_cmp("v2", "v10"), Ordering::Less);
    assert_eq!(natural_cmp("v01", "v1"), Ordering::Equal);
    assert_eq!(natural_cmp("a", "b"), Ordering::Less);
    assert_eq!(natural_cmp("v1", "v1.0"), Ordering::Less);
}

#[test]
fn stashes_come_last_newest_first() {
    let mut refs = with_tags(&["v1"]);
    refs.stashes = (0..2)
        .map(|index| Stash {
            index,
            message: format!("On main: stash {index}"),
            id: CommitId::from_bytes([10 + index as u8; 20]),
        })
        .collect();
    let rows = branch_rows(&refs, &HashSet::new());
    let tail: Vec<_> = rows[rows.len() - 3..]
        .iter()
        .map(|row| match &row.kind {
            BranchRowKind::Section(title) => (*title).to_owned(),
            BranchRowKind::Stash(stash) => stash.name(),
            other => panic!("unexpected row {other:?}"),
        })
        .collect();
    assert_eq!(tail, ["Stashes", "stash@{0}", "stash@{1}"]);
}

#[test]
fn no_stashes_means_no_stash_section() {
    let rows = branch_rows(&refs(), &HashSet::new());
    assert!(
        !rows
            .iter()
            .any(|row| row.kind == BranchRowKind::Section("Stashes"))
    );
}

#[test]
fn stash_rows_show_the_description_and_the_branch() {
    assert_eq!(
        stash_summary("On feature/x: half done"),
        ("half done".to_owned(), Some("feature/x".to_owned()))
    );
    assert_eq!(
        stash_summary("WIP on main: 1a2b3c4 Fix: the parser"),
        (
            "1a2b3c4 Fix: the parser".to_owned(),
            Some("main".to_owned())
        )
    );
    assert_eq!(stash_summary("custom"), ("custom".to_owned(), None));
}
