use crate::map;
use application::{HistoryCursor, HistoryScope, ReadError, RepositoryReader};
use domain::{
    Branch, BranchKind, Commit, CommitDetails, CommitId, FastForward, FileChange, FileContent,
    Head, OngoingOperation, PullConfig, Remote, RepositoryInfo, Revision, Tag, Tracking, Upstream,
};
use git2::{
    BranchType, Diff, DiffFindOptions, DiffOptions, ErrorCode, ObjectType, Oid, Patch,
    ReferenceType, Repository, RepositoryState, Sort, Status, StatusOptions, Tree,
};
use std::fs;
use std::io;
use std::path::Path;

/// Reads repositories with git2. It is stateless: each call opens the
/// repository again, which keeps it `Sync` and always up to date.
#[derive(Debug, Default, Clone, Copy)]
pub struct GitRepositoryReader;

fn open(repo: &RepositoryInfo) -> Result<Repository, ReadError> {
    Repository::open(repo.root()).map_err(map::error)
}

impl RepositoryReader for GitRepositoryReader {
    fn head(&self, repo: &RepositoryInfo) -> Result<Head, ReadError> {
        let git = open(repo)?;
        match git.head() {
            Ok(head) => {
                let target = head.peel_to_commit().map_err(map::error)?.id();
                Ok(if head.is_branch() {
                    Head::Branch {
                        name: map::text(head.shorthand_bytes()),
                        target: map::commit_id(target),
                    }
                } else {
                    Head::Detached(map::commit_id(target))
                })
            }
            Err(err) if err.code() == ErrorCode::UnbornBranch => {
                let head = git.find_reference("HEAD").map_err(map::error)?;
                let target = head
                    .symbolic_target_bytes()
                    .map(map::text)
                    .unwrap_or_default();
                let name = target
                    .strip_prefix("refs/heads/")
                    .unwrap_or(&target)
                    .to_owned();
                Ok(Head::Unborn { name })
            }
            Err(err) => Err(map::error(err)),
        }
    }

    fn branches(&self, repo: &RepositoryInfo) -> Result<Vec<Branch>, ReadError> {
        let git = open(repo)?;
        let mut branches = Vec::new();
        for entry in git.branches(None).map_err(map::error)? {
            let (branch, branch_type) = entry.map_err(map::error)?;
            let reference = branch.get();
            // Skips symbolic refs such as `origin/HEAD`.
            if reference.kind() != Some(ReferenceType::Direct) {
                continue;
            }
            let Some(target) = reference.target() else {
                continue;
            };
            let refname = map::text(reference.name_bytes());
            let name = map::text(branch.name_bytes().map_err(map::error)?);
            let kind = match branch_type {
                BranchType::Local => BranchKind::Local {
                    is_head: branch.is_head(),
                    upstream: upstream(&git, &branch, &refname, target),
                },
                BranchType::Remote => BranchKind::Remote {
                    remote: remote_of(&git, &refname, &name),
                },
            };
            branches.push(Branch {
                name,
                refname,
                target: map::commit_id(target),
                kind,
            });
        }
        Ok(branches)
    }

    fn remotes(&self, repo: &RepositoryInfo) -> Result<Vec<Remote>, ReadError> {
        let git = open(repo)?;
        let names = git.remotes().map_err(map::error)?;
        let remotes = names
            .iter_bytes()
            .map(map::text)
            .map(|name| {
                let url = git
                    .find_remote(&name)
                    .ok()
                    .map(|remote| map::text(remote.url_bytes()))
                    .filter(|url| !url.is_empty());
                Remote { name, url }
            })
            .collect();
        Ok(remotes)
    }

    fn tags(&self, repo: &RepositoryInfo) -> Result<Vec<Tag>, ReadError> {
        let git = open(repo)?;
        let mut tags = Vec::new();
        for reference in git.references_glob("refs/tags/*").map_err(map::error)? {
            let reference = reference.map_err(map::error)?;
            // Tags of trees or blobs have no place in the history.
            let Ok(commit) = reference.peel_to_commit() else {
                continue;
            };
            tags.push(Tag {
                name: map::text(reference.shorthand_bytes()),
                target: map::commit_id(commit.id()),
            });
        }
        Ok(tags)
    }

    fn history(
        &self,
        repo: &RepositoryInfo,
        scope: &HistoryScope,
    ) -> Result<Box<dyn HistoryCursor>, ReadError> {
        let git = open(repo)?;
        let ids = walk(&git, scope)?;
        Ok(Box::new(GitHistoryCursor { git, ids, next: 0 }))
    }

    fn commit_details(
        &self,
        repo: &RepositoryInfo,
        id: CommitId,
    ) -> Result<CommitDetails, ReadError> {
        let git = open(repo)?;
        let commit = git
            .find_commit(map::oid(id))
            .map_err(|err| match err.code() {
                ErrorCode::NotFound => ReadError::CommitNotFound(id),
                _ => map::error(err),
            })?;
        let tree = commit.tree().map_err(map::error)?;
        let parent_tree = match commit.parent(0) {
            Ok(parent) => Some(parent.tree().map_err(map::error)?),
            Err(_) => None,
        };
        let mut diff = git
            .diff_tree_to_tree(
                parent_tree.as_ref(),
                Some(&tree),
                Some(&mut DiffOptions::new()),
            )
            .map_err(map::error)?;
        Ok(CommitDetails {
            commit: map::commit(&commit),
            committer: map::signature(&commit.committer()),
            message: map::text(commit.message_bytes()),
            changes: changes(&mut diff)?,
        })
    }

    fn work_tree_changes(&self, repo: &RepositoryInfo) -> Result<Vec<FileChange>, ReadError> {
        let git = open(repo)?;
        if git.is_bare() {
            return Ok(Vec::new());
        }
        let head_tree = match git.head() {
            Ok(head) => Some(head.peel_to_tree().map_err(map::error)?),
            Err(err) if err.code() == ErrorCode::UnbornBranch => None,
            Err(err) => return Err(map::error(err)),
        };
        let mut options = DiffOptions::new();
        options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .show_untracked_content(true);
        let mut diff = git
            .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut options))
            .map_err(map::error)?;
        changes(&mut diff)
    }

    fn file_content(
        &self,
        repo: &RepositoryInfo,
        revision: Revision,
        path: &str,
        limit: u64,
    ) -> Result<Option<FileContent>, ReadError> {
        let git = open(repo)?;
        let tree = match revision {
            Revision::WorkTree => return work_tree_content(&git, path, limit),
            Revision::Head => match git.head() {
                Ok(head) => head.peel_to_tree().map_err(map::error)?,
                Err(err) if err.code() == ErrorCode::UnbornBranch => return Ok(None),
                Err(err) => return Err(map::error(err)),
            },
            Revision::Commit(id) => git
                .find_commit(map::oid(id))
                .map_err(|err| match err.code() {
                    ErrorCode::NotFound => ReadError::CommitNotFound(id),
                    _ => map::error(err),
                })?
                .tree()
                .map_err(map::error)?,
        };
        tree_content(&git, &tree, path, limit)
    }

    fn tracking(&self, repo: &RepositoryInfo, branch: &str) -> Result<Option<Tracking>, ReadError> {
        let git = open(repo)?;
        let refname = format!("refs/heads/{branch}");
        let not_set = |err: &git2::Error| err.code() == ErrorCode::NotFound;
        let remote = match git.branch_upstream_remote(&refname) {
            Ok(remote) => map::text(&remote),
            Err(err) if not_set(&err) => return Ok(None),
            Err(err) => return Err(map::error(err)),
        };
        let merge_ref = match git.branch_upstream_merge(&refname) {
            Ok(merge) => map::text(&merge),
            Err(err) if not_set(&err) => return Ok(None),
            Err(err) => return Err(map::error(err)),
        };
        let tracking_ref = match git.branch_upstream_name(&refname) {
            Ok(name) => map::text(&name),
            // No fetch refspec maps the branch, e.g. an unusual remote config.
            Err(_) => format!(
                "refs/remotes/{remote}/{}",
                merge_ref.strip_prefix("refs/heads/").unwrap_or(&merge_ref)
            ),
        };
        Ok(Some(Tracking {
            remote,
            merge_ref,
            tracking_ref,
        }))
    }

    fn resolve(&self, repo: &RepositoryInfo, refname: &str) -> Result<Option<CommitId>, ReadError> {
        let git = open(repo)?;
        match git.find_reference(refname) {
            Ok(reference) => {
                let commit = reference.peel_to_commit().map_err(map::error)?;
                Ok(Some(map::commit_id(commit.id())))
            }
            Err(err) if err.code() == ErrorCode::NotFound => Ok(None),
            Err(err) => Err(map::error(err)),
        }
    }

    fn ahead_behind(
        &self,
        repo: &RepositoryInfo,
        local: CommitId,
        upstream: CommitId,
    ) -> Result<(usize, usize), ReadError> {
        let git = open(repo)?;
        git.graph_ahead_behind(map::oid(local), map::oid(upstream))
            .map_err(map::error)
    }

    fn dirty_files(&self, repo: &RepositoryInfo) -> Result<Vec<String>, ReadError> {
        let git = open(repo)?;
        if git.is_bare() {
            return Ok(Vec::new());
        }
        let mut options = StatusOptions::new();
        options
            .include_untracked(false)
            .include_ignored(false)
            .exclude_submodules(true);
        let statuses = git.statuses(Some(&mut options)).map_err(map::error)?;
        let changed = Status::all() - Status::WT_NEW - Status::IGNORED;
        let mut files: Vec<String> = statuses
            .iter()
            .filter(|entry| entry.status().intersects(changed))
            .map(|entry| map::text(entry.path_bytes()))
            .collect();
        files.sort();
        files.dedup();
        Ok(files)
    }

    fn ongoing_operation(
        &self,
        repo: &RepositoryInfo,
    ) -> Result<Option<OngoingOperation>, ReadError> {
        let git = open(repo)?;
        Ok(match git.state() {
            RepositoryState::Clean => None,
            RepositoryState::Merge => Some(OngoingOperation::Merge),
            RepositoryState::Rebase
            | RepositoryState::RebaseInteractive
            | RepositoryState::RebaseMerge => Some(OngoingOperation::Rebase),
            RepositoryState::CherryPick | RepositoryState::CherryPickSequence => {
                Some(OngoingOperation::CherryPick)
            }
            RepositoryState::Revert | RepositoryState::RevertSequence => {
                Some(OngoingOperation::Revert)
            }
            RepositoryState::Bisect => Some(OngoingOperation::Bisect),
            RepositoryState::ApplyMailbox | RepositoryState::ApplyMailboxOrRebase => {
                Some(OngoingOperation::Other)
            }
        })
    }

    fn pull_config(&self, repo: &RepositoryInfo, branch: &str) -> Result<PullConfig, ReadError> {
        let git = open(repo)?;
        let config = git.config().map_err(map::error)?;
        let value = |key: &str| config.get_string(key).ok().map(|v| v.to_ascii_lowercase());
        // `merges` and `interactive` also rebase; the app rebases plainly.
        let rebase = value(&format!("branch.{branch}.rebase"))
            .or_else(|| value("pull.rebase"))
            .is_some_and(|v| !is_false(&v));
        let fast_forward = match value("pull.ff").as_deref() {
            Some("only") => FastForward::Only,
            Some(v) if is_false(v) => FastForward::Never,
            _ => FastForward::Allow,
        };
        Ok(PullConfig {
            rebase,
            fast_forward,
        })
    }
}

/// Git's spellings of a false boolean.
fn is_false(value: &str) -> bool {
    matches!(value, "false" | "no" | "off" | "0" | "")
}

/// The content of a file in a tree, as Git stores it: a symlink is its
/// target path, a submodule the commit it points to.
fn tree_content(
    git: &Repository,
    tree: &Tree<'_>,
    path: &str,
    limit: u64,
) -> Result<Option<FileContent>, ReadError> {
    let entry = match tree.get_path(Path::new(path)) {
        Ok(entry) => entry,
        Err(err) if err.code() == ErrorCode::NotFound => return Ok(None),
        Err(err) => return Err(map::error(err)),
    };
    match entry.kind() {
        Some(ObjectType::Blob) => {
            let odb = git.odb().map_err(map::error)?;
            let (size, _) = odb.read_header(entry.id()).map_err(map::error)?;
            let size = size as u64;
            if size > limit {
                return Ok(Some(FileContent::TooLarge { size }));
            }
            let blob = git.find_blob(entry.id()).map_err(map::error)?;
            Ok(Some(FileContent::Bytes(blob.content().to_vec())))
        }
        Some(ObjectType::Commit) => Ok(Some(submodule(entry.id()))),
        // A directory is no file, e.g. after a file was replaced by one.
        _ => Ok(None),
    }
}

/// The content of a file in the work tree, read the way Git would store it.
fn work_tree_content(
    git: &Repository,
    path: &str,
    limit: u64,
) -> Result<Option<FileContent>, ReadError> {
    let Some(workdir) = git.workdir() else {
        return Ok(None);
    };
    let full = workdir.join(path);
    let io_error = |err: io::Error| ReadError::Backend(format!("{path}: {err}"));
    let metadata = match fs::symlink_metadata(&full) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(io_error(err)),
    };
    if metadata.is_symlink() {
        let target = fs::read_link(&full).map_err(io_error)?;
        let target = target.to_string_lossy().replace('\\', "/");
        return Ok(Some(FileContent::Bytes(target.into_bytes())));
    }
    if metadata.is_dir() {
        // A checked-out submodule; anything else is not a file.
        return Ok(Repository::open(&full)
            .ok()
            .and_then(|sub| sub.head().ok()?.target())
            .map(submodule));
    }
    if metadata.len() > limit {
        return Ok(Some(FileContent::TooLarge {
            size: metadata.len(),
        }));
    }
    fs::read(&full)
        .map(|bytes| Some(FileContent::Bytes(bytes)))
        .map_err(io_error)
}

/// A submodule as `git diff` shows it.
fn submodule(commit: Oid) -> FileContent {
    FileContent::Bytes(format!("Subproject commit {commit}\n").into_bytes())
}

/// The upstream of a local branch, including one whose ref is gone.
fn upstream(
    git: &Repository,
    branch: &git2::Branch<'_>,
    refname: &str,
    target: Oid,
) -> Option<Upstream> {
    match branch.upstream() {
        Ok(upstream) => {
            let name = map::text(upstream.name_bytes().ok()?);
            let counts = upstream
                .get()
                .target()
                .and_then(|up| git.graph_ahead_behind(target, up).ok());
            Some(Upstream {
                name,
                ahead: counts.map(|(ahead, _)| ahead),
                behind: counts.map(|(_, behind)| behind),
            })
        }
        Err(_) => {
            let configured = git.branch_upstream_name(refname).ok()?;
            let configured = map::text(&configured);
            let name = configured
                .strip_prefix("refs/remotes/")
                .unwrap_or(&configured)
                .to_owned();
            Some(Upstream {
                name,
                ahead: None,
                behind: None,
            })
        }
    }
}

/// The remote of a remote-tracking branch. Falls back to the first name
/// segment when git cannot tell (e.g. the remote was removed).
fn remote_of(git: &Repository, refname: &str, name: &str) -> String {
    git.branch_remote_name(refname)
        .map(|buf| map::text(&buf))
        .unwrap_or_else(|_| name.split('/').next().unwrap_or(name).to_owned())
}

/// All commit ids of the scope in display order.
///
/// The walk runs to the end up front, because a topological sort has to see
/// every commit anyway, and a `Revwalk` cannot outlive this call.
fn walk(git: &Repository, scope: &HistoryScope) -> Result<Vec<Oid>, ReadError> {
    let mut walk = git.revwalk().map_err(map::error)?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)
        .map_err(map::error)?;
    match scope {
        HistoryScope::All => {
            if git.head().is_ok() {
                walk.push_head().map_err(map::error)?;
            }
            for glob in ["refs/heads", "refs/remotes", "refs/tags"] {
                walk.push_glob(glob).map_err(map::error)?;
            }
        }
        HistoryScope::Ref(refname) => {
            let reference = git
                .find_reference(refname)
                .map_err(|err| match err.code() {
                    ErrorCode::NotFound => ReadError::RefNotFound(refname.clone()),
                    _ => map::error(err),
                })?;
            let commit = reference.peel_to_commit().map_err(map::error)?;
            walk.push(commit.id()).map_err(map::error)?;
        }
    }
    walk.map(|oid| oid.map_err(map::error)).collect()
}

struct GitHistoryCursor {
    git: Repository,
    ids: Vec<Oid>,
    next: usize,
}

impl HistoryCursor for GitHistoryCursor {
    fn next_page(&mut self, max: usize) -> Result<Vec<Commit>, ReadError> {
        let end = self.ids.len().min(self.next.saturating_add(max));
        let page = self.ids[self.next..end]
            .iter()
            .map(|&oid| {
                let commit = self.git.find_commit(oid).map_err(map::error)?;
                Ok(map::commit(&commit))
            })
            .collect::<Result<Vec<_>, ReadError>>()?;
        self.next = end;
        Ok(page)
    }
}

/// Converts a diff into file changes with line counts. Renames are detected.
fn changes(diff: &mut Diff<'_>) -> Result<Vec<FileChange>, ReadError> {
    diff.find_similar(Some(DiffFindOptions::new().renames(true)))
        .map_err(map::error)?;
    let mut changes = Vec::new();
    for index in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(index) else {
            continue;
        };
        let Some(kind) = map::change_kind(delta.status()) else {
            continue;
        };
        let path = delta
            .new_file()
            .path_bytes()
            .or_else(|| delta.old_file().path_bytes())
            .map(map::text)
            .unwrap_or_default();
        let old_path = (kind == domain::ChangeKind::Renamed)
            .then(|| delta.old_file().path_bytes().map(map::text))
            .flatten();
        let patch = Patch::from_diff(diff, index).map_err(map::error)?;
        let binary = delta.flags().is_binary()
            || patch
                .as_ref()
                .is_some_and(|p| p.delta().flags().is_binary());
        let (additions, deletions) = match &patch {
            Some(patch) if !binary => {
                let (_, additions, deletions) = patch.line_stats().map_err(map::error)?;
                (additions, deletions)
            }
            _ => (0, 0),
        };
        changes.push(FileChange {
            path,
            old_path,
            kind,
            additions,
            deletions,
            binary,
        });
    }
    Ok(changes)
}
