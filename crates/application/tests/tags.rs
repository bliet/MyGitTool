mod common;

use application::{CreateTag, DeleteTag, OperationError, PushTag, RemoteError};
use common::{FakeReader, FakeRemote, FakeWriter, Silent, id};
use domain::{InvalidBranchName, RepositoryInfo};
use std::sync::Arc;

fn repo() -> RepositoryInfo {
    RepositoryInfo::new("/repo")
}

/// A reader that knows the tag `v1` at commit 1.
fn with_v1() -> Arc<FakeReader> {
    let mut reader = FakeReader::default();
    reader.refs.insert("refs/tags/v1".into(), id(1));
    Arc::new(reader)
}

#[test]
fn a_blank_message_makes_a_lightweight_tag() {
    let writer = Arc::new(FakeWriter::default());
    CreateTag::new(with_v1(), writer.clone())
        .create(&repo(), "v2", id(2), "  \n ")
        .unwrap();
    assert_eq!(writer.calls(), [format!("create_tag v2 {}", id(2).short())]);
}

#[test]
fn a_message_makes_an_annotated_tag() {
    let writer = Arc::new(FakeWriter::default());
    CreateTag::new(with_v1(), writer.clone())
        .create(&repo(), "v2", id(2), "  Release 2  ")
        .unwrap();
    assert_eq!(
        writer.calls(),
        [format!("create_tag v2 {} \"Release 2\\n\"", id(2).short())]
    );
}

#[test]
fn existing_and_invalid_names_are_refused() {
    let writer = Arc::new(FakeWriter::default());
    let create = CreateTag::new(with_v1(), writer.clone());
    assert!(matches!(
        create.create(&repo(), "v1", id(2), ""),
        Err(OperationError::TagExists(name)) if name == "v1"
    ));
    assert!(matches!(
        create.check_name(&repo(), "bad name"),
        Err(OperationError::InvalidTagName(
            InvalidBranchName::Forbidden(' ')
        ))
    ));
    assert!(writer.calls().is_empty());
}

#[test]
fn deleting_a_tag_here_and_on_a_remote() {
    let writer = Arc::new(FakeWriter::default());
    let remote = Arc::new(FakeRemote::default());
    let delete = DeleteTag::new(with_v1(), writer.clone(), remote.clone());
    delete.delete_local(&repo(), "v1").unwrap();
    assert_eq!(writer.calls(), ["delete_tag v1"]);
    assert!(matches!(
        delete.delete_local(&repo(), "v9"),
        Err(OperationError::TagNotFound(name)) if name == "v9"
    ));
    delete
        .delete_remote(&repo(), "origin", "v1", &Silent, &Silent)
        .unwrap();
    assert_eq!(remote.calls(), ["delete origin refs/tags/v1"]);
}

#[test]
fn pushing_a_tag() {
    let remote = Arc::new(FakeRemote::default());
    let push = PushTag::new(with_v1(), remote.clone());
    push.push(&repo(), "v1", "origin", &Silent, &Silent)
        .unwrap();
    assert_eq!(remote.calls(), ["push_tag v1 origin"]);
    assert!(matches!(
        push.push(&repo(), "v9", "origin", &Silent, &Silent),
        Err(OperationError::TagNotFound(_))
    ));
}

#[test]
fn a_rejected_push_is_reported() {
    let remote = Arc::new(FakeRemote {
        error: Some(RemoteError::Rejected {
            reference: "refs/tags/v1".into(),
            message: "exists".into(),
        }),
        ..FakeRemote::default()
    });
    let result = PushTag::new(with_v1(), remote).push(&repo(), "v1", "origin", &Silent, &Silent);
    assert!(matches!(
        result,
        Err(OperationError::Remote(RemoteError::Rejected { .. }))
    ));
}
