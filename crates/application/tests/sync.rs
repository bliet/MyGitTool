mod common;

use application::{
    AbortOperation, Fetch, FetchTarget, ForcePushPlan, Integration, IntegrationOutcome,
    OperationError, Pull, Push, PushDestination, PushPlan, RemoteError,
};
use common::{FakeReader, FakeRemote, FakeWriter, Silent, id};
use domain::{
    DirtyWorkTree, FastForward, Head, OngoingOperation, PullConfig, Remote, RepositoryInfo,
    Tracking,
};
use std::sync::Arc;

fn repo() -> RepositoryInfo {
    RepositoryInfo::new("/r")
}

fn remote(name: &str) -> Remote {
    Remote {
        name: name.into(),
        url: Some(format!("https://example.com/{name}.git")),
    }
}

fn tracking(remote: &str, branch: &str) -> Tracking {
    Tracking {
        remote: remote.into(),
        merge_ref: format!("refs/heads/{branch}"),
        tracking_ref: format!("refs/remotes/{remote}/{branch}"),
    }
}

/// On `main` at commit 1, tracking `origin/main`, which is at commit 2
/// after the fetch.
fn on_main() -> FakeReader {
    let mut reader = FakeReader {
        head: Head::Branch {
            name: "main".into(),
            target: id(1),
        },
        remotes: vec![remote("origin")],
        ..FakeReader::default()
    };
    reader
        .tracking
        .insert("main".into(), tracking("origin", "main"));
    reader.refs.insert("refs/remotes/origin/main".into(), id(2));
    reader
}

struct Setup {
    remote: Arc<FakeRemote>,
    writer: Arc<FakeWriter>,
    pull: Pull,
    push: Push,
}

fn setup(reader: FakeReader, writer: FakeWriter, remote: FakeRemote) -> Setup {
    let reader = Arc::new(reader);
    let writer = Arc::new(writer);
    let remote = Arc::new(remote);
    Setup {
        pull: Pull::new(reader.clone(), writer.clone(), remote.clone()),
        push: Push::new(reader, writer.clone(), remote.clone()),
        remote,
        writer,
    }
}

fn pull(reader: FakeReader) -> (Result<IntegrationOutcome, OperationError>, Setup) {
    pull_with(reader, FakeWriter::default())
}

fn pull_with(
    reader: FakeReader,
    writer: FakeWriter,
) -> (Result<IntegrationOutcome, OperationError>, Setup) {
    let setup = setup(reader, writer, FakeRemote::default());
    let result = setup.pull.pull(&repo(), &Silent, &Silent);
    (result, setup)
}

// Fetch

#[test]
fn fetch_all_fetches_every_remote() {
    let reader = FakeReader {
        remotes: vec![remote("origin"), remote("upstream")],
        ..FakeReader::default()
    };
    let remote = Arc::new(FakeRemote::default());
    let fetched = Fetch::new(Arc::new(reader), remote.clone())
        .fetch(&repo(), &FetchTarget::All, &Silent, &Silent)
        .unwrap();
    assert_eq!(fetched, ["origin", "upstream"]);
    assert_eq!(remote.calls(), ["fetch origin", "fetch upstream"]);
}

#[test]
fn fetch_all_without_remotes_fails() {
    let fetch = Fetch::new(
        Arc::new(FakeReader::default()),
        Arc::new(FakeRemote::default()),
    );
    let result = fetch.fetch(&repo(), &FetchTarget::All, &Silent, &Silent);
    assert_eq!(result, Err(OperationError::NoRemotes));
}

#[test]
fn fetch_on_a_local_branch_fetches_its_upstreams_remote() {
    let mut reader = FakeReader::default();
    reader
        .tracking
        .insert("feature".into(), tracking("upstream", "feature"));
    let remote = Arc::new(FakeRemote::default());
    Fetch::new(Arc::new(reader), remote.clone())
        .fetch(
            &repo(),
            &FetchTarget::Branch("feature".into()),
            &Silent,
            &Silent,
        )
        .unwrap();
    assert_eq!(remote.calls(), ["fetch upstream"]);
}

#[test]
fn fetch_on_a_branch_without_upstream_fails() {
    let fetch = Fetch::new(
        Arc::new(FakeReader::default()),
        Arc::new(FakeRemote::default()),
    );
    let result = fetch.fetch(&repo(), &FetchTarget::Branch("x".into()), &Silent, &Silent);
    assert_eq!(result, Err(OperationError::NoUpstream("x".into())));
}

#[test]
fn fetch_errors_are_passed_on() {
    let remote = FakeRemote {
        error: Some(RemoteError::Cancelled),
        ..FakeRemote::default()
    };
    let fetch = Fetch::new(Arc::new(FakeReader::default()), Arc::new(remote));
    let result = fetch.fetch(
        &repo(),
        &FetchTarget::Remote("origin".into()),
        &Silent,
        &Silent,
    );
    assert_eq!(result, Err(OperationError::Remote(RemoteError::Cancelled)));
}

// Pull: preconditions

#[test]
fn pull_needs_a_current_branch() {
    let reader = FakeReader {
        head: Head::Detached(id(1)),
        ..on_main()
    };
    assert_eq!(pull(reader).0, Err(OperationError::NoCurrentBranch));
}

#[test]
fn pull_refuses_while_a_merge_is_in_progress() {
    let reader = FakeReader {
        ongoing: Some(OngoingOperation::Merge),
        ..on_main()
    };
    assert_eq!(
        pull(reader).0,
        Err(OperationError::OperationInProgress(OngoingOperation::Merge))
    );
}

#[test]
fn pull_needs_an_upstream() {
    let mut reader = on_main();
    reader.tracking.clear();
    assert_eq!(
        pull(reader).0,
        Err(OperationError::NoUpstream("main".into()))
    );
}

#[test]
fn pull_refuses_a_dirty_work_tree_before_fetching() {
    let reader = FakeReader {
        dirty: vec!["a.txt".into(), "src/b.rs".into()],
        ..on_main()
    };
    let (result, setup) = pull(reader);
    assert_eq!(
        result,
        Err(OperationError::DirtyWorkTree(DirtyWorkTree {
            files: vec!["a.txt".into(), "src/b.rs".into()]
        }))
    );
    assert!(setup.remote.calls().is_empty());
    assert!(setup.writer.calls().is_empty());
}

#[test]
fn pull_fails_if_the_upstream_is_gone_after_fetching() {
    let mut reader = on_main();
    reader.refs.clear();
    let (result, setup) = pull(reader);
    assert_eq!(
        result,
        Err(OperationError::UpstreamGone("refs/heads/main".into()))
    );
    assert_eq!(setup.remote.calls(), ["fetch origin"]);
}

// Pull: integration

#[test]
fn pull_when_nothing_is_new_is_up_to_date() {
    let reader = FakeReader {
        ahead_behind: (3, 0),
        ..on_main()
    };
    let (result, setup) = pull(reader);
    assert_eq!(result, Ok(IntegrationOutcome::UpToDate));
    assert_eq!(setup.remote.calls(), ["fetch origin"]);
    assert!(setup.writer.calls().is_empty());
}

#[test]
fn pull_fast_forwards_when_possible() {
    let reader = FakeReader {
        ahead_behind: (0, 2),
        ..on_main()
    };
    let (result, setup) = pull(reader);
    assert_eq!(result, Ok(IntegrationOutcome::FastForwarded { to: id(2) }));
    assert_eq!(
        setup.writer.calls(),
        [format!("fast_forward main {}", id(2).short())]
    );
}

#[test]
fn pull_with_ff_false_merges_even_when_a_fast_forward_is_possible() {
    let reader = FakeReader {
        ahead_behind: (0, 2),
        pull_config: PullConfig {
            rebase: false,
            fast_forward: FastForward::Never,
        },
        ..on_main()
    };
    let (result, setup) = pull(reader);
    assert_eq!(result, Ok(IntegrationOutcome::Merged));
    assert!(setup.writer.calls()[0].starts_with("merge "));
}

#[test]
fn pull_with_rebase_fast_forwards_when_possible() {
    let reader = FakeReader {
        ahead_behind: (0, 2),
        pull_config: PullConfig {
            rebase: true,
            fast_forward: FastForward::Never,
        },
        ..on_main()
    };
    assert_eq!(
        pull(reader).0,
        Ok(IntegrationOutcome::FastForwarded { to: id(2) })
    );
}

#[test]
fn diverged_pull_merges_by_default_with_gits_message() {
    let reader = FakeReader {
        ahead_behind: (1, 2),
        ..on_main()
    };
    let (result, setup) = pull(reader);
    assert_eq!(result, Ok(IntegrationOutcome::Merged));
    assert_eq!(
        setup.writer.calls(),
        [format!(
            "merge {} Merge branch 'main' of https://example.com/origin.git",
            id(2).short()
        )]
    );
}

#[test]
fn diverged_pull_rebases_when_configured() {
    let reader = FakeReader {
        ahead_behind: (1, 2),
        pull_config: PullConfig {
            rebase: true,
            fast_forward: FastForward::Allow,
        },
        ..on_main()
    };
    let (result, setup) = pull(reader);
    assert_eq!(result, Ok(IntegrationOutcome::Rebased));
    assert_eq!(setup.writer.calls(), [format!("rebase {}", id(2).short())]);
}

#[test]
fn diverged_pull_with_ff_only_fails_without_changes() {
    let reader = FakeReader {
        ahead_behind: (1, 2),
        pull_config: PullConfig {
            rebase: true,
            fast_forward: FastForward::Only,
        },
        ..on_main()
    };
    let (result, setup) = pull(reader);
    assert_eq!(result, Err(OperationError::NotFastForward("main".into())));
    assert!(setup.writer.calls().is_empty());
}

#[test]
fn conflicts_are_reported_with_the_operation_left_in_progress() {
    let conflicts = || FakeWriter {
        integration: Integration::Conflicts(vec!["a.txt".into()]),
        ..FakeWriter::default()
    };
    let merging = FakeReader {
        ahead_behind: (1, 2),
        ..on_main()
    };
    assert_eq!(
        pull_with(merging, conflicts()).0,
        Ok(IntegrationOutcome::Conflicts {
            operation: OngoingOperation::Merge,
            files: vec!["a.txt".into()]
        })
    );
    let rebasing = FakeReader {
        ahead_behind: (1, 2),
        pull_config: PullConfig {
            rebase: true,
            fast_forward: FastForward::Allow,
        },
        ..on_main()
    };
    let (result, setup) = pull_with(rebasing, conflicts());
    assert_eq!(
        result,
        Ok(IntegrationOutcome::Conflicts {
            operation: OngoingOperation::Rebase,
            files: vec!["a.txt".into()]
        })
    );
    assert!(!setup.writer.calls().contains(&"abort".to_owned()));
}

#[test]
fn abort_aborts_through_the_writer() {
    let writer = Arc::new(FakeWriter::default());
    AbortOperation::new(writer.clone()).abort(&repo()).unwrap();
    assert_eq!(writer.calls(), ["abort"]);
}

// Push

#[test]
fn push_plan_uses_the_upstream() {
    let setup = setup(on_main(), FakeWriter::default(), FakeRemote::default());
    assert_eq!(
        setup.push.plan(&repo(), "main"),
        Ok(PushPlan::Upstream(PushDestination {
            remote: "origin".into(),
            remote_ref: "refs/heads/main".into(),
            set_upstream: false,
        }))
    );
}

#[test]
fn push_plan_without_upstream_offers_the_remotes_origin_first() {
    let reader = FakeReader {
        remotes: vec![remote("fork"), remote("origin"), remote("upstream")],
        ..FakeReader::default()
    };
    let setup = setup(reader, FakeWriter::default(), FakeRemote::default());
    assert_eq!(
        setup.push.plan(&repo(), "topic"),
        Ok(PushPlan::NoUpstream {
            branch: "topic".into(),
            remotes: vec!["origin".into(), "fork".into(), "upstream".into()],
        })
    );
}

#[test]
fn push_plan_without_any_remote_fails() {
    let setup = setup(
        FakeReader::default(),
        FakeWriter::default(),
        FakeRemote::default(),
    );
    assert_eq!(
        setup.push.plan(&repo(), "topic"),
        Err(OperationError::NoRemotes)
    );
}

#[test]
fn push_to_the_upstream_does_not_touch_the_config() {
    let setup = setup(on_main(), FakeWriter::default(), FakeRemote::default());
    let PushPlan::Upstream(destination) = setup.push.plan(&repo(), "main").unwrap() else {
        panic!("expected an upstream");
    };
    setup
        .push
        .push(&repo(), "main", &destination, &Silent, &Silent)
        .unwrap();
    assert_eq!(setup.remote.calls(), ["push main origin refs/heads/main"]);
    assert!(setup.writer.calls().is_empty());
}

#[test]
fn push_with_a_new_upstream_sets_it_after_pushing() {
    let setup = setup(
        FakeReader::default(),
        FakeWriter::default(),
        FakeRemote::default(),
    );
    let destination = Push::new_upstream("topic", "origin");
    setup
        .push
        .push(&repo(), "topic", &destination, &Silent, &Silent)
        .unwrap();
    assert_eq!(setup.remote.calls(), ["push topic origin refs/heads/topic"]);
    assert_eq!(
        setup.writer.calls(),
        ["set_tracking topic origin refs/heads/topic"]
    );
}

#[test]
fn a_rejected_push_sets_no_upstream() {
    let rejected = RemoteError::Rejected {
        reference: "refs/heads/topic".into(),
        message: "non-fast-forward".into(),
    };
    let remote = FakeRemote {
        error: Some(rejected.clone()),
        ..FakeRemote::default()
    };
    let setup = setup(FakeReader::default(), FakeWriter::default(), remote);
    let destination = Push::new_upstream("topic", "origin");
    let result = setup
        .push
        .push(&repo(), "topic", &destination, &Silent, &Silent);
    assert_eq!(result, Err(OperationError::Remote(rejected)));
    assert!(setup.writer.calls().is_empty());
}

#[test]
fn the_current_branch_is_pushed_from_the_toolbar() {
    let setup = setup(on_main(), FakeWriter::default(), FakeRemote::default());
    assert_eq!(setup.push.current_branch(&repo()), Ok("main".into()));
    let detached = FakeReader {
        head: Head::Detached(id(1)),
        ..FakeReader::default()
    };
    let setup = self::setup(detached, FakeWriter::default(), FakeRemote::default());
    assert_eq!(
        setup.push.current_branch(&repo()),
        Err(OperationError::NoCurrentBranch)
    );
}

// Force push

/// `on_main`, with the branch ref itself, and main and origin/main
/// diverged by one commit on each side.
fn diverged() -> FakeReader {
    let mut reader = on_main();
    reader.refs.insert("refs/heads/main".into(), id(1));
    reader.ahead_behind = (1, 3);
    reader
}

#[test]
fn the_force_push_plan_names_the_lease_and_what_is_discarded() {
    let setup = setup(diverged(), FakeWriter::default(), FakeRemote::default());
    assert_eq!(
        setup.push.force_plan(&repo(), "main"),
        Ok(ForcePushPlan {
            branch: "main".into(),
            remote: "origin".into(),
            remote_ref: "refs/heads/main".into(),
            tracking: "origin/main".into(),
            expected: Some(id(2)),
            discarded: 3,
        })
    );
}

#[test]
fn a_remote_branch_never_fetched_is_leased_as_absent() {
    let mut reader = diverged();
    reader.refs.remove("refs/remotes/origin/main");
    let setup = setup(reader, FakeWriter::default(), FakeRemote::default());
    let plan = setup.push.force_plan(&repo(), "main").unwrap();
    assert_eq!((plan.expected, plan.discarded), (None, 0));
}

#[test]
fn force_pushing_needs_an_upstream() {
    let mut reader = diverged();
    reader.tracking.clear();
    let setup = setup(reader, FakeWriter::default(), FakeRemote::default());
    assert_eq!(
        setup.push.force_plan(&repo(), "main"),
        Err(OperationError::NoUpstream("main".into()))
    );
}

#[test]
fn the_force_push_uses_the_confirmed_lease() {
    let setup = setup(diverged(), FakeWriter::default(), FakeRemote::default());
    let plan = setup.push.force_plan(&repo(), "main").unwrap();
    setup
        .push
        .force_push(&repo(), &plan, &Silent, &Silent)
        .unwrap();
    assert_eq!(
        setup.remote.calls(),
        [format!(
            "force_push main origin refs/heads/main lease={}",
            id(2).short()
        )]
    );
    assert!(setup.writer.calls().is_empty());
}
