//! Checking out, creating, deleting, merging and rebasing branches, and
//! continuing or inspecting a merge or rebase in progress.

use crate::ports::{
    CredentialProvider, Integration, ProgressReporter, ReadError, RemoteOperations,
    RepositoryReader, RepositoryWriter,
};
use crate::sync::{IntegrationOutcome, OperationError};
use domain::{
    CommitId, DirtyWorkTree, Head, OngoingOperation, RepositoryInfo, validate_branch_name,
};
use std::sync::Arc;

fn local_ref(branch: &str) -> String {
    format!("refs/heads/{branch}")
}

/// The checked-out branch and its tip.
fn current(
    reader: &dyn RepositoryReader,
    repo: &RepositoryInfo,
) -> Result<(String, CommitId), OperationError> {
    match reader.head(repo)? {
        Head::Branch { name, target } => Ok((name, target)),
        Head::Detached(_) | Head::Unborn { .. } => Err(OperationError::NoCurrentBranch),
    }
}

fn ensure_idle(reader: &dyn RepositoryReader, repo: &RepositoryInfo) -> Result<(), OperationError> {
    match reader.ongoing_operation(repo)? {
        Some(operation) => Err(OperationError::OperationInProgress(operation)),
        None => Ok(()),
    }
}

fn ensure_clean(
    reader: &dyn RepositoryReader,
    repo: &RepositoryInfo,
) -> Result<(), OperationError> {
    let files = reader.dirty_files(repo)?;
    if files.is_empty() {
        Ok(())
    } else {
        Err(DirtyWorkTree { files }.into())
    }
}

/// Refuses a switch to `target` that would touch locally modified files.
/// Changes to other files are carried over, as `git switch` does.
fn ensure_switchable(
    reader: &dyn RepositoryReader,
    repo: &RepositoryInfo,
    target: CommitId,
) -> Result<(), OperationError> {
    ensure_idle(reader, repo)?;
    let dirty = reader.dirty_files(repo)?;
    if dirty.is_empty() {
        return Ok(());
    }
    let touched = match reader.head(repo)?.target() {
        Some(head) => {
            let changed = reader.changed_paths(repo, head, target)?;
            dirty
                .into_iter()
                .filter(|file| changed.binary_search(file).is_ok())
                .collect()
        }
        // Without a commit to compare with, any change may be touched.
        None => dirty,
    };
    if touched.is_empty() {
        Ok(())
    } else {
        Err(DirtyWorkTree { files: touched }.into())
    }
}

fn resolve(
    reader: &dyn RepositoryReader,
    repo: &RepositoryInfo,
    refname: &str,
) -> Result<Option<CommitId>, ReadError> {
    reader.resolve(repo, refname)
}

/// The branch to check out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutTarget {
    Local(String),
    /// A remote branch: switches to the local branch of the same name that
    /// tracks it, creating it if needed.
    Remote {
        remote: String,
        branch: String,
    },
}

#[derive(Clone)]
pub struct Checkout {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
}

impl Checkout {
    pub fn new(reader: Arc<dyn RepositoryReader>, writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { reader, writer }
    }

    /// Checks out `target` and returns the local branch that is now current.
    pub fn checkout(
        &self,
        repo: &RepositoryInfo,
        target: &CheckoutTarget,
    ) -> Result<String, OperationError> {
        let reader = self.reader.as_ref();
        let (local, tip, create) = match target {
            CheckoutTarget::Local(name) => {
                let tip = resolve(reader, repo, &local_ref(name))?
                    .ok_or_else(|| OperationError::BranchNotFound(name.clone()))?;
                (name.clone(), tip, None)
            }
            CheckoutTarget::Remote { remote, branch } => {
                let tip = resolve(reader, repo, &format!("refs/remotes/{remote}/{branch}"))?
                    .ok_or_else(|| OperationError::BranchNotFound(format!("{remote}/{branch}")))?;
                match resolve(reader, repo, &local_ref(branch))? {
                    Some(existing) => {
                        let tracks_other = reader.tracking(repo, branch)?.is_some_and(|t| {
                            t.remote != *remote || t.merge_ref != local_ref(branch)
                        });
                        if tracks_other {
                            return Err(OperationError::BranchExists(branch.clone()));
                        }
                        (branch.clone(), existing, None)
                    }
                    None => (branch.clone(), tip, Some(remote.clone())),
                }
            }
        };
        if matches!(reader.head(repo)?, Head::Branch { name, .. } if name == local) {
            return Ok(local);
        }
        ensure_switchable(reader, repo, tip)?;
        if let Some(remote) = create {
            self.writer.create_branch(repo, &local, tip)?;
            self.writer
                .set_tracking(repo, &local, &remote, &local_ref(&local))?;
        }
        self.writer.checkout(repo, &local)?;
        Ok(local)
    }
}

#[derive(Clone)]
pub struct CreateBranch {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
}

impl CreateBranch {
    pub fn new(reader: Arc<dyn RepositoryReader>, writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { reader, writer }
    }

    /// Checks a proposed name before anything is created, e.g. while the
    /// user types it.
    pub fn check_name(&self, repo: &RepositoryInfo, name: &str) -> Result<(), OperationError> {
        validate_branch_name(name)?;
        if resolve(self.reader.as_ref(), repo, &local_ref(name))?.is_some() {
            return Err(OperationError::BranchExists(name.to_owned()));
        }
        Ok(())
    }

    /// Creates `name` at `start`, and switches to it if `checkout` is set.
    pub fn create(
        &self,
        repo: &RepositoryInfo,
        name: &str,
        start: CommitId,
        checkout: bool,
    ) -> Result<(), OperationError> {
        self.check_name(repo, name)?;
        if checkout {
            ensure_switchable(self.reader.as_ref(), repo, start)?;
        }
        self.writer.create_branch(repo, name, start)?;
        if checkout {
            self.writer.checkout(repo, name)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct DeleteBranch {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
    remote: Arc<dyn RemoteOperations>,
}

impl DeleteBranch {
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

    /// Deletes a local branch. Unless `force` is set, a branch with commits
    /// that the current branch lacks is refused with
    /// [`OperationError::Unmerged`].
    pub fn delete_local(
        &self,
        repo: &RepositoryInfo,
        name: &str,
        force: bool,
    ) -> Result<(), OperationError> {
        let head = self.reader.head(repo)?;
        if matches!(&head, Head::Branch { name: current, .. } if current == name) {
            return Err(OperationError::CannotDeleteCurrent(name.to_owned()));
        }
        let tip = resolve(self.reader.as_ref(), repo, &local_ref(name))?
            .ok_or_else(|| OperationError::BranchNotFound(name.to_owned()))?;
        if !force {
            let unmerged = match head.target() {
                Some(head) => self.reader.ahead_behind(repo, tip, head)?.0,
                None => 1,
            };
            if unmerged > 0 {
                return Err(OperationError::Unmerged {
                    branch: name.to_owned(),
                    commits: unmerged,
                });
            }
        }
        Ok(self.writer.delete_branch(repo, name)?)
    }

    /// Deletes `branch` on `remote`, e.g. `feature/x` on `origin`.
    pub fn delete_remote(
        &self,
        repo: &RepositoryInfo,
        remote: &str,
        branch: &str,
        progress: &dyn ProgressReporter,
        credentials: &dyn CredentialProvider,
    ) -> Result<(), OperationError> {
        Ok(self
            .remote
            .delete_branch(repo, remote, &local_ref(branch), progress, credentials)?)
    }
}

/// The message of a merge commit, as `git merge` writes it.
pub fn merge_message(refname: &str) -> String {
    if let Some(branch) = refname.strip_prefix("refs/heads/") {
        format!("Merge branch '{branch}'")
    } else if let Some(branch) = refname.strip_prefix("refs/remotes/") {
        format!("Merge remote-tracking branch '{branch}'")
    } else {
        format!("Merge '{refname}'")
    }
}

/// Merges a branch into the current one: a fast-forward when possible,
/// otherwise a merge commit.
#[derive(Clone)]
pub struct MergeIntoCurrent {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
}

impl MergeIntoCurrent {
    pub fn new(reader: Arc<dyn RepositoryReader>, writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { reader, writer }
    }

    /// Merges the branch `refname`, e.g. `refs/remotes/origin/main`.
    pub fn merge(
        &self,
        repo: &RepositoryInfo,
        refname: &str,
    ) -> Result<IntegrationOutcome, OperationError> {
        let reader = self.reader.as_ref();
        let (name, head) = current(reader, repo)?;
        ensure_idle(reader, repo)?;
        ensure_clean(reader, repo)?;
        let target = resolve(reader, repo, refname)?
            .ok_or_else(|| OperationError::BranchNotFound(refname.to_owned()))?;
        let (ahead, behind) = reader.ahead_behind(repo, head, target)?;
        if behind == 0 {
            return Ok(IntegrationOutcome::UpToDate);
        }
        if ahead == 0 {
            self.writer.fast_forward(repo, &name, target)?;
            return Ok(IntegrationOutcome::FastForwarded { to: target });
        }
        Ok(
            match self.writer.merge(repo, target, &merge_message(refname))? {
                Integration::Completed => IntegrationOutcome::Merged,
                Integration::Conflicts(files) => IntegrationOutcome::Conflicts {
                    operation: OngoingOperation::Merge,
                    files,
                },
            },
        )
    }
}

/// Rebases the current branch onto another branch.
#[derive(Clone)]
pub struct RebaseCurrentOnto {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
}

impl RebaseCurrentOnto {
    pub fn new(reader: Arc<dyn RepositoryReader>, writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { reader, writer }
    }

    /// Replays the current branch's own commits on top of `refname`.
    pub fn rebase(
        &self,
        repo: &RepositoryInfo,
        refname: &str,
    ) -> Result<IntegrationOutcome, OperationError> {
        let reader = self.reader.as_ref();
        let (name, head) = current(reader, repo)?;
        ensure_idle(reader, repo)?;
        ensure_clean(reader, repo)?;
        let onto = resolve(reader, repo, refname)?
            .ok_or_else(|| OperationError::BranchNotFound(refname.to_owned()))?;
        let (ahead, behind) = reader.ahead_behind(repo, head, onto)?;
        if behind == 0 {
            return Ok(IntegrationOutcome::UpToDate);
        }
        if ahead == 0 {
            self.writer.fast_forward(repo, &name, onto)?;
            return Ok(IntegrationOutcome::FastForwarded { to: onto });
        }
        Ok(match self.writer.rebase(repo, onto)? {
            Integration::Completed => IntegrationOutcome::Rebased,
            Integration::Conflicts(files) => IntegrationOutcome::Conflicts {
                operation: OngoingOperation::Rebase,
                files,
            },
        })
    }
}

/// A merge or similar the repository is in the middle of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OngoingState {
    pub operation: OngoingOperation,
    /// Files with unresolved conflicts, sorted.
    pub conflicts: Vec<String>,
}

/// Reports a merge or rebase in progress, e.g. one started outside the app.
#[derive(Clone)]
pub struct LoadOngoing {
    reader: Arc<dyn RepositoryReader>,
}

impl LoadOngoing {
    pub fn new(reader: Arc<dyn RepositoryReader>) -> Self {
        Self { reader }
    }

    pub fn load(&self, repo: &RepositoryInfo) -> Result<Option<OngoingState>, ReadError> {
        let Some(operation) = self.reader.ongoing_operation(repo)? else {
            return Ok(None);
        };
        Ok(Some(OngoingState {
            operation,
            conflicts: self.reader.conflicted_files(repo)?,
        }))
    }
}

/// Continues a merge or rebase whose conflicts the user has resolved.
#[derive(Clone)]
pub struct ContinueOperation {
    reader: Arc<dyn RepositoryReader>,
    writer: Arc<dyn RepositoryWriter>,
}

impl ContinueOperation {
    pub fn new(reader: Arc<dyn RepositoryReader>, writer: Arc<dyn RepositoryWriter>) -> Self {
        Self { reader, writer }
    }

    pub fn resume(&self, repo: &RepositoryInfo) -> Result<IntegrationOutcome, OperationError> {
        let operation = self
            .reader
            .ongoing_operation(repo)?
            .ok_or(OperationError::NothingInProgress)?;
        if !matches!(
            operation,
            OngoingOperation::Merge | OngoingOperation::Rebase
        ) {
            return Err(OperationError::CannotContinue(operation));
        }
        let conflicts = self.reader.conflicted_files(repo)?;
        if !conflicts.is_empty() {
            return Err(OperationError::UnresolvedConflicts(conflicts));
        }
        Ok(match self.writer.continue_operation(repo)? {
            Integration::Completed if operation == OngoingOperation::Merge => {
                IntegrationOutcome::Merged
            }
            Integration::Completed => IntegrationOutcome::Rebased,
            Integration::Conflicts(files) => IntegrationOutcome::Conflicts { operation, files },
        })
    }
}
