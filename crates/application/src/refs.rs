use crate::ports::{ReadError, RepositoryReader};
use domain::{Branch, BranchKind, CommitId, Head, Remote, RepositoryInfo, Tag};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBranches {
    pub remote: Remote,
    /// Sorted by name.
    pub branches: Vec<Branch>,
}

/// All refs of a repository, sorted for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRefs {
    pub head: Head,
    /// Sorted by name.
    pub local: Vec<Branch>,
    /// Sorted by remote name.
    pub remotes: Vec<RemoteBranches>,
    /// Sorted by name.
    pub tags: Vec<Tag>,
}

impl RepositoryRefs {
    pub fn all_branches(&self) -> impl Iterator<Item = &Branch> {
        self.local
            .iter()
            .chain(self.remotes.iter().flat_map(|r| r.branches.iter()))
    }

    pub fn find_branch(&self, refname: &str) -> Option<&Branch> {
        self.all_branches().find(|b| b.refname == refname)
    }
}

#[derive(Clone)]
pub struct LoadBranches {
    reader: Arc<dyn RepositoryReader>,
}

impl LoadBranches {
    pub fn new(reader: Arc<dyn RepositoryReader>) -> Self {
        Self { reader }
    }

    pub fn load(&self, repo: &RepositoryInfo) -> Result<RepositoryRefs, ReadError> {
        let head = self.reader.head(repo)?;
        let mut remotes: Vec<RemoteBranches> = self
            .reader
            .remotes(repo)?
            .into_iter()
            .map(|remote| RemoteBranches {
                remote,
                branches: Vec::new(),
            })
            .collect();
        let mut local = Vec::new();

        for branch in self.reader.branches(repo)? {
            let remote_name = match &branch.kind {
                BranchKind::Local { .. } => {
                    local.push(branch);
                    continue;
                }
                BranchKind::Remote { remote } => remote.clone(),
            };
            match remotes.iter_mut().find(|r| r.remote.name == remote_name) {
                Some(group) => group.branches.push(branch),
                // A remote-tracking branch whose remote is no longer configured.
                None => remotes.push(RemoteBranches {
                    remote: Remote {
                        name: remote_name,
                        url: None,
                    },
                    branches: vec![branch],
                }),
            }
        }

        local.sort_by(|a, b| a.name.cmp(&b.name));
        remotes.sort_by(|a, b| a.remote.name.cmp(&b.remote.name));
        for group in &mut remotes {
            group.branches.sort_by(|a, b| a.name.cmp(&b.name));
        }
        let mut tags = self.reader.tags(repo)?;
        tags.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(RepositoryRefs {
            head,
            local,
            remotes,
            tags,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefLabelKind {
    /// HEAD when it is detached.
    DetachedHead,
    /// A local branch; `is_head` marks the checked-out one.
    Local {
        is_head: bool,
    },
    Remote,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefLabel {
    pub name: String,
    pub kind: RefLabelKind,
}

/// The ref badges of each commit, for the history rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefIndex {
    labels: HashMap<CommitId, Vec<RefLabel>>,
}

impl RefIndex {
    /// Each commit's labels are ordered: detached HEAD, checked-out branch,
    /// other local branches, remote branches, tags.
    pub fn new(refs: &RepositoryRefs) -> Self {
        let mut labels: HashMap<CommitId, Vec<RefLabel>> = HashMap::new();
        let mut add = |id: CommitId, name: &str, kind: RefLabelKind| {
            labels.entry(id).or_default().push(RefLabel {
                name: name.to_owned(),
                kind,
            });
        };
        if let Head::Detached(id) = refs.head {
            add(id, "HEAD", RefLabelKind::DetachedHead);
        }
        let (current, others): (Vec<_>, Vec<_>) = refs.local.iter().partition(|b| b.is_head());
        for branch in current.into_iter().chain(others) {
            add(
                branch.target,
                &branch.name,
                RefLabelKind::Local {
                    is_head: branch.is_head(),
                },
            );
        }
        for branch in refs.remotes.iter().flat_map(|r| &r.branches) {
            add(branch.target, &branch.name, RefLabelKind::Remote);
        }
        for tag in &refs.tags {
            add(tag.target, &tag.name, RefLabelKind::Tag);
        }
        Self { labels }
    }

    pub fn labels(&self, id: &CommitId) -> &[RefLabel] {
        self.labels.get(id).map_or(&[], Vec::as_slice)
    }
}
