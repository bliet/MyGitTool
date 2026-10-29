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

use git2::{BranchType, IndexAddOption, Oid, Repository, RepositoryInitOptions, Signature, Time};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Fixed base for commit timestamps (2023-11-14), so tests are deterministic.
const BASE_TIME: i64 = 1_700_000_000;

pub struct TestRepo {
    dir: TempDir,
    repo: Repository,
    commits: i64,
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
        }
        Self {
            dir,
            repo,
            commits: 0,
        }
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
    pub fn commit_with_message(mut self, file: &str, content: &str, message: &str) -> Self {
        self = self.write(file, content);
        let signature = self.next_signature();
        {
            let repo = &self.repo;
            let mut index = repo.index().expect("open index");
            index
                .add_all(["*"], IndexAddOption::DEFAULT, None)
                .expect("stage files");
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

    fn work_path(&self, relative: &str) -> PathBuf {
        let workdir = self
            .repo
            .workdir()
            .expect("bare repositories have no work tree");
        workdir.join(relative)
    }

    fn next_signature(&mut self) -> Signature<'static> {
        self.commits += 1;
        let time = Time::new(BASE_TIME + self.commits * 60, 0);
        Signature::new("Test User", "test@example.com", &time).expect("signature")
    }
}
