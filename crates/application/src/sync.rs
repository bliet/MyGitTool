//! Fetch, pull and push, and aborting a pull that stopped with conflicts.

use crate::ports::{
    CredentialProvider, Integration, ProgressReporter, ReadError, RemoteError, RemoteOperations,
    RepositoryReader, RepositoryWriter, WriteError,
};
use domain::{CommitId, DirtyWorkTree, FastForward, Head, OngoingOperation, RepositoryInfo};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SyncError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error(transparent)]
    Remote(#[from] RemoteError),
    #[error(transparent)]
    DirtyWorkTree(#[from] DirtyWorkTree),
    #[error("HEAD is not on a branch with commits; check out a branch first")]
    NoCurrentBranch,
    #[error("the branch {0} has no upstream")]
    NoUpstream(String),
    #[error("the repository is in the middle of a {0}; finish or abort it first")]
    OperationInProgress(OngoingOperation),
    #[error("{0} and its upstream have diverged, and pull.ff only allows fast-forwards")]
    NotFastForward(String),
    #[error("the upstream {0} does not exist on the remote")]
    UpstreamGone(String),
    #[error("the repository has no remotes")]
    NoRemotes,
}

/// What to fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchTarget {
    /// Every remote.
    All,
    Remote(String),
    /// The remote of this local branch's upstream.
    Branch(String),
}

/// Fetches remotes.
#[derive(Clone)]
pub struct Fetch {
    reader: Arc<dyn RepositoryReader>,
    remote: Arc<dyn RemoteOperations>,
}

impl Fetch {
    pub fn new(reader: Arc<dyn RepositoryReader>, remote: Arc<dyn RemoteOperations>) -> Self {
        Self { reader, remote }
    }

    /// The remotes that `target` covers.
    pub fn remotes(
        &self,
        repo: &RepositoryInfo,
        target: &FetchTarget,
    ) -> Result<Vec<String>, SyncError> {
        Ok(match target {
            FetchTarget::All => {
                let remotes: Vec<_> = self
                    .reader
                    .remotes(repo)?
                    .into_iter()
                    .map(|r| r.name)
                    .collect();
                if remotes.is_empty() {
                    return Err(SyncError::NoRemotes);
                }
                remotes
            }
            FetchTarget::Remote(name) => vec![name.clone()],
            FetchTarget::Branch(branch) => match self.reader.tracking(repo, branch)? {
                Some(tracking) => vec![tracking.remote],
                None => return Err(SyncError::NoUpstream(branch.clone())),
            },
        })
    }

    /// Fetches the remotes of `target` one after the other and returns their
    /// names. Stops at the first failure.
    pub fn fetch(
        &self,
        repo: &RepositoryInfo,
        target: &FetchTarget,
        progress: &dyn ProgressReporter,
        credentials: &dyn CredentialProvider,
    ) -> Result<Vec<String>, SyncError> {
        let remotes = self.remotes(repo, target)?;
        for remote in &remotes {
            self.remote.fetch(repo, remote, progress, credentials)?;
        }
        Ok(remotes)
    }
}

/// What a pull did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullOutcome {
    UpToDate,
    FastForwarded {
        to: CommitId,
    },
    Merged,
    Rebased,
    /// The merge or rebase stopped with conflicts and is still in progress:
    /// abort it with [`AbortOperation`] or resolve it outside the app.
    Conflicts {
        operation: OngoingOperation,
        files: Vec<String>,
    },
}

/// Pulls the current branch: fetches its upstream's remote, then
/// fast-forwards, merges or rebases as `pull.rebase` and `pull.ff` say.
#[derive(Clone)]
pub struct Pull {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
    remote: Arc<dyn RemoteOperations>,
}

impl Pull {
    pub fn new(
        reader: Arc<dyn RepositoryReader>,
        writer: Arc<dyn RepositoryWriter>,
        remote: Arc<dyn RemoteOperations>,
    ) -> Self {
        Self {
            reader,
            writer,
            remote,
        }
    }

    pub fn pull(
        &self,
        repo: &RepositoryInfo,
        progress: &dyn ProgressReporter,
        credentials: &dyn CredentialProvider,
    ) -> Result<PullOutcome, SyncError> {
        let Head::Branch { name, target: head } = self.reader.head(repo)? else {
            return Err(SyncError::NoCurrentBranch);
        };
        if let Some(operation) = self.reader.ongoing_operation(repo)? {
            return Err(SyncError::OperationInProgress(operation));
        }
        let tracking = self
            .reader
            .tracking(repo, &name)?
            .ok_or_else(|| SyncError::NoUpstream(name.clone()))?;
        let files = self.reader.dirty_files(repo)?;
        if !files.is_empty() {
            return Err(DirtyWorkTree { files }.into());
        }

        self.remote
            .fetch(repo, &tracking.remote, progress, credentials)?;
        let upstream = self
            .reader
            .resolve(repo, &tracking.tracking_ref)?
            .ok_or_else(|| SyncError::UpstreamGone(tracking.merge_ref.clone()))?;
        let (ahead, behind) = self.reader.ahead_behind(repo, head, upstream)?;
        if behind == 0 {
            return Ok(PullOutcome::UpToDate);
        }

        let config = self.reader.pull_config(repo, &name)?;
        let fast_forward =
            ahead == 0 && (config.rebase || config.fast_forward != FastForward::Never);
        if fast_forward {
            self.writer.fast_forward(repo, &name, upstream)?;
            return Ok(PullOutcome::FastForwarded { to: upstream });
        }
        if ahead > 0 && config.fast_forward == FastForward::Only {
            return Err(SyncError::NotFastForward(name));
        }

        if config.rebase {
            return Ok(match self.writer.rebase(repo, upstream)? {
                Integration::Completed => PullOutcome::Rebased,
                Integration::Conflicts(files) => PullOutcome::Conflicts {
                    operation: OngoingOperation::Rebase,
                    files,
                },
            });
        }
        let message = self.merge_message(repo, &tracking.remote, &tracking.merge_ref)?;
        Ok(match self.writer.merge(repo, upstream, &message)? {
            Integration::Completed => PullOutcome::Merged,
            Integration::Conflicts(files) => PullOutcome::Conflicts {
                operation: OngoingOperation::Merge,
                files,
            },
        })
    }

    /// `Merge branch 'main' of <url>`, as `git pull` writes it.
    fn merge_message(
        &self,
        repo: &RepositoryInfo,
        remote: &str,
        merge_ref: &str,
    ) -> Result<String, SyncError> {
        let branch = merge_ref.strip_prefix("refs/heads/").unwrap_or(merge_ref);
        let source = self
            .reader
            .remotes(repo)?
            .into_iter()
            .find(|r| r.name == remote)
            .and_then(|r| r.url)
            .unwrap_or_else(|| remote.to_owned());
        Ok(format!("Merge branch '{branch}' of {source}"))
    }
}

/// Where a push goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushDestination {
    pub remote: String,
    /// The branch on the remote, e.g. `refs/heads/main`.
    pub remote_ref: String,
    /// Makes the destination the branch's upstream after the push.
    pub set_upstream: bool,
}

/// Where a branch would be pushed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushPlan {
    /// To its upstream.
    Upstream(PushDestination),
    /// The branch has no upstream. These remotes could become one, the
    /// preferred one (`origin`, or the only one) first.
    NoUpstream {
        branch: String,
        remotes: Vec<String>,
    },
}

/// Pushes a local branch. There is no force push.
#[derive(Clone)]
pub struct Push {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
    remote: Arc<dyn RemoteOperations>,
}

impl Push {
    pub fn new(
        reader: Arc<dyn RepositoryReader>,
        writer: Arc<dyn RepositoryWriter>,
        remote: Arc<dyn RemoteOperations>,
    ) -> Self {
        Self {
            reader,
            writer,
            remote,
        }
    }

    /// The current branch, which the toolbar pushes.
    pub fn current_branch(&self, repo: &RepositoryInfo) -> Result<String, SyncError> {
        match self.reader.head(repo)? {
            Head::Branch { name, .. } => Ok(name),
            Head::Detached(_) | Head::Unborn { .. } => Err(SyncError::NoCurrentBranch),
        }
    }

    pub fn plan(&self, repo: &RepositoryInfo, branch: &str) -> Result<PushPlan, SyncError> {
        if let Some(tracking) = self.reader.tracking(repo, branch)? {
            return Ok(PushPlan::Upstream(PushDestination {
                remote: tracking.remote,
                remote_ref: tracking.merge_ref,
                set_upstream: false,
            }));
        }
        let mut remotes: Vec<String> = self
            .reader
            .remotes(repo)?
            .into_iter()
            .map(|r| r.name)
            .collect();
        if remotes.is_empty() {
            return Err(SyncError::NoRemotes);
        }
        if let Some(origin) = remotes.iter().position(|r| r == "origin") {
            let origin = remotes.remove(origin);
            remotes.insert(0, origin);
        }
        Ok(PushPlan::NoUpstream {
            branch: branch.to_owned(),
            remotes,
        })
    }

    /// The destination that makes a branch without upstream track the
    /// branch of the same name on `remote`.
    pub fn new_upstream(branch: &str, remote: &str) -> PushDestination {
        PushDestination {
            remote: remote.to_owned(),
            remote_ref: format!("refs/heads/{branch}"),
            set_upstream: true,
        }
    }

    pub fn push(
        &self,
        repo: &RepositoryInfo,
        branch: &str,
        destination: &PushDestination,
        progress: &dyn ProgressReporter,
        credentials: &dyn CredentialProvider,
    ) -> Result<(), SyncError> {
        self.remote.push(
            repo,
            branch,
            &destination.remote,
            &destination.remote_ref,
            progress,
            credentials,
        )?;
        if destination.set_upstream {
            self.writer
                .set_tracking(repo, branch, &destination.remote, &destination.remote_ref)?;
        }
        Ok(())
    }
}

/// Aborts the merge or rebase a pull left in progress.
#[derive(Clone)]
pub struct AbortOperation {
    writer: Arc<dyn RepositoryWriter>,
}

impl AbortOperation {
    pub fn new(writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { writer }
    }

    pub fn abort(&self, repo: &RepositoryInfo) -> Result<(), SyncError> {
        Ok(self.writer.abort(repo)?)
    }
}
