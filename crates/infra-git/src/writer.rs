use crate::map;
use application::{Integration, RepositoryWriter, WriteError};
use domain::{CommitId, RepositoryInfo};
use git2::build::CheckoutBuilder;
use git2::{
    BranchType, ErrorCode, Index, MergeOptions, Rebase, RebaseOptions, Repository, RepositoryState,
    ResetType, Signature,
};

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
            let files = conflicts(&index)?;
            write_merge_message(&git, message, &files)?;
            return Ok(Integration::Conflicts(files));
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
        run_rebase(&git, &mut rebase, &signature)
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

    fn checkout(&self, repo: &RepositoryInfo, branch: &str) -> Result<(), WriteError> {
        let git = open(repo)?;
        let refname = format!("refs/heads/{branch}");
        let commit = git
            .find_reference(&refname)
            .and_then(|r| r.peel_to_commit())
            .map_err(error)?;
        // A safe checkout keeps local changes to files the switch does not
        // touch, and fails instead of overwriting anything else.
        git.checkout_tree(commit.as_object(), Some(CheckoutBuilder::new().safe()))
            .map_err(|err| match err.code() {
                ErrorCode::Conflict => WriteError::Backend(format!(
                    "switching to {branch} would overwrite local changes or untracked files ({})",
                    err.message()
                )),
                _ => error(err),
            })?;
        git.set_head(&refname).map_err(error)
    }

    fn create_branch(
        &self,
        repo: &RepositoryInfo,
        name: &str,
        target: CommitId,
    ) -> Result<(), WriteError> {
        let git = open(repo)?;
        let commit = git.find_commit(map::oid(target)).map_err(error)?;
        git.branch(name, &commit, false).map_err(error)?;
        Ok(())
    }

    fn delete_branch(&self, repo: &RepositoryInfo, name: &str) -> Result<(), WriteError> {
        let git = open(repo)?;
        let mut branch = git.find_branch(name, BranchType::Local).map_err(error)?;
        // Also removes the branch's config section.
        branch.delete().map_err(error)
    }

    fn create_tag(
        &self,
        repo: &RepositoryInfo,
        name: &str,
        target: CommitId,
        message: Option<&str>,
    ) -> Result<(), WriteError> {
        let git = open(repo)?;
        let commit = git.find_commit(map::oid(target)).map_err(error)?;
        match message {
            Some(message) => {
                let tagger = git.signature().map_err(error)?;
                git.tag(name, commit.as_object(), &tagger, message, false)
            }
            None => git.tag_lightweight(name, commit.as_object(), false),
        }
        .map_err(error)?;
        Ok(())
    }

    fn delete_tag(&self, repo: &RepositoryInfo, name: &str) -> Result<(), WriteError> {
        let git = open(repo)?;
        git.tag_delete(name).map_err(error)
    }

    fn continue_operation(&self, repo: &RepositoryInfo) -> Result<Integration, WriteError> {
        let mut git = open(repo)?;
        let signature = git.signature().map_err(error)?;
        let index = git.index().map_err(error)?;
        if index.has_conflicts() {
            return Ok(Integration::Conflicts(conflicts(&index)?));
        }
        match git.state() {
            RepositoryState::Merge => {
                let mut heads = Vec::new();
                git.mergehead_foreach(|oid| {
                    heads.push(*oid);
                    true
                })
                .map_err(error)?;
                let mut parents = vec![
                    git.head()
                        .and_then(|head| head.peel_to_commit())
                        .map_err(error)?,
                ];
                for oid in heads {
                    parents.push(git.find_commit(oid).map_err(error)?);
                }
                let message = git
                    .message()
                    .map(|text| strip_comments(&text))
                    .unwrap_or_else(|_| "Merge".to_owned());
                let mut index = index;
                let tree = git
                    .find_tree(index.write_tree().map_err(error)?)
                    .map_err(error)?;
                let parents: Vec<_> = parents.iter().collect();
                git.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    &message,
                    &tree,
                    &parents,
                )
                .map_err(error)?;
                git.cleanup_state().map_err(error)?;
                Ok(Integration::Completed)
            }
            RepositoryState::Rebase
            | RepositoryState::RebaseInteractive
            | RepositoryState::RebaseMerge => {
                let mut rebase = git.open_rebase(None).map_err(error)?;
                if rebase.operation_current().is_some() {
                    commit_step(&mut rebase, &signature)?;
                }
                run_rebase(&git, &mut rebase, &signature)
            }
            _ => Err(WriteError::Backend(
                "there is no merge or rebase to continue".to_owned(),
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

/// Replaces libgit2's default `MERGE_MSG` with `message`, so continuing
/// the merge (in the app or with `git commit`) uses it. Lists the conflicts
/// as comments, as Git does.
fn write_merge_message(
    git: &Repository,
    message: &str,
    conflicts: &[String],
) -> Result<(), WriteError> {
    let mut text = format!("{message}\n\n# Conflicts:\n");
    for file in conflicts {
        text.push_str(&format!("#\t{file}\n"));
    }
    std::fs::write(git.path().join("MERGE_MSG"), text)
        .map_err(|err| WriteError::Backend(format!("cannot write MERGE_MSG: {err}")))
}

/// The paths with conflicts, sorted.
fn conflicts(index: &Index) -> Result<Vec<String>, WriteError> {
    map::conflict_paths(index).map_err(error)
}

/// Applies and commits the remaining steps of `rebase`. Stops at the first
/// conflict, leaving the rebase in progress.
fn run_rebase(
    git: &Repository,
    rebase: &mut Rebase<'_>,
    signature: &Signature<'_>,
) -> Result<Integration, WriteError> {
    while let Some(operation) = rebase.next() {
        operation.map_err(error)?;
        let index = git.index().map_err(error)?;
        if index.has_conflicts() {
            return Ok(Integration::Conflicts(conflicts(&index)?));
        }
        commit_step(rebase, signature)?;
    }
    rebase.finish(Some(signature)).map_err(error)?;
    Ok(Integration::Completed)
}

/// Drops `#` comment lines and surrounding blank lines, as `git commit`
/// does with the default cleanup mode.
fn strip_comments(message: &str) -> String {
    let kept: Vec<&str> = message
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    kept.join("\n").trim().to_owned()
}

/// Commits the current step of a rebase.
fn commit_step(rebase: &mut Rebase<'_>, signature: &Signature<'_>) -> Result<(), WriteError> {
    match rebase.commit(None, signature, None) {
        Ok(_) => Ok(()),
        // The change is already upstream; there is nothing to commit.
        Err(err) if err.code() == ErrorCode::Applied => Ok(()),
        Err(err) => Err(error(err)),
    }
}
