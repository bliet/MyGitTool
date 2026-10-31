//! The history walk: commits in topological order (children before
//! parents), newer commits first where the order allows it.
//!
//! With a commit-graph the walk is incremental. A commit is handed out once
//! every child that can reach it has been handed out. Commits in the graph
//! carry a generation number, and a child always has a higher one, so the
//! walk only explores the history down to the generation of the next commit
//! it hands out. The few commits newer than the graph have no generation
//! number and are explored completely before any of them is handed out.
//!
//! Without a commit-graph, every commit has to be seen before the first one
//! can be handed out, and libgit2's walk does that fastest.

use crate::commit_graph::CommitGraph;
use crate::map;
use application::{HistoryCursor, HistoryScope, ReadError};
use domain::Commit;
use git2::{ErrorCode, Oid, Repository, Sort};
use std::collections::hash_map::Entry;
use std::collections::{BinaryHeap, HashMap};

/// Starts a walk over the commits in `scope`.
pub(crate) fn cursor(
    git: Repository,
    scope: &HistoryScope,
) -> Result<Box<dyn HistoryCursor>, ReadError> {
    let tips = tips(&git, scope)?;
    // Git ignores the commit-graph when grafts or replacements change parents.
    let graph = if git.is_shallow() || has_replacements(&git) {
        None
    } else {
        CommitGraph::open(&git.commondir().join("objects"))
    };
    let order = match graph {
        Some(graph) => {
            let mut walk = TopoWalk::new(graph);
            for tip in tips {
                walk.add_tip(&git, tip)?;
            }
            Order::Incremental(Box::new(walk))
        }
        None => Order::Listed {
            ids: full_walk(&git, &tips)?,
            next: 0,
        },
    };
    Ok(Box::new(GitHistoryCursor { git, order }))
}

/// The commits the walk starts from. Refs that do not point to a commit
/// (e.g. a tag of a tree) are left out.
fn tips(git: &Repository, scope: &HistoryScope) -> Result<Vec<Oid>, ReadError> {
    match scope {
        HistoryScope::All => {
            let mut tips = Vec::new();
            if let Ok(head) = git.head()
                && let Ok(commit) = head.peel_to_commit()
            {
                tips.push(commit.id());
            }
            for glob in ["refs/heads/*", "refs/remotes/*", "refs/tags/*"] {
                for reference in git.references_glob(glob).map_err(map::error)? {
                    let reference = reference.map_err(map::error)?;
                    if let Ok(commit) = reference.peel_to_commit() {
                        tips.push(commit.id());
                    }
                }
            }
            Ok(tips)
        }
        HistoryScope::Ref(refname) => {
            let reference = git
                .find_reference(refname)
                .map_err(|err| match err.code() {
                    ErrorCode::NotFound => ReadError::RefNotFound(refname.clone()),
                    _ => map::error(err),
                })?;
            let commit = reference.peel_to_commit().map_err(map::error)?;
            Ok(vec![commit.id()])
        }
    }
}

fn has_replacements(git: &Repository) -> bool {
    git.references_glob("refs/replace/*")
        .is_ok_and(|mut refs| refs.next().is_some())
}

/// All commits reachable from `tips` in display order. A `Revwalk` cannot
/// outlive this call, so it runs to the end.
fn full_walk(git: &Repository, tips: &[Oid]) -> Result<Vec<Oid>, ReadError> {
    let mut walk = git.revwalk().map_err(map::error)?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)
        .map_err(map::error)?;
    for &tip in tips {
        walk.push(tip).map_err(map::error)?;
    }
    walk.map(|oid| oid.map_err(map::error)).collect()
}

enum Order {
    Listed { ids: Vec<Oid>, next: usize },
    Incremental(Box<TopoWalk>),
}

struct GitHistoryCursor {
    git: Repository,
    order: Order,
}

impl HistoryCursor for GitHistoryCursor {
    fn next_page(&mut self, max: usize) -> Result<Vec<Commit>, ReadError> {
        let mut page = Vec::with_capacity(max.min(1024));
        while page.len() < max {
            let oid = match &mut self.order {
                Order::Listed { ids, next } => {
                    let oid = ids.get(*next).copied();
                    *next += 1;
                    oid
                }
                Order::Incremental(walk) => walk.next(&self.git)?,
            };
            let Some(oid) = oid else { break };
            let commit = self.git.find_commit(oid).map_err(map::error)?;
            page.push(map::commit(&commit));
        }
        Ok(page)
    }
}

/// A commit in the walk: a position in the commit-graph, or an oid for one
/// the graph does not cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Key {
    Graph(u32),
    Loose(Oid),
}

/// The generation of commits outside the commit-graph. They are newer than
/// every commit in it, as the graph covers all ancestors of its commits.
const INFINITE: u32 = u32::MAX;

struct Node {
    generation: u32,
    time: i64,
    /// The parents of a loose commit; graph commits look them up.
    parents: Vec<Key>,
    /// Children counted during exploration that were not handed out yet.
    pending: u32,
    explored: bool,
    emitted: bool,
}

struct TopoWalk {
    graph: CommitGraph,
    nodes: HashMap<Key, Node>,
    /// Commits whose parents were not counted yet, highest generation first.
    frontier: BinaryHeap<(u32, Key)>,
    /// Candidates to hand out, newest first; ties go to the later candidate.
    /// Entries that became blocked or were handed out are skipped.
    ready: BinaryHeap<(i64, u64, Key)>,
    sequence: u64,
}

impl TopoWalk {
    fn new(graph: CommitGraph) -> Self {
        Self {
            graph,
            nodes: HashMap::new(),
            frontier: BinaryHeap::new(),
            ready: BinaryHeap::new(),
            sequence: 0,
        }
    }

    fn add_tip(&mut self, git: &Repository, oid: Oid) -> Result<(), ReadError> {
        let key = key_in(&self.graph, oid);
        if self.nodes.contains_key(&key) {
            return Ok(());
        }
        self.discover(git, key)?;
        self.make_ready(key);
        Ok(())
    }

    fn next(&mut self, git: &Repository) -> Result<Option<Oid>, ReadError> {
        while let Some(&(_, _, key)) = self.ready.peek() {
            if !self.is_unblocked(key) {
                self.ready.pop();
                continue;
            }
            // Explore every commit that could still be a child of `key`.
            // Children have a strictly higher generation; commits without
            // one are all explored together.
            let generation = self.nodes[&key].generation;
            while let Some(&(next, candidate)) = self.frontier.peek() {
                if next < generation || (next == generation && generation != INFINITE) {
                    break;
                }
                self.frontier.pop();
                self.explore(git, candidate)?;
            }
            self.ready.pop();
            if !self.is_unblocked(key) {
                // It comes back once its last child is handed out.
                continue;
            }
            self.emit(git, key)?;
            return Ok(Some(self.oid(key)));
        }
        Ok(None)
    }

    fn is_unblocked(&self, key: Key) -> bool {
        self.nodes
            .get(&key)
            .is_some_and(|node| !node.emitted && node.pending == 0)
    }

    /// Counts `key` as a pending child of each of its parents.
    fn explore(&mut self, git: &Repository, key: Key) -> Result<(), ReadError> {
        let Some(node) = self.nodes.get_mut(&key) else {
            return Ok(());
        };
        if node.explored || node.emitted {
            return Ok(());
        }
        node.explored = true;
        for parent in self.parents(key)? {
            self.discover(git, parent)?;
            if let Some(node) = self.nodes.get_mut(&parent) {
                node.pending += 1;
            }
        }
        Ok(())
    }

    /// Hands out `key`: its parents no longer wait for it.
    fn emit(&mut self, git: &Repository, key: Key) -> Result<(), ReadError> {
        let Some(node) = self.nodes.get_mut(&key) else {
            return Ok(());
        };
        node.emitted = true;
        let counted = std::mem::replace(&mut node.explored, true);
        for parent in self.parents(key)? {
            self.discover(git, parent)?;
            let Some(node) = self.nodes.get_mut(&parent) else {
                continue;
            };
            if counted {
                node.pending = node.pending.saturating_sub(1);
            }
            if node.pending == 0 {
                self.make_ready(parent);
            }
        }
        Ok(())
    }

    fn make_ready(&mut self, key: Key) {
        let time = self.nodes.get(&key).map_or(0, |node| node.time);
        self.sequence += 1;
        self.ready.push((time, self.sequence, key));
    }

    /// Adds `key` to the walk if it is new.
    fn discover(&mut self, git: &Repository, key: Key) -> Result<(), ReadError> {
        let Entry::Vacant(entry) = self.nodes.entry(key) else {
            return Ok(());
        };
        let node = match key {
            Key::Graph(position) => Node {
                generation: self.graph.generation(position),
                time: self.graph.time(position),
                parents: Vec::new(),
                pending: 0,
                explored: false,
                emitted: false,
            },
            Key::Loose(oid) => {
                let commit = git.find_commit(oid).map_err(map::error)?;
                Node {
                    generation: INFINITE,
                    time: commit.time().seconds(),
                    parents: commit
                        .parent_ids()
                        .map(|parent| key_in(&self.graph, parent))
                        .collect(),
                    pending: 0,
                    explored: false,
                    emitted: false,
                }
            }
        };
        let generation = node.generation;
        entry.insert(node);
        self.frontier.push((generation, key));
        Ok(())
    }

    fn parents(&self, key: Key) -> Result<Vec<Key>, ReadError> {
        match key {
            Key::Graph(position) => self
                .graph
                .parents(position)
                .map(|parents| parents.into_iter().map(Key::Graph).collect())
                .ok_or_else(|| ReadError::Backend("the commit-graph is corrupt".to_owned())),
            Key::Loose(_) => Ok(self
                .nodes
                .get(&key)
                .map(|node| node.parents.clone())
                .unwrap_or_default()),
        }
    }

    fn oid(&self, key: Key) -> Oid {
        match key {
            Key::Loose(oid) => oid,
            // A graph key only comes from a position the graph returned.
            Key::Graph(position) => self.graph.oid(position).unwrap_or(Oid::ZERO_SHA1),
        }
    }
}

fn key_in(graph: &CommitGraph, oid: Oid) -> Key {
    match graph.position(oid) {
        Some(position) => Key::Graph(position),
        None => Key::Loose(oid),
    }
}
