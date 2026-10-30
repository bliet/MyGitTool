//! The order in which credentials are tried: the SSH agent, the default SSH
//! keys, Git's credential helper, and finally the user. Nothing is stored.

use application::CredentialProvider;
use domain::{Credential, CredentialRequest};
use std::collections::VecDeque;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Private keys in the SSH directory, in the order OpenSSH tries them.
const DEFAULT_KEYS: [&str; 3] = ["id_ed25519", "id_ecdsa", "id_rsa"];
/// How often the user is asked for a user name and password per operation.
const MAX_PROMPTS: usize = 3;
/// The user name for SSH URLs without one, as used by the common hosts.
const DEFAULT_SSH_USER: &str = "git";

/// The kinds of credentials a server accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AllowedCredentials {
    pub ssh_key: bool,
    pub user_pass: bool,
    /// Only a user name, which SSH asks for before the key.
    pub username: bool,
}

/// What to authenticate with next.
#[derive(Clone, PartialEq, Eq)]
pub enum Attempt {
    SshAgent {
        username: String,
    },
    SshKey {
        username: String,
        key: PathBuf,
        passphrase: Option<String>,
    },
    /// Ask Git's configured credential helper; if it has nothing, ask the
    /// chain again.
    CredentialHelper,
    UserPass {
        username: String,
        password: String,
    },
    Username(String),
    /// The user cancelled a prompt.
    Cancelled,
    /// Every option has been tried.
    Exhausted,
}

impl fmt::Debug for Attempt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted = |secret: &Option<String>| secret.as_ref().map(|_| "<redacted>");
        match self {
            Self::SshAgent { username } => write!(f, "SshAgent({username})"),
            Self::SshKey {
                username,
                key,
                passphrase,
            } => f
                .debug_struct("SshKey")
                .field("username", username)
                .field("key", key)
                .field("passphrase", &redacted(passphrase))
                .finish(),
            Self::CredentialHelper => f.write_str("CredentialHelper"),
            Self::UserPass { username, .. } => write!(f, "UserPass({username}, <redacted>)"),
            Self::Username(name) => write!(f, "Username({name})"),
            Self::Cancelled => f.write_str("Cancelled"),
            Self::Exhausted => f.write_str("Exhausted"),
        }
    }
}

/// The state of the credential chain during one remote operation.
#[derive(Debug)]
pub struct CredentialChain {
    ssh_dir: Option<PathBuf>,
    /// The default keys not tried yet, found on the first SSH request.
    keys: Option<VecDeque<PathBuf>>,
    agent_tried: bool,
    helper_tried: bool,
    prompts: usize,
}

impl CredentialChain {
    /// A chain whose default SSH keys live in `ssh_dir`.
    pub fn new(ssh_dir: Option<PathBuf>) -> Self {
        Self {
            ssh_dir,
            keys: None,
            agent_tried: false,
            helper_tried: false,
            prompts: 0,
        }
    }

    /// A chain for the keys in the user's `~/.ssh`.
    pub fn for_current_user() -> Self {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
        Self::new(home.map(|home| PathBuf::from(home).join(".ssh")))
    }

    /// The next thing to try for `url`. `username` is the one in the URL.
    pub fn next(
        &mut self,
        url: &str,
        username: Option<&str>,
        allowed: AllowedCredentials,
        provider: &dyn CredentialProvider,
    ) -> Attempt {
        if allowed.ssh_key {
            return self.next_ssh(url, username.unwrap_or(DEFAULT_SSH_USER), provider);
        }
        if allowed.username {
            return Attempt::Username(username.unwrap_or(DEFAULT_SSH_USER).to_owned());
        }
        if allowed.user_pass {
            return self.next_user_pass(url, username, provider);
        }
        Attempt::Exhausted
    }

    fn next_ssh(
        &mut self,
        url: &str,
        username: &str,
        provider: &dyn CredentialProvider,
    ) -> Attempt {
        if !self.agent_tried {
            self.agent_tried = true;
            return Attempt::SshAgent {
                username: username.to_owned(),
            };
        }
        let ssh_dir = self.ssh_dir.clone();
        let keys = self
            .keys
            .get_or_insert_with(|| default_keys(ssh_dir.as_deref()));
        let Some(key) = keys.pop_front() else {
            return Attempt::Exhausted;
        };
        let passphrase = if is_encrypted(&key) {
            let request = CredentialRequest::SshPassphrase {
                url: url.to_owned(),
                key: key.clone(),
            };
            match provider.credentials(&request) {
                Some(Credential::Passphrase(passphrase)) => Some(passphrase),
                _ => return Attempt::Cancelled,
            }
        } else {
            None
        };
        Attempt::SshKey {
            username: username.to_owned(),
            key,
            passphrase,
        }
    }

    fn next_user_pass(
        &mut self,
        url: &str,
        username: Option<&str>,
        provider: &dyn CredentialProvider,
    ) -> Attempt {
        if !self.helper_tried {
            self.helper_tried = true;
            return Attempt::CredentialHelper;
        }
        if self.prompts >= MAX_PROMPTS {
            return Attempt::Exhausted;
        }
        self.prompts += 1;
        let request = CredentialRequest::UserPass {
            url: url.to_owned(),
            username: username.map(str::to_owned),
        };
        match provider.credentials(&request) {
            Some(Credential::UserPass { username, password }) => {
                Attempt::UserPass { username, password }
            }
            _ => Attempt::Cancelled,
        }
    }
}

fn default_keys(ssh_dir: Option<&Path>) -> VecDeque<PathBuf> {
    let Some(dir) = ssh_dir else {
        return VecDeque::new();
    };
    DEFAULT_KEYS
        .iter()
        .map(|name| dir.join(name))
        .filter(|path| path.is_file())
        .collect()
}

/// Whether a private key needs a passphrase. Covers the OpenSSH format,
/// whose header names the cipher, and the legacy PEM formats.
pub fn is_encrypted(key: &Path) -> bool {
    let Ok(text) = fs::read_to_string(key) else {
        return false;
    };
    if text.contains("ENCRYPTED") {
        return true;
    }
    if !text.contains("BEGIN OPENSSH PRIVATE KEY") {
        return false;
    }
    let body: String = text
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(128)
        .collect();
    let header = decode_base64(&body);
    // "openssh-key-v1\0", then the cipher name as a length-prefixed string.
    let magic = b"openssh-key-v1\0";
    let Some(rest) = header.strip_prefix(magic.as_slice()) else {
        return false;
    };
    let Some((len, rest)) = rest.split_first_chunk::<4>() else {
        return false;
    };
    let len = u32::from_be_bytes(*len) as usize;
    rest.get(..len).is_some_and(|cipher| cipher != b"none")
}

/// Decodes standard base64, stopping at the first character that is not
/// part of the alphabet.
fn decode_base64(text: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => break,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            // Masked to one byte, so the cast cannot truncate.
            bytes.push((buffer >> bits & 0xff) as u8);
        }
    }
    bytes
}
