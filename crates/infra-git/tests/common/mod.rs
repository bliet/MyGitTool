//! `TestRepo`: a builder for temporary Git repositories used by the
//! integration tests.
//!
//! ```ignore
//! let repo = TestRepo::new()
//!     .commit("a.txt", "1")
//!     .branch("feature")
//!     .checkout("feature")
//!     .commit("b.txt", "2");
//! ```
#![allow(dead_code)]

use git2::{
    BranchType, IndexAddOption, ObjectType, Oid, Repository, RepositoryInitOptions, Signature, Time,
};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Fixed base for commit timestamps (2023-11-14), so tests are deterministic.
/// Likewise, every repository sets `core.autocrlf=false`, so a machine that
/// converts line endings (e.g. Git for Windows) cannot change file contents.
const BASE_TIME: i64 = 1_700_000_000;

pub struct TestRepo {
    dir: TempDir,
    repo: Repository,
    commits: i64,
    /// Added to every commit time, see [`TestRepo::shift_clock`].
    clock: i64,
}

impl TestRepo {
    /// A new non-bare repository whose initial branch is `main`, without commits.
    pub fn new() -> Self {
        Self::init(false)
    }

    /// A new bare repository, e.g. to act as a remote.
    pub fn bare() -> Self {
        Self::init(true)
    }

    fn init(bare: bool) -> Self {
        let dir = TempDir::new().expect("create temp dir");
        let mut opts = RepositoryInitOptions::new();
        opts.bare(bare).initial_head("main");
        let repo = Repository::init_opts(dir.path(), &opts).expect("init repository");
        {
            let mut config = repo.config().expect("open repo config");
            config
                .set_str("user.name", "Test User")
                .expect("set user.name");
            config
                .set_str("user.email", "test@example.com")
                .expect("set user.email");
            config
                .set_bool("core.autocrlf", false)
                .expect("set core.autocrlf");
        }
        Self {
            dir,
            repo,
            commits: 0,
            clock: 0,
        }
    }

    /// A clone of the repository at `url`, e.g. a bare `TestRepo` acting as
    /// a remote, with the same user config.
    pub fn clone_of(url: &Path) -> Self {
        let dir = TempDir::new().expect("create temp dir");
        let repo = Repository::clone(url.to_str().expect("UTF-8 path"), dir.path())
            .expect("clone repository");
        {
            let mut config = repo.config().expect("open repo config");
            config
                .set_str("user.name", "Other User")
                .expect("set user.name");
            config
                .set_str("user.email", "other@example.com")
                .expect("set user.email");
            config
                .set_bool("core.autocrlf", false)
                .expect("set core.autocrlf");
        }
        // The clone checked its files out with the machine's line endings.
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .expect("check out again");
        Self {
            dir,
            repo,
            commits: 100,
            clock: 0,
        }
    }

    /// Sets a value in the repository's config.
    pub fn config(self, key: &str, value: &str) -> Self {
        self.repo
            .config()
            .expect("open repo config")
            .set_str(key, value)
            .expect("set config value");
        self
    }

    /// The repository's root: the work tree, or the Git directory if bare.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn git(&self) -> &Repository {
        &self.repo
    }

    /// Writes `content` to `file` in the work tree without staging it.
    pub fn write(self, file: &str, content: &str) -> Self {
        let path = self.work_path(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(&path, content).expect("write file");
        self
    }

    /// Creates a directory (and its parents) in the work tree.
    pub fn mkdir(self, dir: &str) -> Self {
        fs::create_dir_all(self.work_path(dir)).expect("create dir");
        self
    }

    /// Writes `file`, stages everything and commits with the message `"Update <file>"`.
    pub fn commit(self, file: &str, content: &str) -> Self {
        let message = format!("Update {file}");
        self.commit_with_message(file, content, &message)
    }

    /// Writes `file`, stages everything and commits with `message`.
    pub fn commit_with_message(self, file: &str, content: &str, message: &str) -> Self {
        self.write(file, content).commit_all(message)
    }

    /// Stages the work tree state of `file`, including its deletion.
    pub fn stage(self, file: &str) -> Self {
        {
            let mut index = self.repo.index().expect("open index");
            if self.work_path(file).symlink_metadata().is_ok() {
                index.add_path(Path::new(file)).expect("stage file");
            } else {
                index.remove_path(Path::new(file)).expect("stage deletion");
            }
            index.write().expect("write index");
        }
        self
    }

    /// Deletes `file` from the work tree without staging it.
    pub fn delete(self, file: &str) -> Self {
        fs::remove_file(self.work_path(file)).expect("delete file");
        self
    }

    /// Renames `from` to `to` in the work tree without staging it.
    pub fn rename(self, from: &str, to: &str) -> Self {
        fs::rename(self.work_path(from), self.work_path(to)).expect("rename file");
        self
    }

    /// Stages all work tree changes (including deletions) and commits them.
    pub fn commit_all(mut self, message: &str) -> Self {
        let signature = self.next_signature();
        {
            let repo = &self.repo;
            let mut index = repo.index().expect("open index");
            index
                .add_all(["*"], IndexAddOption::DEFAULT, None)
                .expect("stage files");
            index.update_all(["*"], None).expect("stage deletions");
            index.write().expect("write index");
            let tree = repo
                .find_tree(index.write_tree().expect("write tree"))
                .expect("find tree");
            let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
            let parents: Vec<_> = parent.iter().collect();
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &parents,
            )
            .expect("commit");
        }
        self
    }

    /// Creates a merge commit of HEAD and the local branch `other`. The tree
    /// is HEAD's, which is all the history tests need.
    pub fn merge(self, other: &str, message: &str) -> Self {
        self.merge_all(&[other], message)
    }

    /// Like [`merge`](Self::merge) with several branches, e.g. an octopus merge.
    pub fn merge_all(mut self, others: &[&str], message: &str) -> Self {
        let signature = self.next_signature();
        {
            let repo = &self.repo;
            let head = repo
                .head()
                .expect("HEAD")
                .peel_to_commit()
                .expect("HEAD commit");
            let mut parents = vec![head.clone()];
            for other in others {
                let commit = repo
                    .find_branch(other, BranchType::Local)
                    .expect("find branch")
                    .get()
                    .peel_to_commit()
                    .expect("branch commit");
                parents.push(commit);
            }
            let parents: Vec<_> = parents.iter().collect();
            let tree = head.tree().expect("HEAD tree");
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &parents,
            )
            .expect("merge commit");
        }
        self
    }

    /// Moves the clock of the following commits by `seconds`, e.g. back in
    /// time to make a commit older than its parent.
    pub fn shift_clock(mut self, seconds: i64) -> Self {
        self.clock += seconds;
        self
    }

    /// Writes a commit-graph with the git command line, split into layers
    /// if `split`. Layers are only added for commits not covered yet.
    pub fn write_commit_graph(self, split: bool) -> Self {
        let mut args = vec!["commit-graph", "write", "--reachable"];
        if split {
            args.push("--split=no-merge");
        }
        let status = std::process::Command::new("git")
            .args(&args)
            .current_dir(self.path())
            .status()
            .expect("run git");
        assert!(status.success(), "git commit-graph write failed");
        self
    }

    /// A lightweight tag at HEAD.
    pub fn tag(self, name: &str) -> Self {
        {
            let head = self
                .repo
                .head()
                .expect("HEAD")
                .peel(ObjectType::Commit)
                .expect("HEAD commit");
            self.repo
                .tag_lightweight(name, &head, false)
                .expect("create tag");
        }
        self
    }

    /// An annotated tag at HEAD.
    pub fn annotated_tag(mut self, name: &str) -> Self {
        let signature = self.next_signature();
        {
            let head = self
                .repo
                .head()
                .expect("HEAD")
                .peel(ObjectType::Commit)
                .expect("HEAD commit");
            self.repo
                .tag(name, &head, &signature, "annotated", false)
                .expect("create tag");
        }
        self
    }

    /// Configures a remote without fetching from it.
    pub fn remote(self, name: &str, url: &str) -> Self {
        self.repo.remote(name, url).expect("add remote");
        self
    }

    /// Creates the remote-tracking ref `refs/remotes/<remote>/<branch>` at
    /// HEAD, as if it had been fetched.
    pub fn remote_branch(self, remote: &str, branch: &str) -> Self {
        let head = self.head();
        self.repo
            .reference(
                &format!("refs/remotes/{remote}/{branch}"),
                head,
                true,
                "test",
            )
            .expect("create remote ref");
        self
    }

    /// Sets the upstream of `local` to `upstream`, e.g. `origin/main`.
    pub fn set_upstream(self, local: &str, upstream: &str) -> Self {
        {
            let mut branch = self
                .repo
                .find_branch(local, BranchType::Local)
                .expect("find branch");
            branch.set_upstream(Some(upstream)).expect("set upstream");
        }
        self
    }

    /// Detaches HEAD at its current commit.
    pub fn detach(self) -> Self {
        let head = self.head();
        self.repo.set_head_detached(head).expect("detach HEAD");
        self
    }

    /// Creates a branch at HEAD without checking it out.
    pub fn branch(self, name: &str) -> Self {
        {
            let head = self
                .repo
                .head()
                .expect("HEAD")
                .peel_to_commit()
                .expect("HEAD commit");
            self.repo.branch(name, &head, false).expect("create branch");
        }
        self
    }

    /// Checks out the local branch `name` and updates the work tree.
    pub fn checkout(self, name: &str) -> Self {
        let refname = format!("refs/heads/{name}");
        self.repo
            .find_branch(name, BranchType::Local)
            .expect("find branch");
        self.repo.set_head(&refname).expect("set HEAD");
        self.repo
            .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .expect("checkout HEAD");
        self
    }

    /// The commit HEAD points to.
    pub fn head(&self) -> Oid {
        self.repo
            .head()
            .expect("HEAD")
            .target()
            .expect("HEAD target")
    }

    /// The commit the local branch `name` points to.
    pub fn head_of(&self, name: &str) -> Oid {
        self.repo
            .find_branch(name, BranchType::Local)
            .expect("find branch")
            .get()
            .target()
            .expect("branch target")
    }

    fn work_path(&self, relative: &str) -> PathBuf {
        let workdir = self
            .repo
            .workdir()
            .expect("bare repositories have no work tree");
        workdir.join(relative)
    }

    fn next_signature(&mut self) -> Signature<'static> {
        self.commits += 1;
        let time = Time::new(BASE_TIME + self.commits * 60 + self.clock, 0);
        Signature::new("Test User", "test@example.com", &time).expect("signature")
    }
}
