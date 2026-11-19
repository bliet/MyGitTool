//! Which tags each remote has. Git keeps no remote-tracking refs for tags, so
//! the tags a remote advertises when fetching from or pushing to it are
//! recorded in a file in the Git directory:
//!
//! ```text
//! [origin]
//! 1f0c…e3 v1.0
//! 9a7b…41 v1.1
//! ```
//!
//! The object ids are the tag refs' own targets, not peeled: an annotated
//! tag is recorded with the id of its tag object.

use git2::{Oid, Remote, Repository};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

/// The tags of each recorded remote, by remote and tag name.
pub(crate) type RemoteTags = BTreeMap<String, BTreeMap<String, Oid>>;

const TAG_PREFIX: &str = "refs/tags/";

fn file(git: &Repository) -> PathBuf {
    git.commondir().join("mygittool").join("remote-tags")
}

/// The recorded tags. A missing or unreadable file records nothing.
pub(crate) fn load(git: &Repository) -> RemoteTags {
    let Ok(text) = fs::read_to_string(file(git)) else {
        return RemoteTags::new();
    };
    let mut all = RemoteTags::new();
    let mut current: Option<&mut BTreeMap<String, Oid>> = None;
    for line in text.lines() {
        if let Some(remote) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = Some(all.entry(remote.to_owned()).or_default());
        } else if let Some((oid, name)) = line.split_once(' ')
            && let Ok(oid) = Oid::from_str(oid)
            && let Some(tags) = current.as_deref_mut()
        {
            tags.insert(name.to_owned(), oid);
        }
    }
    all
}

/// The tags `handle` advertised at its last connection, or `None` if it
/// never connected.
pub(crate) fn advertised(handle: &Remote<'_>) -> Option<BTreeMap<String, Oid>> {
    let heads = handle.list().ok()?;
    Some(
        heads
            .iter()
            .filter_map(|head| {
                let name = head.name().strip_prefix(TAG_PREFIX)?;
                // `v1^{}` is the commit an annotated `v1` points to.
                (!name.ends_with("^{}")).then(|| (name.to_owned(), head.oid()))
            })
            .collect(),
    )
}

/// Records `tags` as all the tags of `remote`.
///
/// The record only informs the display, so callers may ignore a failure:
/// the operation that saw the tags succeeded regardless.
pub(crate) fn store(git: &Repository, remote: &str, tags: BTreeMap<String, Oid>) -> io::Result<()> {
    let mut all = load(git);
    all.insert(remote.to_owned(), tags);
    let mut text = String::new();
    for (remote, tags) in &all {
        text.push_str(&format!("[{remote}]\n"));
        for (name, oid) in tags {
            text.push_str(&format!("{oid} {name}\n"));
        }
    }
    let path = file(git);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // Written aside and renamed, so a reader never sees half a file.
    let partial = path.with_extension("lock");
    fs::write(&partial, text)?;
    fs::rename(&partial, &path)
}

/// The tag name of `refname`, if it is a tag.
pub(crate) fn tag_name(refname: &str) -> Option<&str> {
    refname.strip_prefix(TAG_PREFIX)
}
