//! Web pages of repositories on well-known hosting services.

use std::fmt;

/// A hosting service whose web pages can be derived from a remote URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hoster {
    GitHub,
    GitLab,
    Bitbucket,
    Codeberg,
}

impl Hoster {
    fn from_host(host: &str) -> Option<Self> {
        match host.to_ascii_lowercase().as_str() {
            "github.com" | "www.github.com" | "ssh.github.com" => Some(Self::GitHub),
            "gitlab.com" | "www.gitlab.com" | "altssh.gitlab.com" => Some(Self::GitLab),
            "bitbucket.org" | "www.bitbucket.org" => Some(Self::Bitbucket),
            "codeberg.org" => Some(Self::Codeberg),
            _ => None,
        }
    }

    fn web_host(self) -> &'static str {
        match self {
            Self::GitHub => "github.com",
            Self::GitLab => "gitlab.com",
            Self::Bitbucket => "bitbucket.org",
            Self::Codeberg => "codeberg.org",
        }
    }

    /// The path below the repository page that shows `branch`.
    fn branch_path(self, branch: &str) -> String {
        let branch = encode_path(branch);
        match self {
            Self::GitHub => format!("/tree/{branch}"),
            Self::GitLab => format!("/-/tree/{branch}"),
            Self::Bitbucket => format!("/src/{branch}"),
            Self::Codeberg => format!("/src/branch/{branch}"),
        }
    }
}

impl fmt::Display for Hoster {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::GitHub => "GitHub",
            Self::GitLab => "GitLab",
            Self::Bitbucket => "Bitbucket",
            Self::Codeberg => "Codeberg",
        })
    }
}

/// A page to open in the browser.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WebLink {
    pub hoster: Hoster,
    pub url: String,
}

/// The web page of the repository a remote URL points to, or of `branch` in
/// it. `None` unless the remote is on a known hoster. Understands
/// `https://host/owner/repo.git`, `ssh://git@host/owner/repo.git` and
/// `git@host:owner/repo.git`.
pub fn web_link(remote_url: &str, branch: Option<&str>) -> Option<WebLink> {
    let (host, path) = split_remote_url(remote_url.trim())?;
    let hoster = Hoster::from_host(host)?;
    let path = path.trim_matches('/');
    let path = path
        .strip_suffix(".git")
        .unwrap_or(path)
        .trim_end_matches('/');
    // At least an owner and a repository; GitLab groups can nest deeper.
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() < 2 || segments.iter().any(|s| s.is_empty() || *s == "..") {
        return None;
    }
    let mut url = format!("https://{}/{path}", hoster.web_host());
    if let Some(branch) = branch.filter(|b| !b.is_empty()) {
        url.push_str(&hoster.branch_path(branch));
    }
    Some(WebLink { hoster, url })
}

/// The host and the path of a remote URL in URL or scp-like form.
fn split_remote_url(url: &str) -> Option<(&str, &str)> {
    if let Some((scheme, rest)) = url.split_once("://") {
        if !matches!(
            scheme,
            "https" | "http" | "ssh" | "git" | "git+ssh" | "ssh+git"
        ) {
            return None;
        }
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        let host = host.split_once(':').map_or(host, |(h, _)| h);
        return Some((host, path));
    }
    // scp-like: [user@]host:path, where the host contains no slash.
    let (authority, path) = url.split_once(':')?;
    if authority.contains('/') {
        return None;
    }
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    Some((host, path))
}

/// Percent-encodes a branch name for a URL path, keeping `/` between its
/// segments.
fn encode_path(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
