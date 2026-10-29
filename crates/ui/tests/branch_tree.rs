use application::{RemoteBranches, RepositoryRefs};
use domain::{Branch, BranchKind, CommitId, Head, Remote, Upstream};
use std::collections::HashSet;
use ui::view::branch_tree::{BranchRowKind, branch_rows, remote_key};

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
