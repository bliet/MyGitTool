use domain::{Branch, BranchKind, ChangeKind, CommitId, Head};

const HEX: &str = "0123456789abcdef0123456789abcdef01234567";

#[test]
fn commit_id_round_trips_through_hex() {
    let id: CommitId = HEX.parse().unwrap();
    assert_eq!(id.to_hex(), HEX);
    assert_eq!(id.short(), "0123456");
    assert_eq!(CommitId::from_slice(id.as_bytes()).unwrap(), id);
}

#[test]
fn commit_id_rejects_bad_input() {
    assert!("abc".parse::<CommitId>().is_err());
    assert!(
        "z123456789abcdef0123456789abcdef01234567"
            .parse::<CommitId>()
            .is_err()
    );
    assert!(CommitId::from_slice(&[0; 19]).is_err());
}

#[test]
fn change_kind_letters() {
    let letters: String = [
        ChangeKind::Added,
        ChangeKind::Modified,
        ChangeKind::Deleted,
        ChangeKind::Renamed,
        ChangeKind::TypeChanged,
    ]
    .into_iter()
    .map(ChangeKind::letter)
    .collect();
    assert_eq!(letters, "AMDRT");
}

#[test]
fn remote_branch_name_on_remote_strips_the_remote() {
    let branch = Branch {
        name: "origin/feature/x".into(),
        refname: "refs/remotes/origin/feature/x".into(),
        target: CommitId::from_bytes([1; 20]),
        kind: BranchKind::Remote {
            remote: "origin".into(),
        },
    };
    assert_eq!(branch.name_on_remote(), "feature/x");
    assert!(!branch.is_local());
}

#[test]
fn head_target() {
    let id = CommitId::from_bytes([2; 20]);
    assert_eq!(Head::Detached(id).target(), Some(id));
    assert_eq!(
        Head::Unborn {
            name: "main".into()
        }
        .target(),
        None
    );
}
