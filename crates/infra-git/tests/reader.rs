mod common;

use application::{HistoryScope, ReadError, RepositoryReader};
use common::TestRepo;
use domain::{BranchKind, ChangeKind, CommitId, Head, RepositoryInfo};
use infra_git::GitRepositoryReader;

fn info(repo: &TestRepo) -> RepositoryInfo {
    RepositoryInfo::new(repo.path())
}

fn id(oid: git2::Oid) -> CommitId {
    CommitId::from_slice(oid.as_bytes()).unwrap()
}

fn summaries(repo: &TestRepo, scope: HistoryScope) -> Vec<String> {
    let mut cursor = GitRepositoryReader.history(&info(repo), &scope).unwrap();
    cursor
        .next_page(1000)
        .unwrap()
        .into_iter()
        .map(|c| c.summary)
        .collect()
}

#[test]
fn head_on_a_branch() {
    let repo = TestRepo::new().commit("a.txt", "1");
    let head = GitRepositoryReader.head(&info(&repo)).unwrap();
    assert_eq!(
        head,
        Head::Branch {
            name: "main".into(),
            target: id(repo.head())
        }
    );
}

#[test]
fn head_detached_and_unborn() {
    let repo = TestRepo::new().commit("a.txt", "1").detach();
    assert_eq!(
        GitRepositoryReader.head(&info(&repo)).unwrap(),
        Head::Detached(id(repo.head()))
    );

    let empty = TestRepo::new();
    assert_eq!(
        GitRepositoryReader.head(&info(&empty)).unwrap(),
        Head::Unborn {
            name: "main".into()
        }
    );
}

#[test]
fn branches_with_upstream_and_ahead_behind() {
    let repo = TestRepo::new()
        .commit("a.txt", "1")
        .remote("origin", "https://example.com/r.git")
        .remote_branch("origin", "main")
        .commit("a.txt", "2")
        .commit("a.txt", "3")
        .set_upstream("main", "origin/main")
        .branch("feature/x");

    let branches = GitRepositoryReader.branches(&info(&repo)).unwrap();
    let main = branches.iter().find(|b| b.name == "main").unwrap();
    match &main.kind {
        BranchKind::Local { is_head, upstream } => {
            assert!(is_head);
            let upstream = upstream.as_ref().unwrap();
            assert_eq!(upstream.name, "origin/main");
            assert_eq!((upstream.ahead, upstream.behind), (Some(2), Some(0)));
        }
        other => panic!("expected a local branch, got {other:?}"),
    }

    let feature = branches.iter().find(|b| b.name == "feature/x").unwrap();
    assert_eq!(feature.refname, "refs/heads/feature/x");
    assert!(!feature.is_head());

    let remote = branches.iter().find(|b| b.name == "origin/main").unwrap();
    assert_eq!(
        remote.kind,
        BranchKind::Remote {
            remote: "origin".into()
        }
    );
    assert_eq!(remote.name_on_remote(), "main");
}

#[test]
fn a_gone_upstream_has_no_counts() {
    let repo = TestRepo::new()
        .commit("a.txt", "1")
        .remote("origin", "https://example.com/r.git")
        .remote_branch("origin", "main")
        .set_upstream("main", "origin/main");
    repo.git()
        .find_reference("refs/remotes/origin/main")
        .unwrap()
        .delete()
        .unwrap();

    let branches = GitRepositoryReader.branches(&info(&repo)).unwrap();
    let BranchKind::Local {
        upstream: Some(upstream),
        ..
    } = &branches[0].kind
    else {
        panic!("expected an upstream");
    };
    assert_eq!(upstream.name, "origin/main");
    assert_eq!((upstream.ahead, upstream.behind), (None, None));
}

#[test]
fn symbolic_remote_head_is_skipped() {
    let repo = TestRepo::new()
        .commit("a.txt", "1")
        .remote("origin", "https://example.com/r.git")
        .remote_branch("origin", "main");
    repo.git()
        .reference_symbolic(
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
            true,
            "test",
        )
        .unwrap();
    let names: Vec<_> = GitRepositoryReader
        .branches(&info(&repo))
        .unwrap()
        .into_iter()
        .map(|b| b.name)
        .collect();
    assert!(!names.contains(&"origin/HEAD".to_owned()), "{names:?}");
}

#[test]
fn remotes_with_urls() {
    let repo = TestRepo::new()
        .remote("origin", "https://example.com/r.git")
        .remote("upstream", "git@example.com:u/r.git");
    let remotes = GitRepositoryReader.remotes(&info(&repo)).unwrap();
    let pairs: Vec<_> = remotes
        .iter()
        .map(|r| (r.name.as_str(), r.url.as_deref()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("origin", Some("https://example.com/r.git")),
            ("upstream", Some("git@example.com:u/r.git")),
        ]
    );
}

#[test]
fn tags_are_peeled_to_commits() {
    let repo = TestRepo::new()
        .commit("a.txt", "1")
        .tag("v1")
        .annotated_tag("v1-annotated");
    let tags = GitRepositoryReader.tags(&info(&repo)).unwrap();
    assert_eq!(tags.len(), 2);
    assert!(tags.iter().all(|t| t.target == id(repo.head())));
}

#[test]
fn history_covers_all_refs_in_topological_order() {
    let repo = TestRepo::new()
        .commit_with_message("a.txt", "1", "base")
        .branch("feature")
        .checkout("feature")
        .commit_with_message("f.txt", "1", "feature work")
        .checkout("main")
        .commit_with_message("a.txt", "2", "main work")
        .branch("side")
        .merge("feature", "merge feature");
    let all = summaries(&repo, HistoryScope::All);
    assert_eq!(all.first().map(String::as_str), Some("merge feature"));
    assert_eq!(all.last().map(String::as_str), Some("base"));
    assert_eq!(all.len(), 4);

    let feature = summaries(&repo, HistoryScope::Ref("refs/heads/feature".into()));
    assert_eq!(feature, ["feature work", "base"]);
}

#[test]
fn history_includes_commits_only_reachable_from_other_refs() {
    let repo = TestRepo::new()
        .commit_with_message("a.txt", "1", "base")
        .branch("feature")
        .checkout("feature")
        .commit_with_message("f.txt", "1", "only on feature")
        .checkout("main");
    assert!(summaries(&repo, HistoryScope::All).contains(&"only on feature".to_owned()));
}

#[test]
fn history_pages_and_reports_parents() {
    let repo = TestRepo::new()
        .commit("a.txt", "1")
        .commit("a.txt", "2")
        .commit("a.txt", "3");
    let mut cursor = GitRepositoryReader
        .history(&info(&repo), &HistoryScope::All)
        .unwrap();
    let first = cursor.next_page(2).unwrap();
    let rest = cursor.next_page(2).unwrap();
    assert_eq!((first.len(), rest.len()), (2, 1));
    assert_eq!(first[0].id, id(repo.head()));
    assert_eq!(first[0].parents, [first[1].id]);
    assert!(rest[0].parents.is_empty());
    assert!(cursor.next_page(2).unwrap().is_empty());
}

#[test]
fn history_of_an_unborn_repository_is_empty() {
    assert!(summaries(&TestRepo::new(), HistoryScope::All).is_empty());
}

#[test]
fn history_of_an_unknown_ref_fails() {
    let repo = TestRepo::new().commit("a.txt", "1");
    let err = GitRepositoryReader
        .history(&info(&repo), &HistoryScope::Ref("refs/heads/nope".into()))
        .err()
        .unwrap();
    assert_eq!(err, ReadError::RefNotFound("refs/heads/nope".into()));
}

#[test]
fn commit_details_with_changes_and_line_counts() {
    let repo = TestRepo::new()
        .write("keep.txt", "a\nb\nc\n")
        .write("old.txt", "one\ntwo\nthree\nfour\nfive\n")
        .commit_with_message("gone.txt", "x\n", "base")
        .write("keep.txt", "a\nB\nc\nd\n")
        .write("new.txt", "1\n2\n")
        .write("image.bin", "\0\x01\x02")
        .delete("gone.txt")
        .rename("old.txt", "renamed.txt")
        .commit_all("Change things\n\nWith a body.\n");

    let details = GitRepositoryReader
        .commit_details(&info(&repo), id(repo.head()))
        .unwrap();
    assert_eq!(details.commit.summary, "Change things");
    assert_eq!(details.message, "Change things\n\nWith a body.\n");
    assert_eq!(details.committer.name, "Test User");
    assert_eq!(details.commit.parents.len(), 1);

    let find = |path: &str| details.changes.iter().find(|c| c.path == path).unwrap();
    let keep = find("keep.txt");
    assert_eq!(
        (keep.kind, keep.additions, keep.deletions),
        (ChangeKind::Modified, 2, 1)
    );
    let new = find("new.txt");
    assert_eq!(
        (new.kind, new.additions, new.deletions),
        (ChangeKind::Added, 2, 0)
    );
    assert_eq!(find("gone.txt").kind, ChangeKind::Deleted);
    let renamed = find("renamed.txt");
    assert_eq!(renamed.kind, ChangeKind::Renamed);
    assert_eq!(renamed.old_path.as_deref(), Some("old.txt"));
    assert!(find("image.bin").binary);
    assert_eq!(details.changes.len(), 5);
}

#[test]
fn root_commit_details_list_every_file_as_added() {
    let repo = TestRepo::new().commit("a.txt", "1\n");
    let details = GitRepositoryReader
        .commit_details(&info(&repo), id(repo.head()))
        .unwrap();
    assert!(details.commit.parents.is_empty());
    assert_eq!(details.changes.len(), 1);
    assert_eq!(details.changes[0].kind, ChangeKind::Added);
}

#[test]
fn details_of_an_unknown_commit_fail() {
    let repo = TestRepo::new().commit("a.txt", "1");
    let missing = CommitId::from_bytes([7; 20]);
    let err = GitRepositoryReader
        .commit_details(&info(&repo), missing)
        .unwrap_err();
    assert_eq!(err, ReadError::CommitNotFound(missing));
}

#[test]
fn work_tree_changes_cover_modified_and_untracked_files() {
    let repo = TestRepo::new()
        .commit("a.txt", "1\n")
        .write("a.txt", "1\n2\n")
        .write("dir/new.txt", "x\n");
    let changes = GitRepositoryReader.work_tree_changes(&info(&repo)).unwrap();
    assert!(changes.staged.is_empty());
    let mut changes = changes.unstaged;
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    let summary: Vec<_> = changes
        .iter()
        .map(|c| (c.path.as_str(), c.kind, c.additions))
        .collect();
    assert_eq!(
        summary,
        [
            ("a.txt", ChangeKind::Modified, 1),
            ("dir/new.txt", ChangeKind::Added, 1)
        ]
    );
}

#[test]
fn clean_and_bare_repositories_have_no_work_tree_changes() {
    let clean = TestRepo::new().commit("a.txt", "1");
    assert!(
        GitRepositoryReader
            .work_tree_changes(&info(&clean))
            .unwrap()
            .is_empty()
    );
    assert!(
        GitRepositoryReader
            .work_tree_changes(&info(&TestRepo::bare()))
            .unwrap()
            .is_empty()
    );
}
