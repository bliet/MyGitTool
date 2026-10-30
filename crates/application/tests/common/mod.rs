//! Hand-written fakes of the port traits, shared by the use case tests.
#![allow(dead_code)]

use application::{LocateError, RecentRepositoriesStore, RepositoryLocator, StorageError};
use domain::RepositoryInfo;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A locator that knows a fixed set of repositories. Any path inside a known
/// root resolves to that root. `unreadable` paths fail with `Unreadable`.
#[derive(Default)]
pub struct FakeLocator {
    roots: Vec<PathBuf>,
    unreadable: Vec<PathBuf>,
}

impl FakeLocator {
    pub fn with_repo(mut self, root: impl Into<PathBuf>) -> Self {
        self.roots.push(root.into());
        self
    }

    pub fn with_unreadable(mut self, path: impl Into<PathBuf>) -> Self {
        self.unreadable.push(path.into());
        self
    }
}

impl RepositoryLocator for FakeLocator {
    fn locate(&self, path: &Path) -> Result<RepositoryInfo, LocateError> {
        if self.unreadable.iter().any(|p| p == path) {
            return Err(LocateError::Unreadable {
                path: path.to_path_buf(),
                message: "permission denied".into(),
            });
        }
        self.roots
            .iter()
            .find(|root| path.starts_with(root))
            .map(|root| RepositoryInfo::new(root.clone()))
            .ok_or_else(|| LocateError::NotARepository {
                path: path.to_path_buf(),
            })
    }
}

/// An in-memory store. Set `fail_saves` to make every save fail.
#[derive(Default)]
pub struct FakeStore {
    paths: Mutex<Vec<PathBuf>>,
    fail_saves: bool,
}

impl FakeStore {
    pub fn with_paths<I, P>(paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        Self {
            paths: Mutex::new(paths.into_iter().map(Into::into).collect()),
            ..Self::default()
        }
    }

    pub fn failing_saves(mut self) -> Self {
        self.fail_saves = true;
        self
    }

    pub fn stored(&self) -> Vec<PathBuf> {
        self.paths.lock().unwrap().clone()
    }
}

impl RecentRepositoriesStore for FakeStore {
    fn load(&self) -> Result<Vec<PathBuf>, StorageError> {
        Ok(self.stored())
    }

    fn save(&self, paths: &[PathBuf]) -> Result<(), StorageError> {
        if self.fail_saves {
            return Err(StorageError::new("cannot write", "disk full"));
        }
        *self.paths.lock().unwrap() = paths.to_vec();
        Ok(())
    }
}

/// Builds a `CommitId` from a small number, for readable tests.
pub fn id(n: u8) -> domain::CommitId {
    domain::CommitId::from_bytes([n; 20])
}

pub fn commit(n: u8, parents: &[u8]) -> domain::Commit {
    domain::Commit {
        id: id(n),
        parents: parents.iter().map(|&p| id(p)).collect(),
        author: domain::Signature {
            name: "Ann".into(),
            email: "ann@example.com".into(),
            time: domain::Timestamp {
                seconds: 1_700_000_000 + i64::from(n),
                offset_minutes: 0,
            },
        },
        summary: format!("commit {n}"),
    }
}

pub fn local(name: &str, target: u8, is_head: bool) -> domain::Branch {
    domain::Branch {
        name: name.into(),
        refname: format!("refs/heads/{name}"),
        target: id(target),
        kind: domain::BranchKind::Local {
            is_head,
            upstream: None,
        },
    }
}

pub fn remote_branch(remote: &str, name: &str, target: u8) -> domain::Branch {
    domain::Branch {
        name: format!("{remote}/{name}"),
        refname: format!("refs/remotes/{remote}/{name}"),
        target: id(target),
        kind: domain::BranchKind::Remote {
            remote: remote.into(),
        },
    }
}

/// A reader over fixed data. `history` serves `commits` for every scope.
/// `files` holds the file contents by revision and path; a file larger than
/// the requested limit is served as `TooLarge`.
pub struct FakeReader {
    pub head: domain::Head,
    pub branches: Vec<domain::Branch>,
    pub remotes: Vec<domain::Remote>,
    pub tags: Vec<domain::Tag>,
    pub commits: Vec<domain::Commit>,
    pub work_tree: Vec<domain::FileChange>,
    pub files: HashMap<(domain::Revision, String), Vec<u8>>,
    pub scopes: Mutex<Vec<application::HistoryScope>>,
    /// Every `file_content` call as (revision, path).
    pub reads: Mutex<Vec<(domain::Revision, String)>>,
}

impl FakeReader {
    pub fn with_file(mut self, revision: domain::Revision, path: &str, content: &str) -> Self {
        self.files
            .insert((revision, path.to_owned()), content.as_bytes().to_vec());
        self
    }

    pub fn reads(&self) -> Vec<(domain::Revision, String)> {
        self.reads.lock().unwrap().clone()
    }
}

impl Default for FakeReader {
    fn default() -> Self {
        Self {
            head: domain::Head::Unborn {
                name: "main".into(),
            },
            branches: Vec::new(),
            remotes: Vec::new(),
            tags: Vec::new(),
            commits: Vec::new(),
            work_tree: Vec::new(),
            files: HashMap::new(),
            scopes: Mutex::new(Vec::new()),
            reads: Mutex::new(Vec::new()),
        }
    }
}

struct FakeCursor {
    commits: std::vec::IntoIter<domain::Commit>,
}

impl application::HistoryCursor for FakeCursor {
    fn next_page(&mut self, max: usize) -> Result<Vec<domain::Commit>, application::ReadError> {
        Ok(self.commits.by_ref().take(max).collect())
    }
}

impl application::RepositoryReader for FakeReader {
    fn head(&self, _: &RepositoryInfo) -> Result<domain::Head, application::ReadError> {
        Ok(self.head.clone())
    }

    fn branches(&self, _: &RepositoryInfo) -> Result<Vec<domain::Branch>, application::ReadError> {
        Ok(self.branches.clone())
    }

    fn remotes(&self, _: &RepositoryInfo) -> Result<Vec<domain::Remote>, application::ReadError> {
        Ok(self.remotes.clone())
    }

    fn tags(&self, _: &RepositoryInfo) -> Result<Vec<domain::Tag>, application::ReadError> {
        Ok(self.tags.clone())
    }

    fn history(
        &self,
        _: &RepositoryInfo,
        scope: &application::HistoryScope,
    ) -> Result<Box<dyn application::HistoryCursor>, application::ReadError> {
        self.scopes.lock().unwrap().push(scope.clone());
        Ok(Box::new(FakeCursor {
            commits: self.commits.clone().into_iter(),
        }))
    }

    fn commit_details(
        &self,
        _: &RepositoryInfo,
        id: domain::CommitId,
    ) -> Result<domain::CommitDetails, application::ReadError> {
        Err(application::ReadError::CommitNotFound(id))
    }

    fn work_tree_changes(
        &self,
        _: &RepositoryInfo,
    ) -> Result<Vec<domain::FileChange>, application::ReadError> {
        Ok(self.work_tree.clone())
    }

    fn file_content(
        &self,
        _: &RepositoryInfo,
        revision: domain::Revision,
        path: &str,
        limit: u64,
    ) -> Result<Option<domain::FileContent>, application::ReadError> {
        self.reads.lock().unwrap().push((revision, path.to_owned()));
        Ok(self
            .files
            .get(&(revision, path.to_owned()))
            .map(|bytes| match bytes.len() as u64 {
                size if size > limit => domain::FileContent::TooLarge { size },
                _ => domain::FileContent::Bytes(bytes.clone()),
            }))
    }
}
