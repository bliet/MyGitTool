mod common;

use application::{
    Checkout, CheckoutTarget, ContinueOperation, CreateBranch, DeleteBranch, Integration,
    IntegrationOutcome, LoadOngoing, MergeIntoCurrent, OngoingState, OperationError,
    RebaseCurrentOnto, merge_message,
};
use common::{FakeReader, FakeRemote, FakeWriter, Silent, id};
use domain::{DirtyWorkTree, Head, InvalidBranchName, OngoingOperation, RepositoryInfo, Tracking};
use std::sync::Arc;

fn repo() -> RepositoryInfo {
    RepositoryInfo::new("/r")
}

/// On `main` at commit 1, with `feature` at 2 and `origin/feature` at 3.
fn reader() -> FakeReader {
    let mut reader = FakeReader {
        head: Head::Branch {
            name: "main".into(),
            target: id(1),
        },
        ..FakeReader::default()
    };
    reader.refs.insert("refs/heads/main".into(), id(1));
    reader.refs.insert("refs/heads/feature".into(), id(2));
    reader
        .refs
        .insert("refs/remotes/origin/feature".into(), id(3));
    reader
        .refs
        .insert("refs/remotes/origin/topic".into(), id(4));
    reader
}

fn conflicting() -> FakeWriter {
    FakeWriter {
        integration: Integration::Conflicts(vec!["a.txt".into()]),
        ..FakeWriter::default()
    }
}

// Checkout

fn checkout(
    reader: FakeReader,
    target: CheckoutTarget,
) -> (Result<String, OperationError>, Arc<FakeWriter>) {
    let writer = Arc::new(FakeWriter::default());
    let result = Checkout::new(Arc::new(reader), writer.clone()).checkout(&repo(), &target);
    (result, writer)
}

#[test]
fn checkout_switches_to_a_local_branch() {
    let (result, writer) = checkout(reader(), CheckoutTarget::Local("feature".into()));
    assert_eq!(result, Ok("feature".into()));
    assert_eq!(writer.calls(), ["checkout feature"]);
}

#[test]
fn checkout_of_the_current_branch_does_nothing() {
    let (result, writer) = checkout(reader(), CheckoutTarget::Local("main".into()));
    assert_eq!(result, Ok("main".into()));
    assert!(writer.calls().is_empty());
}

#[test]
fn checkout_of_a_missing_branch_fails() {
    let (result, _) = checkout(reader(), CheckoutTarget::Local("nope".into()));
    assert_eq!(result, Err(OperationError::BranchNotFound("nope".into())));
}

#[test]
fn checkout_keeps_changes_to_files_the_switch_does_not_touch() {
    let reader = FakeReader {
        dirty: vec!["notes.txt".into()],
        changed: vec!["src/a.rs".into()],
        ..reader()
    };
    let (result, writer) = checkout(reader, CheckoutTarget::Local("feature".into()));
    assert_eq!(result, Ok("feature".into()));
    assert_eq!(writer.calls(), ["checkout feature"]);
}

#[test]
fn checkout_refuses_to_touch_modified_files_and_lists_them() {
    let reader = FakeReader {
        dirty: vec!["notes.txt".into(), "src/a.rs".into()],
        changed: vec!["README.md".into(), "src/a.rs".into()],
        ..reader()
    };
    let (result, writer) = checkout(reader, CheckoutTarget::Local("feature".into()));
    assert_eq!(
        result,
        Err(OperationError::DirtyWorkTree(DirtyWorkTree {
            files: vec!["src/a.rs".into()]
        }))
    );
    assert!(writer.calls().is_empty());
}

#[test]
fn checkout_refuses_during_a_merge() {
    let reader = FakeReader {
        ongoing: Some(OngoingOperation::Merge),
        ..reader()
    };
    let (result, _) = checkout(reader, CheckoutTarget::Local("feature".into()));
    assert_eq!(
        result,
        Err(OperationError::OperationInProgress(OngoingOperation::Merge))
    );
}

#[test]
fn checkout_of_a_remote_branch_creates_a_tracking_branch() {
    let target = CheckoutTarget::Remote {
        remote: "origin".into(),
        branch: "topic".into(),
    };
    let (result, writer) = checkout(reader(), target);
    assert_eq!(result, Ok("topic".into()));
    assert_eq!(
        writer.calls(),
        [
            format!("create_branch topic {}", id(4).short()),
            "set_tracking topic origin refs/heads/topic".to_owned(),
            "checkout topic".to_owned(),
        ]
    );
}

#[test]
fn checkout_of_a_remote_branch_switches_to_its_existing_tracking_branch() {
    let mut reader = reader();
    reader.tracking.insert(
        "feature".into(),
        Tracking {
            remote: "origin".into(),
            merge_ref: "refs/heads/feature".into(),
            tracking_ref: "refs/remotes/origin/feature".into(),
        },
    );
    let target = CheckoutTarget::Remote {
        remote: "origin".into(),
        branch: "feature".into(),
    };
    let (result, writer) = checkout(reader, target);
    assert_eq!(result, Ok("feature".into()));
    assert_eq!(writer.calls(), ["checkout feature"]);
}

#[test]
fn checkout_of_a_remote_branch_will_not_take_over_an_unrelated_local_branch() {
    let mut reader = reader();
    reader.tracking.insert(
        "feature".into(),
        Tracking {
            remote: "fork".into(),
            merge_ref: "refs/heads/feature".into(),
            tracking_ref: "refs/remotes/fork/feature".into(),
        },
    );
    let target = CheckoutTarget::Remote {
        remote: "origin".into(),
        branch: "feature".into(),
    };
    let (result, writer) = checkout(reader, target);
    assert_eq!(result, Err(OperationError::BranchExists("feature".into())));
    assert!(writer.calls().is_empty());
}

// Create

#[test]
fn create_branch_at_a_commit_and_optionally_switch() {
    let writer = Arc::new(FakeWriter::default());
    let create = CreateBranch::new(Arc::new(reader()), writer.clone());
    create.create(&repo(), "fix/x", id(7), false).unwrap();
    create.create(&repo(), "fix/y", id(7), true).unwrap();
    assert_eq!(
        writer.calls(),
        [
            format!("create_branch fix/x {}", id(7).short()),
            format!("create_branch fix/y {}", id(7).short()),
            "checkout fix/y".to_owned(),
        ]
    );
}

#[test]
fn create_branch_validates_the_name() {
    let create = CreateBranch::new(Arc::new(reader()), Arc::new(FakeWriter::default()));
    assert_eq!(
        create.check_name(&repo(), "bad name"),
        Err(OperationError::InvalidBranchName(
            InvalidBranchName::Forbidden(' ')
        ))
    );
    assert_eq!(
        create.check_name(&repo(), "feature"),
        Err(OperationError::BranchExists("feature".into()))
    );
    assert_eq!(create.check_name(&repo(), "feature-2"), Ok(()));
}

// Delete

fn delete(reader: FakeReader) -> (DeleteBranch, Arc<FakeWriter>, Arc<FakeRemote>) {
    let writer = Arc::new(FakeWriter::default());
    let remote = Arc::new(FakeRemote::default());
    (
        DeleteBranch::new(Arc::new(reader), writer.clone(), remote.clone()),
        writer,
        remote,
    )
}

#[test]
fn deleting_a_merged_branch_needs_no_confirmation() {
    let reader = FakeReader {
        ahead_behind: (0, 5),
        ..reader()
    };
    let (delete, writer, _) = delete(reader);
    delete.delete_local(&repo(), "feature", false).unwrap();
    assert_eq!(writer.calls(), ["delete_branch feature"]);
}

#[test]
fn deleting_an_unmerged_branch_asks_first() {
    let reader = FakeReader {
        ahead_behind: (2, 0),
        ..reader()
    };
    let (delete, writer, _) = delete(reader);
    assert_eq!(
        delete.delete_local(&repo(), "feature", false),
        Err(OperationError::Unmerged {
            branch: "feature".into(),
            commits: 2
        })
    );
    assert!(writer.calls().is_empty());
    delete.delete_local(&repo(), "feature", true).unwrap();
    assert_eq!(writer.calls(), ["delete_branch feature"]);
}

#[test]
fn the_current_branch_cannot_be_deleted() {
    let (delete, writer, _) = delete(reader());
    assert_eq!(
        delete.delete_local(&repo(), "main", true),
        Err(OperationError::CannotDeleteCurrent("main".into()))
    );
    assert!(writer.calls().is_empty());
}

#[test]
fn deleting_a_remote_branch_deletes_it_on_the_server() {
    let (delete, _, remote) = delete(reader());
    delete
        .delete_remote(&repo(), "origin", "feature", &Silent, &Silent)
        .unwrap();
    assert_eq!(remote.calls(), ["delete origin refs/heads/feature"]);
}

// Merge and rebase

#[test]
fn merge_messages_follow_git() {
    assert_eq!(
        merge_message("refs/heads/feature"),
        "Merge branch 'feature'"
    );
    assert_eq!(
        merge_message("refs/remotes/origin/main"),
        "Merge remote-tracking branch 'origin/main'"
    );
}

fn merge(
    reader: FakeReader,
    writer: FakeWriter,
) -> (Result<IntegrationOutcome, OperationError>, Arc<FakeWriter>) {
    let writer = Arc::new(writer);
    let result = MergeIntoCurrent::new(Arc::new(reader), writer.clone())
        .merge(&repo(), "refs/heads/feature");
    (result, writer)
}

#[test]
fn merge_fast_forwards_when_possible() {
    let reader = FakeReader {
        ahead_behind: (0, 3),
        ..reader()
    };
    let (result, writer) = merge(reader, FakeWriter::default());
    assert_eq!(result, Ok(IntegrationOutcome::FastForwarded { to: id(2) }));
    assert_eq!(
        writer.calls(),
        [format!("fast_forward main {}", id(2).short())]
    );
}

#[test]
fn merge_of_diverged_branches_creates_a_merge_commit() {
    let reader = FakeReader {
        ahead_behind: (1, 3),
        ..reader()
    };
    let (result, writer) = merge(reader, FakeWriter::default());
    assert_eq!(result, Ok(IntegrationOutcome::Merged));
    assert_eq!(
        writer.calls(),
        [format!("merge {} Merge branch 'feature'", id(2).short())]
    );
}

#[test]
fn merge_of_an_already_merged_branch_is_up_to_date() {
    let reader = FakeReader {
        ahead_behind: (4, 0),
        ..reader()
    };
    let (result, writer) = merge(reader, FakeWriter::default());
    assert_eq!(result, Ok(IntegrationOutcome::UpToDate));
    assert!(writer.calls().is_empty());
}

#[test]
fn merge_conflicts_are_reported() {
    let reader = FakeReader {
        ahead_behind: (1, 3),
        ..reader()
    };
    assert_eq!(
        merge(reader, conflicting()).0,
        Ok(IntegrationOutcome::Conflicts {
            operation: OngoingOperation::Merge,
            files: vec!["a.txt".into()]
        })
    );
}

#[test]
fn merge_and_rebase_need_a_clean_work_tree_and_a_branch() {
    let dirty = FakeReader {
        dirty: vec!["a.txt".into()],
        ahead_behind: (1, 3),
        ..reader()
    };
    assert!(matches!(
        merge(dirty, FakeWriter::default()).0,
        Err(OperationError::DirtyWorkTree(_))
    ));
    let detached = FakeReader {
        head: Head::Detached(id(1)),
        ..reader()
    };
    let rebase = RebaseCurrentOnto::new(Arc::new(detached), Arc::new(FakeWriter::default()));
    assert_eq!(
        rebase.rebase(&repo(), "refs/heads/feature"),
        Err(OperationError::NoCurrentBranch)
    );
}

#[test]
fn rebase_replays_the_current_branch() {
    let reader = FakeReader {
        ahead_behind: (2, 3),
        ..reader()
    };
    let writer = Arc::new(FakeWriter::default());
    let result = RebaseCurrentOnto::new(Arc::new(reader), writer.clone())
        .rebase(&repo(), "refs/remotes/origin/feature");
    assert_eq!(result, Ok(IntegrationOutcome::Rebased));
    assert_eq!(writer.calls(), [format!("rebase {}", id(3).short())]);
}

#[test]
fn rebase_onto_a_descendant_is_a_fast_forward() {
    let reader = FakeReader {
        ahead_behind: (0, 3),
        ..reader()
    };
    let writer = Arc::new(FakeWriter::default());
    let result = RebaseCurrentOnto::new(Arc::new(reader), writer.clone())
        .rebase(&repo(), "refs/heads/feature");
    assert_eq!(result, Ok(IntegrationOutcome::FastForwarded { to: id(2) }));
}

// Ongoing operations

#[test]
fn load_ongoing_reports_the_operation_and_its_conflicts() {
    let reader = FakeReader {
        ongoing: Some(OngoingOperation::Rebase),
        conflicts: vec!["a.txt".into()],
        ..reader()
    };
    assert_eq!(
        LoadOngoing::new(Arc::new(reader)).load(&repo()),
        Ok(Some(OngoingState {
            operation: OngoingOperation::Rebase,
            conflicts: vec!["a.txt".into()]
        }))
    );
    assert_eq!(
        LoadOngoing::new(Arc::new(self::reader())).load(&repo()),
        Ok(None)
    );
}

fn resume(
    reader: FakeReader,
    writer: FakeWriter,
) -> (Result<IntegrationOutcome, OperationError>, Arc<FakeWriter>) {
    let writer = Arc::new(writer);
    let result = ContinueOperation::new(Arc::new(reader), writer.clone()).resume(&repo());
    (result, writer)
}

#[test]
fn continue_finishes_a_resolved_merge_or_rebase() {
    let merging = FakeReader {
        ongoing: Some(OngoingOperation::Merge),
        ..reader()
    };
    let (result, writer) = resume(merging, FakeWriter::default());
    assert_eq!(result, Ok(IntegrationOutcome::Merged));
    assert_eq!(writer.calls(), ["continue"]);

    let rebasing = FakeReader {
        ongoing: Some(OngoingOperation::Rebase),
        ..reader()
    };
    assert_eq!(
        resume(rebasing, FakeWriter::default()).0,
        Ok(IntegrationOutcome::Rebased)
    );
}

#[test]
fn continue_needs_resolved_conflicts() {
    let reader = FakeReader {
        ongoing: Some(OngoingOperation::Merge),
        conflicts: vec!["a.txt".into()],
        ..reader()
    };
    let (result, writer) = resume(reader, FakeWriter::default());
    assert_eq!(
        result,
        Err(OperationError::UnresolvedConflicts(vec!["a.txt".into()]))
    );
    assert!(writer.calls().is_empty());
}

#[test]
fn a_continued_rebase_can_stop_again() {
    let reader = FakeReader {
        ongoing: Some(OngoingOperation::Rebase),
        ..reader()
    };
    assert_eq!(
        resume(reader, conflicting()).0,
        Ok(IntegrationOutcome::Conflicts {
            operation: OngoingOperation::Rebase,
            files: vec!["a.txt".into()]
        })
    );
}

#[test]
fn continue_without_an_operation_or_for_a_cherry_pick_fails() {
    assert_eq!(
        resume(reader(), FakeWriter::default()).0,
        Err(OperationError::NothingInProgress)
    );
    let picking = FakeReader {
        ongoing: Some(OngoingOperation::CherryPick),
        ..reader()
    };
    assert_eq!(
        resume(picking, FakeWriter::default()).0,
        Err(OperationError::CannotContinue(OngoingOperation::CherryPick))
    );
}
