//! The branch tree of the left column, flattened into rows.

use application::RepositoryRefs;
use domain::{Branch, BranchKind, CommitId, WebLink, web_link};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchItem {
    /// The last name segment, e.g. `x` for `feature/x`.
    pub label: String,
    /// The full short name, e.g. `origin/feature/x`.
    pub name: String,
    pub refname: String,
    pub target: CommitId,
    pub is_head: bool,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    /// The upstream is configured but its ref no longer exists.
    pub upstream_gone: bool,
    /// The remote of a remote branch; `None` for a local branch.
    pub remote: Option<String>,
    /// The branch's page on its hoster, for a remote branch on a known one.
    pub web: Option<WebLink>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagItem {
    pub name: String,
    pub target: CommitId,
}

impl TagItem {
    pub fn refname(&self) -> String {
        format!("refs/tags/{}", self.name)
    }
}

/// The collapse key of the tags folder. It starts collapsed, as a project
/// can have hundreds of tags.
pub const TAGS_KEY: &str = "tags";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchRowKind {
    Section(&'static str),
    /// A `/`-prefix group or a remote. `key` identifies it for collapsing.
    Folder {
        key: String,
        name: String,
        collapsed: bool,
        /// The repository's page, for a remote on a known hoster.
        web: Option<WebLink>,
    },
    Branch(BranchItem),
    Tag(TagItem),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRow {
    pub depth: usize,
    pub kind: BranchRowKind,
}

/// The collapse key of a remote's folder.
pub fn remote_key(remote: &str) -> String {
    format!("remote:{remote}")
}

/// The remote whose top-level folder has the collapse key `key`; `None`
/// for any other folder.
pub fn remote_of_folder(key: &str) -> Option<&str> {
    key.strip_prefix("remote:")
        .filter(|name| !name.contains(':'))
}

/// Local branches grouped by `/` prefix, then each remote with its branches
/// grouped the same way, then the tags in one folder, newest version first.
/// Folders come before branches; both are sorted. Children of collapsed
/// folders (by key) are left out.
pub fn branch_rows(refs: &RepositoryRefs, collapsed: &HashSet<String>) -> Vec<BranchRow> {
    let mut rows = vec![BranchRow {
        depth: 0,
        kind: BranchRowKind::Section("Local"),
    }];
    let local: Vec<_> = refs.local.iter().map(|b| (b.name.as_str(), b)).collect();
    push_tree(&mut rows, &build(&local), "local:", 0, collapsed, None);

    if !refs.remotes.is_empty() {
        rows.push(BranchRow {
            depth: 0,
            kind: BranchRowKind::Section("Remotes"),
        });
    }
    for group in &refs.remotes {
        let key = remote_key(&group.remote.name);
        let is_collapsed = collapsed.contains(&key);
        rows.push(BranchRow {
            depth: 0,
            kind: BranchRowKind::Folder {
                key: key.clone(),
                name: group.remote.name.clone(),
                collapsed: is_collapsed,
                web: group
                    .remote
                    .url
                    .as_deref()
                    .and_then(|url| web_link(url, None)),
            },
        });
        if !is_collapsed {
            let branches: Vec<_> = group
                .branches
                .iter()
                .map(|b| (b.name_on_remote(), b))
                .collect();
            push_tree(
                &mut rows,
                &build(&branches),
                &format!("{key}:"),
                1,
                collapsed,
                group.remote.url.as_deref(),
            );
        }
    }
    if !refs.tags.is_empty() {
        rows.push(BranchRow {
            depth: 0,
            kind: BranchRowKind::Section("Tags"),
        });
        let is_collapsed = collapsed.contains(TAGS_KEY);
        let count = match refs.tags.len() {
            1 => "1 tag".to_owned(),
            n => format!("{n} tags"),
        };
        rows.push(BranchRow {
            depth: 0,
            kind: BranchRowKind::Folder {
                key: TAGS_KEY.to_owned(),
                name: count,
                collapsed: is_collapsed,
                web: None,
            },
        });
        if !is_collapsed {
            let mut tags: Vec<_> = refs.tags.iter().collect();
            tags.sort_by(|a, b| natural_cmp(&b.name, &a.name));
            rows.extend(tags.into_iter().map(|tag| BranchRow {
                depth: 1,
                kind: BranchRowKind::Tag(TagItem {
                    name: tag.name.clone(),
                    target: tag.target,
                }),
            }));
        }
    }
    rows
}

/// Compares names with runs of digits as numbers, so `v1.10` comes after
/// `v1.9`.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (a, b);
    loop {
        let (Some(x), Some(y)) = (a.chars().next(), b.chars().next()) else {
            return a.len().cmp(&b.len());
        };
        if x.is_ascii_digit() && y.is_ascii_digit() {
            let end_a = a.find(|c: char| !c.is_ascii_digit()).unwrap_or(a.len());
            let end_b = b.find(|c: char| !c.is_ascii_digit()).unwrap_or(b.len());
            let (digits_a, digits_b) = (
                a[..end_a].trim_start_matches('0'),
                b[..end_b].trim_start_matches('0'),
            );
            let order = digits_a
                .len()
                .cmp(&digits_b.len())
                .then_with(|| digits_a.cmp(digits_b));
            if order.is_ne() {
                return order;
            }
            a = &a[end_a..];
            b = &b[end_b..];
        } else {
            if x != y {
                return x.cmp(&y);
            }
            a = &a[x.len_utf8()..];
            b = &b[y.len_utf8()..];
        }
    }
}

#[derive(Default)]
struct Node<'a> {
    folders: BTreeMap<&'a str, Node<'a>>,
    branches: BTreeMap<&'a str, &'a Branch>,
}

fn build<'a>(branches: &[(&'a str, &'a Branch)]) -> Node<'a> {
    let mut root = Node::default();
    for &(path, branch) in branches {
        let mut node = &mut root;
        let mut segments = path.split('/').peekable();
        while let Some(segment) = segments.next() {
            if segments.peek().is_none() {
                node.branches.insert(segment, branch);
            } else {
                node = node.folders.entry(segment).or_default();
            }
        }
    }
    root
}

fn push_tree(
    rows: &mut Vec<BranchRow>,
    node: &Node<'_>,
    prefix: &str,
    depth: usize,
    collapsed: &HashSet<String>,
    remote_url: Option<&str>,
) {
    for (name, child) in &node.folders {
        let key = format!("{prefix}{name}");
        let is_collapsed = collapsed.contains(&key);
        rows.push(BranchRow {
            depth,
            kind: BranchRowKind::Folder {
                key: key.clone(),
                name: (*name).to_owned(),
                collapsed: is_collapsed,
                web: None,
            },
        });
        if !is_collapsed {
            push_tree(
                rows,
                child,
                &format!("{key}/"),
                depth + 1,
                collapsed,
                remote_url,
            );
        }
    }
    for (label, branch) in &node.branches {
        rows.push(BranchRow {
            depth,
            kind: BranchRowKind::Branch(item(label, branch, remote_url)),
        });
    }
}

fn item(label: &str, branch: &Branch, remote_url: Option<&str>) -> BranchItem {
    let remote = match &branch.kind {
        BranchKind::Remote { remote } => Some(remote.clone()),
        BranchKind::Local { .. } => None,
    };
    let (is_head, ahead, behind, upstream_gone) = match &branch.kind {
        BranchKind::Local { is_head, upstream } => match upstream {
            Some(up) => (*is_head, up.ahead, up.behind, up.ahead.is_none()),
            None => (*is_head, None, None, false),
        },
        BranchKind::Remote { .. } => (false, None, None, false),
    };
    BranchItem {
        label: label.to_owned(),
        name: branch.name.clone(),
        refname: branch.refname.clone(),
        target: branch.target,
        is_head,
        ahead,
        behind,
        upstream_gone,
        web: remote
            .as_ref()
            .and(remote_url)
            .and_then(|url| web_link(url, Some(branch.name_on_remote()))),
        remote,
    }
}
