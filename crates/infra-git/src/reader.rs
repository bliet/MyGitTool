use crate::map;
use application::{HistoryCursor, HistoryScope, ReadError, RepositoryReader};
use domain::{
    Branch, BranchKind, Commit, CommitDetails, CommitId, FileChange, Head, Remote, RepositoryInfo,
    Tag, Upstream,
};
use git2::{
    BranchType, Diff, DiffFindOptions, DiffOptions, ErrorCode, Oid, Patch, ReferenceType,
    Repository, Sort,
};

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
