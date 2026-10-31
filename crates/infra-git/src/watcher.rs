//! Watches a repository's Git directory and work tree with `notify`.

use application::{RepoChanges, RepoWatcher, WatchError, WatchGuard};
use domain::RepositoryInfo;
use git2::Repository;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread;
use std::time::{Duration, Instant};

/// How long changes have to settle before they are reported.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(300);
/// Changes that keep coming are reported at least this often.
const MAX_DELAY: Duration = Duration::from_secs(2);

/// Directories inside the Git directory whose changes never matter on
/// their own: new objects only show once a ref points to them.
const QUIET_GIT_DIRS: [&str; 2] = ["objects", "logs"];

#[derive(Debug, Clone, Copy)]
pub struct GitRepoWatcher {
    debounce: Duration,
}

impl GitRepoWatcher {
    /// A watcher that reports once changes have settled for `debounce`.
    pub fn new(debounce: Duration) -> Self {
        Self { debounce }
    }
}

impl Default for GitRepoWatcher {
    fn default() -> Self {
        Self::new(DEFAULT_DEBOUNCE)
    }
}

impl RepoWatcher for GitRepoWatcher {
    fn watch(
        &self,
        repo: &RepositoryInfo,
        on_change: Box<dyn Fn(RepoChanges) + Send>,
    ) -> Result<WatchGuard, WatchError> {
        let git = Repository::open(repo.root()).map_err(|e| WatchError(e.message().to_owned()))?;
        let classifier = Classifier::new(git);
        let (sender, events) = channel();
        let mut watcher =
            notify::recommended_watcher(sender).map_err(|e| WatchError(e.to_string()))?;
        for dir in classifier.watched_dirs() {
            watcher
                .watch(&dir, RecursiveMode::Recursive)
                .map_err(|e| WatchError(format!("{}: {e}", dir.display())))?;
        }
        let debounce = self.debounce;
        thread::Builder::new()
            .name("repo-watcher".to_owned())
            .spawn(move || report(&events, &classifier, debounce, &*on_change))
            .map_err(|e| WatchError(e.to_string()))?;
        // Dropping the watcher closes the channel, which ends the thread.
        Ok(WatchGuard::new(watcher))
    }
}

/// Reports the relevant changes from `events`, debounced, until the
/// channel closes.
fn report(
    events: &Receiver<notify::Result<Event>>,
    classifier: &Classifier,
    debounce: Duration,
    on_change: &dyn Fn(RepoChanges),
) {
    loop {
        let mut changes = RepoChanges::default();
        while !changes.any() {
            let Ok(event) = events.recv() else { return };
            changes = changes.merge(classifier.classify(event));
        }
        let deadline = Instant::now() + MAX_DELAY;
        let mut settled = Instant::now() + debounce;
        loop {
            let wait = settled
                .min(deadline)
                .saturating_duration_since(Instant::now());
            match events.recv_timeout(wait) {
                Ok(event) => {
                    let more = classifier.classify(event);
                    if more.any() {
                        changes = changes.merge(more);
                        settled = Instant::now() + debounce;
                    }
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        on_change(changes);
    }
}

/// Decides which part of the repository an event touched. Each directory is
/// kept as given and canonicalized, as backends report either form.
struct Classifier {
    git: Repository,
    git_dirs: Vec<Dir>,
    work_tree: Option<Dir>,
}

struct Dir {
    given: PathBuf,
    canonical: Option<PathBuf>,
}

impl Dir {
    fn new(path: &Path) -> Self {
        Self {
            given: path.to_path_buf(),
            canonical: path.canonicalize().ok(),
        }
    }

    /// `path` relative to this directory, or `None` if it is outside.
    fn relative(&self, path: &Path) -> Option<PathBuf> {
        if let Ok(inside) = path.strip_prefix(&self.given) {
            return Some(inside.to_path_buf());
        }
        let canonical = self.canonical.as_deref()?;
        if let Ok(inside) = path.strip_prefix(canonical) {
            return Some(inside.to_path_buf());
        }
        // The path may go through a symlink. It may also be gone (e.g. a
        // deleted file), so canonicalize its parent.
        let parent = path.parent()?.canonicalize().ok()?;
        let inside = parent.strip_prefix(canonical).ok()?;
        Some(match path.file_name() {
            Some(name) => inside.join(name),
            None => inside.to_path_buf(),
        })
    }
}

impl Classifier {
    fn new(git: Repository) -> Self {
        let mut git_dirs = vec![Dir::new(git.path())];
        if git.commondir() != git.path() {
            git_dirs.push(Dir::new(git.commondir()));
        }
        let work_tree = git.workdir().map(Dir::new);
        Self {
            git,
            git_dirs,
            work_tree,
        }
    }

    /// The directories to watch recursively. A Git directory inside the
    /// work tree is covered by the work tree's watch.
    fn watched_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self.work_tree.iter().map(|d| d.given.clone()).collect();
        for dir in &self.git_dirs {
            if !dirs.iter().any(|d| dir.given.starts_with(d)) {
                dirs.push(dir.given.clone());
            }
        }
        dirs
    }

    fn classify(&self, event: notify::Result<Event>) -> RepoChanges {
        let event = match event {
            Ok(event) => event,
            // The backend may have lost events, so anything could have changed.
            Err(_) => return RepoChanges::ALL,
        };
        if event.need_rescan() {
            return RepoChanges::ALL;
        }
        // Reads, including the app's own, change nothing.
        if matches!(event.kind, EventKind::Access(_)) {
            return RepoChanges::default();
        }
        event
            .paths
            .iter()
            .map(|path| self.classify_path(path))
            .fold(RepoChanges::default(), RepoChanges::merge)
    }

    fn classify_path(&self, path: &Path) -> RepoChanges {
        if let Some(inside) = self.git_dirs.iter().find_map(|dir| dir.relative(path)) {
            let quiet = match inside.components().next() {
                Some(Component::Normal(first)) => QUIET_GIT_DIRS.iter().any(|d| first == *d),
                _ => false,
            };
            let lock = inside.extension().is_some_and(|ext| ext == "lock");
            return RepoChanges {
                git_dir: !quiet && !lock,
                work_tree: false,
            };
        }
        match self.work_tree.as_ref().and_then(|dir| dir.relative(path)) {
            Some(inside) if !inside.as_os_str().is_empty() => RepoChanges {
                git_dir: false,
                work_tree: !self.is_ignored(&inside),
            },
            _ => RepoChanges::default(),
        }
    }

    /// Whether Git ignores `path` (relative to the work tree). A tracked file
    /// counts even if it matches an ignore rule.
    fn is_ignored(&self, path: &Path) -> bool {
        if !self.git.is_path_ignored(path).unwrap_or(false) {
            return false;
        }
        let tracked = self
            .git
            .index()
            .is_ok_and(|index| index.get_path(path, 0).is_some());
        !tracked
    }
}
