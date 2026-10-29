use crate::commit::CommitId;

/// The tracking branch of a local branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    /// Short name, e.g. `origin/main`.
    pub name: String,
    /// Commits on the local branch that the upstream lacks, and vice versa.
    /// Both are `None` if the upstream ref no longer exists ("gone").
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchKind {
    Local {
        is_head: bool,
        upstream: Option<Upstream>,
    },
    Remote {
        /// The remote's name, e.g. `origin`.
        remote: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// Short name: `feature/x` for a local branch, `origin/feature/x` for a
    /// remote one.
    pub name: String,
    /// Full ref name, e.g. `refs/heads/feature/x`.
    pub refname: String,
    pub target: CommitId,
    pub kind: BranchKind,
}

impl Branch {
    pub fn is_local(&self) -> bool {
        matches!(self.kind, BranchKind::Local { .. })
    }

    pub fn is_head(&self) -> bool {
        matches!(self.kind, BranchKind::Local { is_head: true, .. })
    }

    /// For a remote branch, the name without the remote prefix.
    pub fn name_on_remote(&self) -> &str {
        match &self.kind {
            BranchKind::Remote { remote } => self
                .name
                .strip_prefix(remote.as_str())
                .and_then(|rest| rest.strip_prefix('/'))
                .unwrap_or(&self.name),
            BranchKind::Local { .. } => &self.name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    /// The commit the tag points to (annotated tags are peeled).
    pub target: CommitId,
}

/// What HEAD points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    Branch {
        name: String,
        target: CommitId,
    },
    Detached(CommitId),
    /// A branch without commits yet, as in a freshly initialized repository.
    Unborn {
        name: String,
    },
}

impl Head {
    pub fn target(&self) -> Option<CommitId> {
        match self {
            Self::Branch { target, .. } | Self::Detached(target) => Some(*target),
            Self::Unborn { .. } => None,
        }
    }
}
