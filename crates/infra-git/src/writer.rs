use crate::map;
use application::{Integration, RepositoryWriter, WriteError};
use domain::{CommitId, RepositoryInfo};
use git2::build::CheckoutBuilder;
use git2::{ErrorCode, Index, MergeOptions, RebaseOptions, Repository, RepositoryState, ResetType};

/// Changes repositories with git2. Like the reader, it opens the repository
/// on every call.
#[derive(Debug, Default, Clone, Copy)]
pub struct GitRepositoryWriter;

fn open(repo: &RepositoryInfo) -> Result<Repository, WriteError> {
    Repository::open(repo.root()).map_err(error)
}

fn error(err: git2::Error) -> WriteError {
    WriteError::Backend(err.message().to_owned())
}

impl RepositoryWriter for GitRepositoryWriter {
    fn fast_forward(
        &self,
        repo: &RepositoryInfo,
        branch: &str,
        target: CommitId,
    ) -> Result<(), WriteError> {
        let git = open(repo)?;
        let commit = git.find_commit(map::oid(target)).map_err(error)?;
        // A safe checkout refuses to overwrite local changes instead of
        // discarding them.
        git.checkout_tree(commit.as_object(), Some(CheckoutBuilder::new().safe()))
            .map_err(error)?;
        let mut reference = git
            .find_reference(&format!("refs/heads/{branch}"))
            .map_err(error)?;
        reference
            .set_target(commit.id(), "pull: fast-forward")
            .map_err(error)?;
        Ok(())
    }

    fn merge(
        &self,
        repo: &RepositoryInfo,
        commit: CommitId,
        message: &str,
    ) -> Result<Integration, WriteError> {
        let git = open(repo)?;
        let theirs = git.find_commit(map::oid(commit)).map_err(error)?;
        let annotated = git.find_annotated_commit(theirs.id()).map_err(error)?;
        let mut checkout = CheckoutBuilder::new();
        checkout
            .safe()
            .allow_conflicts(true)
            .conflict_style_merge(true);
        git.merge(
            &[&annotated],
            Some(&mut MergeOptions::new()),
            Some(&mut checkout),
        )
        .map_err(error)?;

        let mut index = git.index().map_err(error)?;
        if index.has_conflicts() {
            return Ok(Integration::Conflicts(conflicts(&index)?));
        }
        let tree = git
            .find_tree(index.write_tree().map_err(error)?)
            .map_err(error)?;
        let ours = git
            .head()
            .and_then(|head| head.peel_to_commit())
            .map_err(error)?;
        let signature = git.signature().map_err(error)?;
        git.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &[&ours, &theirs],
        )
        .map_err(error)?;
        git.cleanup_state().map_err(error)?;
        Ok(Integration::Completed)
    }

    fn rebase(&self, repo: &RepositoryInfo, upstream: CommitId) -> Result<Integration, WriteError> {
        let git = open(repo)?;
        let upstream = git
            .find_annotated_commit(map::oid(upstream))
            .map_err(error)?;
        let signature = git.signature().map_err(error)?;
        let mut rebase = git
            .rebase(None, Some(&upstream), None, Some(&mut RebaseOptions::new()))
            .map_err(error)?;
        while let Some(operation) = rebase.next() {
            operation.map_err(error)?;
            let index = git.index().map_err(error)?;
            if index.has_conflicts() {
                return Ok(Integration::Conflicts(conflicts(&index)?));
            }
            match rebase.commit(None, &signature, None) {
                Ok(_) => {}
                // The change is already upstream; there is nothing to commit.
                Err(err) if err.code() == ErrorCode::Applied => {}
                Err(err) => return Err(error(err)),
            }
        }
        rebase.finish(Some(&signature)).map_err(error)?;
        Ok(Integration::Completed)
    }

    fn abort(&self, repo: &RepositoryInfo) -> Result<(), WriteError> {
        let git = open(repo)?;
        match git.state() {
            RepositoryState::Rebase
            | RepositoryState::RebaseInteractive
            | RepositoryState::RebaseMerge => {
                let mut rebase = git.open_rebase(None).map_err(error)?;
                rebase.abort().map_err(error)
            }
            RepositoryState::Merge => {
                // Like `git merge --abort`. A merge starts only on a clean
                // work tree, so the reset drops nothing but the merge.
                let head = git
                    .head()
                    .and_then(|head| head.peel_to_commit())
                    .map_err(error)?;
                git.reset(head.as_object(), ResetType::Hard, None)
                    .map_err(error)?;
                git.cleanup_state().map_err(error)
            }
            _ => Err(WriteError::Backend(
                "there is no merge or rebase to abort".to_owned(),
            )),
        }
    }

    fn set_tracking(
        &self,
        repo: &RepositoryInfo,
        branch: &str,
        remote: &str,
        merge_ref: &str,
    ) -> Result<(), WriteError> {
        let git = open(repo)?;
        let mut config = git.config().map_err(error)?;
        config
            .set_str(&format!("branch.{branch}.remote"), remote)
            .map_err(error)?;
        config
            .set_str(&format!("branch.{branch}.merge"), merge_ref)
            .map_err(error)
    }
}

/// The paths with conflicts, sorted.
fn conflicts(index: &Index) -> Result<Vec<String>, WriteError> {
    let mut paths = Vec::new();
    for conflict in index.conflicts().map_err(error)? {
        let conflict = conflict.map_err(error)?;
        let entry = conflict.our.or(conflict.their).or(conflict.ancestor);
        if let Some(entry) = entry {
            paths.push(map::text(&entry.path));
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}
