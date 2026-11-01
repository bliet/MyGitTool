//! Hand-written fakes of the port traits, shared by the use case tests.
#![allow(dead_code)]

use application::{
    LayoutStore, LocateError, RecentRepositoriesStore, RepoChanges, RepoWatcher, RepositoryLocator,
    Settings, SettingsStore, StorageError, WatchError, WatchGuard, WindowLayout,
};
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

pub fn stash(index: usize, n: u8) -> domain::Stash {
    domain::Stash {
        index,
        message: format!("On main: stash {n}"),
        id: id(n),
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
    pub stashes: Vec<domain::Stash>,
    pub commits: Vec<domain::Commit>,
    pub work_tree: domain::WorkTreeChanges,
    pub patch: Vec<u8>,
    pub files: HashMap<(domain::Revision, String), Vec<u8>>,
    /// Upstream config by local branch name.
    pub tracking: HashMap<String, domain::Tracking>,
    /// Ref targets by full ref name.
    pub refs: HashMap<String, domain::CommitId>,
    /// What `ahead_behind` answers for any pair.
    pub ahead_behind: (usize, usize),
    pub dirty: Vec<String>,
    pub ongoing: Option<domain::OngoingOperation>,
    pub pull_config: domain::PullConfig,
    /// What `changed_paths` answers for any pair, sorted.
    pub changed: Vec<String>,
    pub conflicts: Vec<String>,
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
            stashes: Vec::new(),
            commits: Vec::new(),
            work_tree: domain::WorkTreeChanges::default(),
            patch: Vec::new(),
            files: HashMap::new(),
            tracking: HashMap::new(),
            refs: HashMap::new(),
            ahead_behind: (0, 0),
            dirty: Vec::new(),
            ongoing: None,
            pull_config: domain::PullConfig::default(),
            changed: Vec::new(),
            conflicts: Vec::new(),
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

    fn stashes(&self, _: &RepositoryInfo) -> Result<Vec<domain::Stash>, application::ReadError> {
        Ok(self.stashes.clone())
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
    ) -> Result<domain::WorkTreeChanges, application::ReadError> {
        Ok(self.work_tree.clone())
    }

    fn work_tree_patch(&self, _: &RepositoryInfo) -> Result<Vec<u8>, application::ReadError> {
        Ok(self.patch.clone())
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

    fn tracking(
        &self,
        _: &RepositoryInfo,
        branch: &str,
    ) -> Result<Option<domain::Tracking>, application::ReadError> {
        Ok(self.tracking.get(branch).cloned())
    }

    fn resolve(
        &self,
        _: &RepositoryInfo,
        refname: &str,
    ) -> Result<Option<domain::CommitId>, application::ReadError> {
        Ok(self.refs.get(refname).copied())
    }

    fn ahead_behind(
        &self,
        _: &RepositoryInfo,
        _: domain::CommitId,
        _: domain::CommitId,
    ) -> Result<(usize, usize), application::ReadError> {
        Ok(self.ahead_behind)
    }

    fn dirty_files(&self, _: &RepositoryInfo) -> Result<Vec<String>, application::ReadError> {
        Ok(self.dirty.clone())
    }

    fn ongoing_operation(
        &self,
        _: &RepositoryInfo,
    ) -> Result<Option<domain::OngoingOperation>, application::ReadError> {
        Ok(self.ongoing)
    }

    fn pull_config(
        &self,
        _: &RepositoryInfo,
        _: &str,
    ) -> Result<domain::PullConfig, application::ReadError> {
        Ok(self.pull_config)
    }

    fn changed_paths(
        &self,
        _: &RepositoryInfo,
        _: domain::CommitId,
        _: domain::CommitId,
    ) -> Result<Vec<String>, application::ReadError> {
        Ok(self.changed.clone())
    }

    fn conflicted_files(&self, _: &RepositoryInfo) -> Result<Vec<String>, application::ReadError> {
        Ok(self.conflicts.clone())
    }
}

/// A writer that records its calls, e.g. `merge <id> <message>`, and
/// answers merges and rebases with `integration`.
pub struct FakeWriter {
    pub calls: Mutex<Vec<String>>,
    pub integration: application::Integration,
}

impl Default for FakeWriter {
    fn default() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            integration: application::Integration::Completed,
        }
    }
}

impl FakeWriter {
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
}

impl application::RepositoryWriter for FakeWriter {
    fn fast_forward(
        &self,
        _: &RepositoryInfo,
        branch: &str,
        target: domain::CommitId,
    ) -> Result<(), application::WriteError> {
        self.record(format!("fast_forward {branch} {}", target.short()));
        Ok(())
    }

    fn merge(
        &self,
        _: &RepositoryInfo,
        commit: domain::CommitId,
        message: &str,
    ) -> Result<application::Integration, application::WriteError> {
        self.record(format!("merge {} {message}", commit.short()));
        Ok(self.integration.clone())
    }

    fn rebase(
        &self,
        _: &RepositoryInfo,
        upstream: domain::CommitId,
    ) -> Result<application::Integration, application::WriteError> {
        self.record(format!("rebase {}", upstream.short()));
        Ok(self.integration.clone())
    }

    fn abort(&self, _: &RepositoryInfo) -> Result<(), application::WriteError> {
        self.record("abort".to_owned());
        Ok(())
    }

    fn set_tracking(
        &self,
        _: &RepositoryInfo,
        branch: &str,
        remote: &str,
        merge_ref: &str,
    ) -> Result<(), application::WriteError> {
        self.record(format!("set_tracking {branch} {remote} {merge_ref}"));
        Ok(())
    }

    fn checkout(&self, _: &RepositoryInfo, branch: &str) -> Result<(), application::WriteError> {
        self.record(format!("checkout {branch}"));
        Ok(())
    }

    fn create_branch(
        &self,
        _: &RepositoryInfo,
        name: &str,
        target: domain::CommitId,
    ) -> Result<(), application::WriteError> {
        self.record(format!("create_branch {name} {}", target.short()));
        Ok(())
    }

    fn delete_branch(&self, _: &RepositoryInfo, name: &str) -> Result<(), application::WriteError> {
        self.record(format!("delete_branch {name}"));
        Ok(())
    }

    fn create_tag(
        &self,
        _: &RepositoryInfo,
        name: &str,
        target: domain::CommitId,
        message: Option<&str>,
    ) -> Result<(), application::WriteError> {
        match message {
            Some(message) => {
                self.record(format!("create_tag {name} {} {message:?}", target.short()))
            }
            None => self.record(format!("create_tag {name} {}", target.short())),
        }
        Ok(())
    }

    fn delete_tag(&self, _: &RepositoryInfo, name: &str) -> Result<(), application::WriteError> {
        self.record(format!("delete_tag {name}"));
        Ok(())
    }

    fn continue_operation(
        &self,
        _: &RepositoryInfo,
    ) -> Result<application::Integration, application::WriteError> {
        self.record("continue".to_owned());
        Ok(self.integration.clone())
    }

    fn stage(&self, _: &RepositoryInfo, paths: &[String]) -> Result<(), application::WriteError> {
        self.record(format!("stage {}", paths.join(" ")));
        Ok(())
    }

    fn unstage(&self, _: &RepositoryInfo, paths: &[String]) -> Result<(), application::WriteError> {
        self.record(format!("unstage {}", paths.join(" ")));
        Ok(())
    }

    fn commit(
        &self,
        _: &RepositoryInfo,
        message: &str,
    ) -> Result<domain::CommitId, application::WriteError> {
        self.record(format!("commit {message:?}"));
        Ok(id(9))
    }

    fn stash_save(
        &self,
        _: &RepositoryInfo,
        message: Option<&str>,
        include_untracked: bool,
    ) -> Result<(), application::WriteError> {
        self.record(format!(
            "stash_save {message:?} untracked={include_untracked}"
        ));
        Ok(())
    }

    fn stash_apply(
        &self,
        _: &RepositoryInfo,
        index: usize,
    ) -> Result<application::Integration, application::WriteError> {
        self.record(format!("stash_apply {index}"));
        Ok(self.integration.clone())
    }

    fn stash_drop(&self, _: &RepositoryInfo, index: usize) -> Result<(), application::WriteError> {
        self.record(format!("stash_drop {index}"));
        Ok(())
    }

    fn apply_patch(&self, _: &RepositoryInfo, patch: &[u8]) -> Result<(), application::WriteError> {
        self.record(format!("apply_patch {}", String::from_utf8_lossy(patch)));
        Ok(())
    }
}

/// Remote operations that record their calls, e.g. `fetch origin`, and
/// fail with `error` if it is set.
#[derive(Default)]
pub struct FakeRemote {
    pub calls: Mutex<Vec<String>>,
    pub error: Option<application::RemoteError>,
}

impl FakeRemote {
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn answer(&self, call: String) -> Result<(), application::RemoteError> {
        self.calls.lock().unwrap().push(call);
        self.error.clone().map_or(Ok(()), Err)
    }
}

impl application::RemoteOperations for FakeRemote {
    fn fetch(
        &self,
        _: &RepositoryInfo,
        remote: &str,
        _: &dyn application::ProgressReporter,
        _: &dyn application::CredentialProvider,
    ) -> Result<(), application::RemoteError> {
        self.answer(format!("fetch {remote}"))
    }

    fn push(
        &self,
        _: &RepositoryInfo,
        branch: &str,
        remote: &str,
        remote_ref: &str,
        _: &dyn application::ProgressReporter,
        _: &dyn application::CredentialProvider,
    ) -> Result<(), application::RemoteError> {
        self.answer(format!("push {branch} {remote} {remote_ref}"))
    }

    fn push_tag(
        &self,
        _: &RepositoryInfo,
        tag: &str,
        remote: &str,
        _: &dyn application::ProgressReporter,
        _: &dyn application::CredentialProvider,
    ) -> Result<(), application::RemoteError> {
        self.answer(format!("push_tag {tag} {remote}"))
    }

    fn delete_branch(
        &self,
        _: &RepositoryInfo,
        remote: &str,
        remote_ref: &str,
        _: &dyn application::ProgressReporter,
        _: &dyn application::CredentialProvider,
    ) -> Result<(), application::RemoteError> {
        self.answer(format!("delete {remote} {remote_ref}"))
    }
}

/// Ignores progress and never has credentials.
pub struct Silent;

impl application::ProgressReporter for Silent {
    fn report(&self, _: domain::Progress) {}
}

impl application::CredentialProvider for Silent {
    fn credentials(&self, _: &domain::CredentialRequest) -> Option<domain::Credential> {
        None
    }
}

/// An in-memory settings store; `None` until something is saved.
#[derive(Default)]
pub struct FakeSettingsStore {
    pub settings: Mutex<Option<Settings>>,
}

impl SettingsStore for FakeSettingsStore {
    fn load(&self) -> Result<Option<Settings>, StorageError> {
        Ok(*self.settings.lock().unwrap())
    }

    fn save(&self, settings: &Settings) -> Result<(), StorageError> {
        *self.settings.lock().unwrap() = Some(*settings);
        Ok(())
    }
}

/// An in-memory layout store; `None` until something is saved.
#[derive(Default)]
pub struct FakeLayoutStore {
    pub layout: Mutex<Option<WindowLayout>>,
}

impl LayoutStore for FakeLayoutStore {
    fn load(&self) -> Result<Option<WindowLayout>, StorageError> {
        Ok(*self.layout.lock().unwrap())
    }

    fn save(&self, layout: &WindowLayout) -> Result<(), StorageError> {
        *self.layout.lock().unwrap() = Some(*layout);
        Ok(())
    }
}

type ChangeCallback = Box<dyn Fn(RepoChanges) + Send>;

/// A watcher that keeps the callback of the last watch, so a test can
/// report changes through it. Watching `failing` fails.
#[derive(Default)]
pub struct FakeWatcher {
    pub callback: std::sync::Arc<Mutex<Option<ChangeCallback>>>,
    pub watched: Mutex<Vec<PathBuf>>,
    pub failing: bool,
}

impl FakeWatcher {
    pub fn report(&self, changes: RepoChanges) {
        if let Some(callback) = &*self.callback.lock().unwrap() {
            callback(changes);
        }
    }
}

impl RepoWatcher for FakeWatcher {
    fn watch(
        &self,
        repo: &RepositoryInfo,
        on_change: ChangeCallback,
    ) -> Result<WatchGuard, WatchError> {
        if self.failing {
            return Err(WatchError("no watches left".into()));
        }
        self.watched.lock().unwrap().push(repo.root().to_path_buf());
        *self.callback.lock().unwrap() = Some(on_change);
        // Dropping the guard forgets the callback, like a stopped watch.
        let callback = self.callback.clone();
        Ok(WatchGuard::new(DropFn(Some(Box::new(move || {
            *callback.lock().unwrap() = None;
        })))))
    }
}

struct DropFn(Option<Box<dyn FnOnce() + Send>>);

impl Drop for DropFn {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f();
        }
    }
}
