//! Conversions from git2 types to domain types.

use application::ReadError;
use domain::{ChangeKind, Commit, CommitId, Signature, Timestamp};
use git2::{Delta, Oid};

pub fn error(err: git2::Error) -> ReadError {
    ReadError::Backend(err.message().to_owned())
}

pub fn commit_id(oid: Oid) -> CommitId {
    // A SHA-1 oid is always 20 bytes; libgit2 is built without SHA-256 here.
    CommitId::from_slice(oid.as_bytes()).unwrap_or(CommitId::from_bytes([0; 20]))
}

pub fn oid(id: CommitId) -> Oid {
    // Twenty bytes are always a valid SHA-1 oid.
    Oid::from_bytes(id.as_bytes()).unwrap_or(Oid::ZERO_SHA1)
}

pub fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

pub fn signature(sig: &git2::Signature<'_>) -> Signature {
    let when = sig.when();
    Signature {
        name: text(sig.name_bytes()),
        email: text(sig.email_bytes()),
        time: Timestamp {
            seconds: when.seconds(),
            offset_minutes: when.offset_minutes(),
        },
    }
}

pub fn commit(commit: &git2::Commit<'_>) -> Commit {
    Commit {
        id: commit_id(commit.id()),
        parents: commit.parent_ids().map(commit_id).collect(),
        author: signature(&commit.author()),
        summary: commit.summary_bytes().map(text).unwrap_or_default(),
    }
}

/// Maps a delta status; `None` for entries that are not changes.
pub fn change_kind(delta: Delta) -> Option<ChangeKind> {
    match delta {
        Delta::Added | Delta::Untracked | Delta::Copied => Some(ChangeKind::Added),
        Delta::Deleted => Some(ChangeKind::Deleted),
        Delta::Modified | Delta::Conflicted => Some(ChangeKind::Modified),
        Delta::Renamed => Some(ChangeKind::Renamed),
        Delta::Typechange => Some(ChangeKind::TypeChanged),
        Delta::Unmodified | Delta::Ignored | Delta::Unreadable => None,
    }
}
