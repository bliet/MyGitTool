use crate::map;
use application::{Integration, RepositoryWriter, WriteError};
use domain::{CommitId, RepositoryInfo};
use git2::build::CheckoutBuilder;
use git2::{
    ApplyLocation, BranchType, Commit, Delta, Diff, DiffOptions, ErrorCode, Index, IndexEntry,
    IndexTime, MergeOptions, Rebase, RebaseOptions, Repository, RepositoryState, ResetType,
    Signature, StashApplyOptions, StashFlags,
};
use std::path::Path;

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

    fn stage(&self, repo: &RepositoryInfo, paths: &[String]) -> Result<(), WriteError> {
        let git = open(repo)?;
        let workdir = git
            .workdir()
            .ok_or_else(|| WriteError::Backend("a bare repository has no work tree".to_owned()))?
            .to_path_buf();
        let mut index = git.index().map_err(error)?;
        for path in paths {
            // Both calls also drop the conflict entries of the path.
            if workdir.join(path).symlink_metadata().is_ok() {
                index.add_path(Path::new(path)).map_err(error)?;
            } else {
                index.remove_path(Path::new(path)).map_err(error)?;
            }
        }
        index.write().map_err(error)
    }

    fn unstage(&self, repo: &RepositoryInfo, paths: &[String]) -> Result<(), WriteError> {
        // Like libgit2's `git_reset_default`, but with literal paths: that
        // one treats them as glob patterns.
        let git = open(repo)?;
        let head_tree = match git.head() {
            Ok(head) => Some(head.peel_to_tree().map_err(error)?),
            Err(err) if err.code() == ErrorCode::UnbornBranch => None,
            Err(err) => return Err(error(err)),
        };
        let mut index = git.index().map_err(error)?;
        let mut options = DiffOptions::new();
        options.disable_pathspec_match(true);
        for path in paths {
            options.pathspec(path.as_str());
        }
        let diff = git
            .diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut options))
            .map_err(error)?;
        for delta in diff.deltas() {
            let head = delta.old_file();
            let path = delta
                .new_file()
                .path_bytes()
                .or_else(|| head.path_bytes())
                .unwrap_or_default()
                .to_vec();
            match delta.status() {
                // Not staged; resolving is done by staging.
                Delta::Conflicted => {}
                Delta::Added => index
                    .remove_path(Path::new(&map::text(&path)))
                    .map_err(error)?,
                _ => index
                    .add(&IndexEntry {
                        // Zero stat data makes Git compare the file's
                        // content the next time instead of trusting it.
                        ctime: IndexTime::new(0, 0),
                        mtime: IndexTime::new(0, 0),
                        dev: 0,
                        ino: 0,
                        mode: head.mode().into(),
                        uid: 0,
                        gid: 0,
                        file_size: 0,
                        id: head.id(),
                        flags: 0,
                        flags_extended: 0,
                        path,
                    })
                    .map_err(error)?,
            }
        }
        index.write().map_err(error)
    }

    fn commit(&self, repo: &RepositoryInfo, message: &str) -> Result<CommitId, WriteError> {
        let git = open(repo)?;
        let mut index = git.index().map_err(error)?;
        let tree = git
            .find_tree(index.write_tree().map_err(error)?)
            .map_err(error)?;
        let parent = match git.head() {
            Ok(head) => Some(head.peel_to_commit().map_err(error)?),
            Err(err) if err.code() == ErrorCode::UnbornBranch => None,
            Err(err) => return Err(error(err)),
        };
        let parents: Vec<&Commit<'_>> = parent.iter().collect();
        let signature = git.signature().map_err(error)?;
        let id = git
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &parents,
            )
            .map_err(error)?;
        Ok(map::commit_id(id))
    }

    fn stash_save(
        &self,
        repo: &RepositoryInfo,
        message: Option<&str>,
        include_untracked: bool,
    ) -> Result<(), WriteError> {
        let mut git = open(repo)?;
        let stasher = git.signature().map_err(error)?;
        let flags = if include_untracked {
            StashFlags::INCLUDE_UNTRACKED
        } else {
            StashFlags::DEFAULT
        };
        git.stash_save2(&stasher, message, Some(flags))
            .map_err(error)?;
        Ok(())
    }

    fn stash_apply(&self, repo: &RepositoryInfo, index: usize) -> Result<Integration, WriteError> {
        let mut git = open(repo)?;
        // Like `git stash apply`: conflicts are written into the files
        // instead of stopping, and never overwrite other local changes.
        let mut checkout = CheckoutBuilder::new();
        checkout
            .safe()
            .allow_conflicts(true)
            .conflict_style_merge(true);
        let mut options = StashApplyOptions::new();
        options.checkout_options(checkout);
        git.stash_apply(index, Some(&mut options)).map_err(error)?;
        let index = git.index().map_err(error)?;
        if index.has_conflicts() {
            return Ok(Integration::Conflicts(conflicts(&index)?));
        }
        Ok(Integration::Completed)
    }

    fn stash_drop(&self, repo: &RepositoryInfo, index: usize) -> Result<(), WriteError> {
        let mut git = open(repo)?;
        git.stash_drop(index).map_err(error)
    }

    fn apply_patch(&self, repo: &RepositoryInfo, patch: &[u8]) -> Result<(), WriteError> {
        let git = open(repo)?;
        let diff = Diff::from_buffer(patch)
            .map_err(|err| WriteError::Backend(format!("not a valid patch: {}", err.message())))?;
        git.apply(&diff, ApplyLocation::WorkDir, None)
            .map_err(|err| {
                WriteError::Backend(format!("the patch does not apply: {}", err.message()))
            })
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
