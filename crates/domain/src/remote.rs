use std::fmt;
use std::path::PathBuf;

/// Where a local branch pulls from and pushes to, from its
/// `branch.<name>.remote` and `branch.<name>.merge` config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tracking {
    /// The remote's name, e.g. `origin`.
    pub remote: String,
    /// The branch on the remote, e.g. `refs/heads/main`.
    pub merge_ref: String,
    /// The local remote-tracking ref, e.g. `refs/remotes/origin/main`.
    pub tracking_ref: String,
}

/// Progress of a fetch or push.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    Receiving {
        objects: usize,
        total: usize,
        bytes: usize,
    },
    Resolving {
        deltas: usize,
        total: usize,
    },
    Sending {
        objects: usize,
        total: usize,
        bytes: usize,
    },
    /// A status line from the server, e.g. "Counting objects: 42".
    Message(String),
}

/// What a remote operation needs from the user to authenticate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialRequest {
    /// A user name and password (or token) for an HTTPS remote.
    UserPass {
        url: String,
        username: Option<String>,
    },
    /// The passphrase of an encrypted SSH key.
    SshPassphrase { url: String, key: PathBuf },
}

/// An answer to a [`CredentialRequest`]. It lives only as long as the
/// operation that asked for it and is never stored.
#[derive(Clone, PartialEq, Eq)]
pub enum Credential {
    UserPass { username: String, password: String },
    Passphrase(String),
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UserPass { username, .. } => f
                .debug_struct("UserPass")
                .field("username", username)
                .field("password", &"<redacted>")
                .finish(),
            Self::Passphrase(_) => f.write_str("Passphrase(<redacted>)"),
        }
    }
}

/// How `pull` integrates the fetched upstream, from `pull.rebase`,
/// `branch.<name>.rebase` and `pull.ff`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PullConfig {
    pub rebase: bool,
    pub fast_forward: FastForward,
}

/// The `pull.ff` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FastForward {
    /// Fast-forward when possible, otherwise merge (`true`, the default).
    #[default]
    Allow,
    /// Refuse anything but a fast-forward (`only`).
    Only,
    /// Always create a merge commit (`false`).
    Never,
}
