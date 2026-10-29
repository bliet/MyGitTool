use crate::change::FileChange;
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// The SHA-1 object id of a commit.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommitId([u8; 20]);

/// The number of hex digits shown for an abbreviated id.
const SHORT_LEN: usize = 7;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid commit id: {0:?}")]
pub struct InvalidCommitId(pub String);

impl CommitId {
    pub const fn from_bytes(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// Fails unless `bytes` is exactly 20 bytes long.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, InvalidCommitId> {
        <[u8; 20]>::try_from(bytes)
            .map(Self)
            .map_err(|_| InvalidCommitId(format!("{} bytes", bytes.len())))
    }

    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        self.to_string()
    }

    /// The first seven hex digits.
    pub fn short(&self) -> String {
        let mut hex = self.to_hex();
        hex.truncate(SHORT_LEN);
        hex
    }
}

impl fmt::Display for CommitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for CommitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CommitId({})", self.short())
    }
}

impl FromStr for CommitId {
    type Err = InvalidCommitId;

    /// Parses a full 40-digit hex id.
    fn from_str(hex: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidCommitId(hex.to_owned());
        if hex.len() != 40 || !hex.is_ascii() {
            return Err(invalid());
        }
        let mut bytes = [0u8; 20];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|_| invalid())?;
        }
        Ok(Self(bytes))
    }
}

/// A point in time with the UTC offset it was recorded in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Timestamp {
    /// Seconds since the Unix epoch.
    pub seconds: i64,
    /// Offset from UTC in minutes, e.g. `60` for UTC+01:00.
    pub offset_minutes: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Signature {
    pub name: String,
    pub email: String,
    pub time: Timestamp,
}

/// The data of a commit needed for a history row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub id: CommitId,
    /// Parent ids, first parent first.
    pub parents: Vec<CommitId>,
    pub author: Signature,
    /// The first line of the message.
    pub summary: String,
}

/// Everything the details column shows about a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitDetails {
    pub commit: Commit,
    pub committer: Signature,
    pub message: String,
    /// Changes against the first parent (or the empty tree for a root commit).
    pub changes: Vec<FileChange>,
}
