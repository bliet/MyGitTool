//! Fetch, pull and push against a local bare repository as the remote.

mod common;

use application::{
    CredentialProvider, Fetch, FetchTarget, ProgressReporter, Pull, PullOutcome, Push, PushPlan,
    RemoteError, RepositoryReader, SyncError,
};
use common::TestRepo;
use domain::{
    CommitId, Credential, CredentialRequest, DirtyWorkTree, FastForward, OngoingOperation,
    Progress, PullConfig, RepositoryInfo,
};
use infra_git::{GitRemoteOperations, GitRepositoryReader, GitRepositoryWriter};
use std::fs;
use std::sync::Arc;

struct Silent;

impl ProgressReporter for Silent {
    fn report(&self, _: Progress) {}
}

impl CredentialProvider for Silent {
    fn credentials(&self, _: &CredentialRequest) -> Option<Credential> {
        None
    }
}

fn info(repo: &TestRepo) -> RepositoryInfo {
    RepositoryInfo::new(repo.path())
}

fn id(oid: git2::Oid) -> CommitId {
    CommitId::from_slice(oid.as_bytes()).unwrap()
}

fn pull_case() -> Pull {
    Pull::new(
        Arc::new(GitRepositoryReader),
        Arc::new(GitRepositoryWriter),
        Arc::new(GitRemoteOperations),
    )
}

fn push_case() -> Push {
    Push::new(
        Arc::new(GitRepositoryReader),
        Arc::new(GitRepositoryWriter),
        Arc::new(GitRemoteOperations),
    )
}

fn pull(repo: &TestRepo) -> Result<PullOutcome, SyncError> {
    pull_case().pull(&info(repo), &Silent, &Silent)
}

/// Pushes the current branch, setting `origin` as its upstream if needed.
fn push(repo: &TestRepo) -> Result<(), SyncError> {
    let push = push_case();
    let branch = push.current_branch(&info(repo))?;
    let destination = match push.plan(&info(repo), &branch)? {
        PushPlan::Upstream(destination) => destination,
        PushPlan::NoUpstream { remotes, .. } => Push::new_upstream(&branch, &remotes[0]),
    };
    push.push(&info(repo), &branch, &destination, &Silent, &Silent)
}

/// A bare remote and a clone of it with one pushed commit on `main`, which
/// tracks `origin/main`.
fn remote_and_local() -> (TestRepo, TestRepo) {
    let remote = TestRepo::bare();
    let local = TestRepo::new()
        .commit("a.txt", "base\n")
        .remote("origin", remote.path().to_str().unwrap());
    push(&local).unwrap();
    (remote, local)
}

/// Another clone that commits `content` to `file` and pushes it.
fn push_from_elsewhere(remote: &TestRepo, file: &str, content: &str) -> git2::Oid {
    let other = TestRepo::clone_of(remote.path()).commit(file, content);
    push(&other).unwrap();
    other.head()
}

fn ref_target(repo: &TestRepo, refname: &str) -> Option<git2::Oid> {
    repo.git()
        .find_reference(refname)
        .ok()
        .and_then(|r| r.target())
}

// Push

#[test]
fn first_push_creates_the_branch_and_sets_the_upstream() {
    let (remote, local) = remote_and_local();
    assert_eq!(ref_target(&remote, "refs/heads/main"), Some(local.head()));
    assert_eq!(
        ref_target(&local, "refs/remotes/origin/main"),
        Some(local.head())
    );
    let tracking = GitRepositoryReader
        .tracking(&info(&local), "main")
        .unwrap()
        .unwrap();
    assert_eq!(tracking.remote, "origin");
    assert_eq!(tracking.merge_ref, "refs/heads/main");
    assert_eq!(tracking.tracking_ref, "refs/remotes/origin/main");
}

#[test]
fn push_to_the_upstream_sends_new_commits() {
    let (remote, local) = remote_and_local();
    let local = local.commit("a.txt", "second\n");
    push(&local).unwrap();
    assert_eq!(ref_target(&remote, "refs/heads/main"), Some(local.head()));
}

#[test]
fn push_never_overwrites_commits_on_the_remote() {
    let (remote, local) = remote_and_local();
    let theirs = push_from_elsewhere(&remote, "b.txt", "theirs\n");
    let local = local.commit("a.txt", "ours\n");
    let result = push(&local);
    assert!(
        matches!(result, Err(SyncError::Remote(RemoteError::Rejected { .. }))),
        "got {result:?}"
    );
    assert_eq!(ref_target(&remote, "refs/heads/main"), Some(theirs));
}

#[test]
fn branch_without_upstream_offers_the_remotes() {
    let (_remote, local) = remote_and_local();
    let local = local.branch("topic");
    let plan = push_case().plan(&info(&local), "topic").unwrap();
    assert_eq!(
        plan,
        PushPlan::NoUpstream {
            branch: "topic".into(),
            remotes: vec!["origin".into()]
        }
    );
}

// Fetch

#[test]
fn fetch_updates_the_remote_tracking_branches() {
    let (remote, local) = remote_and_local();
    let theirs = push_from_elsewhere(&remote, "b.txt", "new\n");
    let fetch = Fetch::new(Arc::new(GitRepositoryReader), Arc::new(GitRemoteOperations));
    let fetched = fetch
        .fetch(
            &info(&local),
            &FetchTarget::Branch("main".into()),
            &Silent,
            &Silent,
        )
        .unwrap();
    assert_eq!(fetched, ["origin"]);
    assert_eq!(ref_target(&local, "refs/remotes/origin/main"), Some(theirs));
    // Fetching never touches the local branch.
    assert_ne!(local.head(), theirs);
}

#[test]
fn fetch_of_an_unknown_remote_fails() {
    let (_remote, local) = remote_and_local();
    let fetch = Fetch::new(Arc::new(GitRepositoryReader), Arc::new(GitRemoteOperations));
    let result = fetch.fetch(
        &info(&local),
        &FetchTarget::Remote("nope".into()),
        &Silent,
        &Silent,
    );
    assert_eq!(
        result,
        Err(SyncError::Remote(RemoteError::UnknownRemote("nope".into())))
    );
}

// Pull

#[test]
fn pull_without_new_commits_is_up_to_date() {
    let (_remote, local) = remote_and_local();
    assert_eq!(pull(&local), Ok(PullOutcome::UpToDate));
}

#[test]
fn pull_fast_forwards_branch_and_work_tree() {
    let (remote, local) = remote_and_local();
    let theirs = push_from_elsewhere(&remote, "b.txt", "from elsewhere\n");
    assert_eq!(
        pull(&local),
        Ok(PullOutcome::FastForwarded { to: id(theirs) })
    );
    assert_eq!(local.head(), theirs);
    assert_eq!(
        fs::read_to_string(local.path().join("b.txt")).unwrap(),
        "from elsewhere\n"
    );
    assert!(local.git().head().unwrap().is_branch());
}

#[test]
fn diverged_pull_creates_a_merge_commit() {
    let (remote, local) = remote_and_local();
    let theirs = push_from_elsewhere(&remote, "b.txt", "theirs\n");
    let local = local.commit("c.txt", "ours\n");
    let ours = local.head();
    assert_eq!(pull(&local), Ok(PullOutcome::Merged));

    let merge = local.git().find_commit(local.head()).unwrap();
    let parents: Vec<_> = merge.parent_ids().collect();
    assert_eq!(parents, [ours, theirs]);
    assert!(
        merge
            .message()
            .unwrap()
            .starts_with("Merge branch 'main' of ")
    );
    assert!(local.path().join("b.txt").exists());
    assert!(local.path().join("c.txt").exists());
    assert_eq!(local.git().state(), git2::RepositoryState::Clean);
}

#[test]
fn diverged_pull_rebases_when_pull_rebase_is_set() {
    let (remote, local) = remote_and_local();
    let theirs = push_from_elsewhere(&remote, "b.txt", "theirs\n");
    let local = local
        .config("pull.rebase", "true")
        .commit("c.txt", "ours\n");
    assert_eq!(pull(&local), Ok(PullOutcome::Rebased));

    let head = local.git().find_commit(local.head()).unwrap();
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), [theirs]);
    assert_eq!(head.summary_bytes(), Some(b"Update c.txt".as_slice()));
    assert!(local.git().head().unwrap().is_branch());
    assert_eq!(local.git().state(), git2::RepositoryState::Clean);
}

#[test]
fn merge_conflicts_are_reported_and_can_be_aborted() {
    let (remote, local) = remote_and_local();
    push_from_elsewhere(&remote, "a.txt", "theirs\n");
    let local = local.commit("a.txt", "ours\n");
    let ours = local.head();

    assert_eq!(
        pull(&local),
        Ok(PullOutcome::Conflicts {
            operation: OngoingOperation::Merge,
            files: vec!["a.txt".into()]
        })
    );
    let reader = GitRepositoryReader;
    assert_eq!(
        reader.ongoing_operation(&info(&local)),
        Ok(Some(OngoingOperation::Merge))
    );

    application::AbortOperation::new(Arc::new(GitRepositoryWriter))
        .abort(&info(&local))
        .unwrap();
    assert_eq!(reader.ongoing_operation(&info(&local)), Ok(None));
    assert_eq!(local.head(), ours);
    assert_eq!(
        fs::read_to_string(local.path().join("a.txt")).unwrap(),
        "ours\n"
    );
}

#[test]
fn rebase_conflicts_are_reported_and_can_be_aborted() {
    let (remote, local) = remote_and_local();
    push_from_elsewhere(&remote, "a.txt", "theirs\n");
    let local = local
        .config("pull.rebase", "true")
        .commit("a.txt", "ours\n");
    let ours = local.head();

    assert_eq!(
        pull(&local),
        Ok(PullOutcome::Conflicts {
            operation: OngoingOperation::Rebase,
            files: vec!["a.txt".into()]
        })
    );
    assert_eq!(
        GitRepositoryReader.ongoing_operation(&info(&local)),
        Ok(Some(OngoingOperation::Rebase))
    );

    application::AbortOperation::new(Arc::new(GitRepositoryWriter))
        .abort(&info(&local))
        .unwrap();
    assert_eq!(
        GitRepositoryReader.ongoing_operation(&info(&local)),
        Ok(None)
    );
    assert_eq!(local.head(), ours);
    assert!(local.git().head().unwrap().is_branch());
    assert_eq!(
        fs::read_to_string(local.path().join("a.txt")).unwrap(),
        "ours\n"
    );
}

#[test]
fn pull_refuses_uncommitted_changes_but_ignores_untracked_files() {
    let (remote, local) = remote_and_local();
    push_from_elsewhere(&remote, "b.txt", "new\n");
    let local = local.write("untracked.txt", "x").write("a.txt", "edited\n");
    assert_eq!(
        pull(&local),
        Err(SyncError::DirtyWorkTree(DirtyWorkTree {
            files: vec!["a.txt".into()]
        }))
    );
    assert_eq!(
        fs::read_to_string(local.path().join("a.txt")).unwrap(),
        "edited\n"
    );

    let local = local.write("a.txt", "base\n");
    assert!(matches!(
        pull(&local),
        Ok(PullOutcome::FastForwarded { .. })
    ));
}

// Reader details

#[test]
fn dirty_files_lists_staged_and_unstaged_changes() {
    let repo = TestRepo::new()
        .commit("a.txt", "1")
        .commit("b.txt", "1")
        .write("a.txt", "2")
        .write("new.txt", "untracked");
    {
        let mut index = repo.git().index().unwrap();
        fs::write(repo.path().join("b.txt"), "staged").unwrap();
        index.add_path(std::path::Path::new("b.txt")).unwrap();
        index.write().unwrap();
    }
    assert_eq!(
        GitRepositoryReader.dirty_files(&info(&repo)),
        Ok(vec!["a.txt".to_owned(), "b.txt".to_owned()])
    );
}

#[test]
fn pull_config_follows_git_config() {
    let repo = TestRepo::new().commit("a.txt", "1");
    let config = |repo: &TestRepo| {
        GitRepositoryReader
            .pull_config(&info(repo), "main")
            .unwrap()
    };
    assert_eq!(config(&repo), PullConfig::default());

    let repo = repo
        .config("pull.rebase", "merges")
        .config("pull.ff", "only");
    assert_eq!(
        config(&repo),
        PullConfig {
            rebase: true,
            fast_forward: FastForward::Only
        }
    );

    let repo = repo
        .config("branch.main.rebase", "false")
        .config("pull.ff", "false");
    assert_eq!(
        config(&repo),
        PullConfig {
            rebase: false,
            fast_forward: FastForward::Never
        }
    );
}

#[test]
fn branch_without_upstream_has_no_tracking() {
    let repo = TestRepo::new().commit("a.txt", "1");
    assert_eq!(GitRepositoryReader.tracking(&info(&repo), "main"), Ok(None));
}
