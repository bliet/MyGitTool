//! Tag operations against real repositories.

mod common;

use application::{
    CreateTag, CredentialProvider, DeleteTag, OperationError, ProgressReporter, PushTag,
    RemoteError, RemoteOperations, RepositoryReader,
};
use common::TestRepo;
use domain::{
    CommitId, Credential, CredentialRequest, Progress, RemoteTag, RepositoryInfo, TagPresence,
};
use git2::ObjectType;
use infra_git::{GitRemoteOperations, GitRepositoryReader, GitRepositoryWriter};
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

fn create_tag() -> CreateTag {
    CreateTag::new(Arc::new(GitRepositoryReader), Arc::new(GitRepositoryWriter))
}

fn delete_tag() -> DeleteTag {
    DeleteTag::new(
        Arc::new(GitRepositoryReader),
        Arc::new(GitRepositoryWriter),
        Arc::new(GitRemoteOperations),
    )
}

fn push_tag() -> PushTag {
    PushTag::new(Arc::new(GitRepositoryReader), Arc::new(GitRemoteOperations))
}

/// A repository with one commit and a bare repository as `origin`.
fn with_remote() -> (TestRepo, TestRepo) {
    let remote = TestRepo::bare();
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .remote("origin", remote.path().to_str().unwrap());
    (repo, remote)
}

#[test]
fn lightweight_and_annotated_tags() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .commit("a.txt", "2\n");
    let first = repo
        .git()
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .parent_id(0)
        .unwrap();
    create_tag()
        .create(&info(&repo), "light", id(first), "")
        .unwrap();
    create_tag()
        .create(&info(&repo), "release/v1", id(repo.head()), "First release")
        .unwrap();

    let light = repo.git().find_reference("refs/tags/light").unwrap();
    assert_eq!(light.target(), Some(first));
    assert!(light.peel(ObjectType::Tag).is_err());

    let annotated = repo
        .git()
        .find_reference("refs/tags/release/v1")
        .unwrap()
        .peel(ObjectType::Tag)
        .unwrap()
        .into_tag()
        .unwrap();
    assert_eq!(annotated.target_id(), repo.head());
    assert_eq!(annotated.message().unwrap(), Some("First release\n"));
    assert_eq!(annotated.tagger().unwrap().name().unwrap(), "Test User");
}

#[test]
fn an_existing_tag_is_refused() {
    let repo = TestRepo::new().commit("a.txt", "1\n").tag("v1");
    let result = create_tag().create(&info(&repo), "v1", id(repo.head()), "");
    assert!(matches!(result, Err(OperationError::TagExists(_))));
}

#[test]
fn deleting_a_local_tag() {
    let repo = TestRepo::new().commit("a.txt", "1\n").annotated_tag("v1");
    delete_tag().delete_local(&info(&repo), "v1").unwrap();
    assert!(repo.git().find_reference("refs/tags/v1").is_err());
}

#[test]
fn pushing_and_deleting_a_tag_on_the_remote() {
    let (repo, remote) = with_remote();
    let repo = repo.tag("v1");
    push_tag()
        .push(&info(&repo), "v1", "origin", &Silent, &Silent)
        .unwrap();
    let pushed = remote.git().find_reference("refs/tags/v1").unwrap();
    assert_eq!(pushed.peel_to_commit().unwrap().id(), repo.head());

    delete_tag()
        .delete_remote(&info(&repo), "origin", "v1", &Silent, &Silent)
        .unwrap();
    assert!(remote.git().find_reference("refs/tags/v1").is_err());
    // The local tag stays.
    assert!(repo.git().find_reference("refs/tags/v1").is_ok());
}

/// Whether each remote has the local tag `name`, as the reader sees it.
fn remotes_of(repo: &TestRepo, name: &str) -> Vec<RemoteTag> {
    GitRepositoryReader
        .tags(&info(repo))
        .unwrap()
        .into_iter()
        .find(|tag| tag.name == name)
        .unwrap()
        .remotes
}

fn on_origin(presence: TagPresence) -> Vec<RemoteTag> {
    vec![RemoteTag {
        remote: "origin".into(),
        presence,
    }]
}

#[test]
fn whether_a_remote_has_a_tag_is_unknown_until_it_was_seen() {
    let (repo, _remote) = with_remote();
    let repo = repo.tag("v1");
    assert_eq!(remotes_of(&repo, "v1"), []);
}

#[test]
fn pushing_and_deleting_a_tag_records_whether_the_remote_has_it() {
    let (repo, _remote) = with_remote();
    let repo = repo.annotated_tag("v1");
    push_tag()
        .push(&info(&repo), "v1", "origin", &Silent, &Silent)
        .unwrap();
    assert_eq!(remotes_of(&repo, "v1"), on_origin(TagPresence::Same));

    delete_tag()
        .delete_remote(&info(&repo), "origin", "v1", &Silent, &Silent)
        .unwrap();
    assert_eq!(remotes_of(&repo, "v1"), on_origin(TagPresence::Missing));
}

#[test]
fn fetching_records_which_tags_the_remote_has() {
    let (repo, remote) = with_remote();
    let repo = repo.tag("pushed").tag("different");
    push_tag()
        .push(&info(&repo), "pushed", "origin", &Silent, &Silent)
        .unwrap();
    // Someone else tags the same commit `different`, annotated.
    {
        let commit = remote.git().find_object(repo.head(), None).unwrap();
        let signature = git2::Signature::now("Other", "other@example.com").unwrap();
        remote
            .git()
            .tag("different", &commit, &signature, "theirs", false)
            .unwrap();
    }
    let repo = repo.annotated_tag("local");

    GitRemoteOperations
        .fetch(&info(&repo), "origin", &Silent, &Silent)
        .unwrap();

    assert_eq!(remotes_of(&repo, "pushed"), on_origin(TagPresence::Same));
    assert_eq!(remotes_of(&repo, "local"), on_origin(TagPresence::Missing));
    assert_eq!(
        remotes_of(&repo, "different"),
        on_origin(TagPresence::Different)
    );
}

#[test]
fn a_removed_remote_no_longer_counts() {
    let (repo, _remote) = with_remote();
    let repo = repo.tag("v1");
    GitRemoteOperations
        .fetch(&info(&repo), "origin", &Silent, &Silent)
        .unwrap();
    assert_eq!(remotes_of(&repo, "v1"), on_origin(TagPresence::Missing));

    repo.git().remote_delete("origin").unwrap();
    assert_eq!(remotes_of(&repo, "v1"), []);
}

#[test]
fn a_different_tag_of_the_same_name_on_the_remote_is_not_replaced() {
    let (repo, remote) = with_remote();
    let repo = repo.tag("v1");
    push_tag()
        .push(&info(&repo), "v1", "origin", &Silent, &Silent)
        .unwrap();
    let old = repo.head();

    // Move the local tag to a new commit and push again.
    let repo = repo.commit("a.txt", "2\n");
    repo.git()
        .tag_lightweight(
            "v1",
            &repo.git().find_object(repo.head(), None).unwrap(),
            true,
        )
        .unwrap();
    let result = push_tag().push(&info(&repo), "v1", "origin", &Silent, &Silent);
    assert!(
        matches!(
            &result,
            Err(OperationError::Remote(RemoteError::Rejected { .. }))
        ),
        "{result:?}"
    );
    let on_remote = remote.git().find_reference("refs/tags/v1").unwrap();
    assert_eq!(on_remote.target(), Some(old));
    assert_eq!(remotes_of(&repo, "v1"), on_origin(TagPresence::Different));
}
