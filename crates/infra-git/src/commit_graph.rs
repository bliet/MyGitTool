//! A reader for Git's commit-graph files (`objects/info/commit-graph`, or a
//! split chain in `objects/info/commit-graphs/`). They hold the parents,
//! generation number and commit time of every commit they cover, which lets
//! the history walk start without parsing the whole history. libgit2 does
//! not use them for walks.
//!
//! The format is described in Git's `gitformat-commit-graph(5)`. Any file
//! that does not match it is ignored, so the walk falls back to parsing
//! commits.

use git2::Oid;
use std::fs;
use std::path::Path;

const SIGNATURE: &[u8; 4] = b"CGPH";
const VERSION: u8 = 1;
/// Hash version 1 is SHA-1, the only one libgit2 is built with here.
const HASH_VERSION: u8 = 1;
const HASH_LEN: usize = 20;
const HEADER_LEN: usize = 8;
const CHUNK_ENTRY_LEN: usize = 12;
const FANOUT_LEN: usize = 256 * 4;
/// A commit's entry: its tree, two parent words, then the generation and
/// commit time in eight bytes.
const DATA_LEN: usize = HASH_LEN + 16;
const PARENTS_AT: usize = HASH_LEN;
const GENERATION_AT: usize = HASH_LEN + 8;

const NO_PARENT: u32 = 0x7000_0000;
const EXTRA_EDGES: u32 = 0x8000_0000;
const EDGE_INDEX: u32 = 0x7fff_ffff;

/// A commit-graph, possibly made of several layers. Positions are global:
/// the base layer comes first and each later layer continues its numbering.
pub(crate) struct CommitGraph {
    layers: Vec<Layer>,
}

struct Layer {
    data: Vec<u8>,
    /// The global position of the layer's first commit.
    offset: u32,
    count: u32,
    fanout: usize,
    lookup: usize,
    commits: usize,
    edges: Option<(usize, usize)>,
}

impl CommitGraph {
    /// Reads the commit-graph of the object directory `objects`, or `None`
    /// if there is none or it cannot be used.
    pub fn open(objects: &Path) -> Option<Self> {
        let info = objects.join("info");
        let files = match fs::read(info.join("commit-graph")) {
            Ok(data) => vec![data],
            Err(_) => {
                let dir = info.join("commit-graphs");
                let chain = fs::read_to_string(dir.join("commit-graph-chain")).ok()?;
                chain
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(|hash| fs::read(dir.join(format!("graph-{hash}.graph"))).ok())
                    .collect::<Option<Vec<_>>>()?
            }
        };
        let mut layers = Vec::with_capacity(files.len());
        let mut offset = 0u32;
        for data in files {
            let layer = Layer::parse(data, offset)?;
            offset = offset.checked_add(layer.count)?;
            layers.push(layer);
        }
        let graph = Self { layers };
        // Git versions before 2.18 wrote zero generation numbers, which
        // give no ordering.
        let usable = graph.len() > 0 && (0..graph.len()).all(|p| graph.generation(p) > 0);
        usable.then_some(graph)
    }

    fn len(&self) -> u32 {
        self.layers.last().map_or(0, |l| l.offset + l.count)
    }

    /// The position of `oid`, or `None` if the graph does not cover it.
    pub fn position(&self, oid: Oid) -> Option<u32> {
        self.layers
            .iter()
            .find_map(|layer| layer.find(oid.as_bytes()).map(|i| layer.offset + i))
    }

    pub fn oid(&self, position: u32) -> Option<Oid> {
        let (layer, index) = self.locate(position)?;
        Oid::from_bytes(layer.hash(index)).ok()
    }

    /// The topological level: one more than the highest level among the
    /// parents, and 1 for a root commit. Every child has a higher level.
    pub fn generation(&self, position: u32) -> u32 {
        self.locate(position).map_or(0, |(layer, index)| {
            layer.word(layer.data_at(index) + GENERATION_AT) >> 2
        })
    }

    pub fn time(&self, position: u32) -> i64 {
        self.locate(position).map_or(0, |(layer, index)| {
            let at = layer.data_at(index) + GENERATION_AT;
            let high = u64::from(layer.word(at) & 0b11);
            let low = u64::from(layer.word(at + 4));
            // A 34-bit value always fits into an i64.
            ((high << 32) | low) as i64
        })
    }

    /// The parent positions, or `None` if the entry is malformed.
    pub fn parents(&self, position: u32) -> Option<Vec<u32>> {
        let (layer, index) = self.locate(position)?;
        let at = layer.data_at(index) + PARENTS_AT;
        let first = layer.word(at);
        let second = layer.word(at + 4);
        let mut parents = Vec::with_capacity(2);
        if first == NO_PARENT {
            return Some(parents);
        }
        parents.push(first);
        if second == NO_PARENT {
            return Some(parents);
        }
        if second & EXTRA_EDGES == 0 {
            parents.push(second);
            return Some(parents);
        }
        // An octopus merge: the other parents are in the extra-edge list,
        // the last one marked by the high bit.
        let (start, len) = layer.edges?;
        let mut edge = (second & EDGE_INDEX) as usize;
        loop {
            let at = start + edge.checked_mul(4)?;
            if at + 4 > start + len {
                return None;
            }
            let value = layer.word(at);
            parents.push(value & EDGE_INDEX);
            if value & EXTRA_EDGES != 0 {
                return Some(parents);
            }
            edge += 1;
        }
    }

    fn locate(&self, position: u32) -> Option<(&Layer, usize)> {
        self.layers
            .iter()
            .find(|l| position >= l.offset && position - l.offset < l.count)
            .map(|l| (l, (position - l.offset) as usize))
    }
}

impl Layer {
    fn parse(data: Vec<u8>, offset: u32) -> Option<Self> {
        if data.len() < HEADER_LEN
            || &data[..4] != SIGNATURE
            || data[4] != VERSION
            || data[5] != HASH_VERSION
        {
            return None;
        }
        let chunks = usize::from(data[6]);
        let (mut fanout, mut lookup, mut commits, mut edges) = (None, None, None, None);
        for i in 0..chunks {
            let at = HEADER_LEN + i * CHUNK_ENTRY_LEN;
            let id = data.get(at..at + 4)?;
            let start = usize::try_from(read_u64(&data, at + 4)?).ok()?;
            let end = usize::try_from(read_u64(&data, at + 4 + CHUNK_ENTRY_LEN)?).ok()?;
            if start > end || end > data.len() {
                return None;
            }
            match id {
                b"OIDF" => fanout = Some((start, end - start)),
                b"OIDL" => lookup = Some((start, end - start)),
                b"CDAT" => commits = Some((start, end - start)),
                b"EDGE" => edges = Some((start, end - start)),
                _ => {}
            }
        }
        let (fanout, fanout_len) = fanout?;
        let (lookup, lookup_len) = lookup?;
        let (commits, commits_len) = commits?;
        if fanout_len != FANOUT_LEN {
            return None;
        }
        let count = read_u32(&data, fanout + FANOUT_LEN - 4)?;
        let n = count as usize;
        if lookup_len != n * HASH_LEN || commits_len != n * DATA_LEN {
            return None;
        }
        Some(Self {
            data,
            offset,
            count,
            fanout,
            lookup,
            commits,
            edges,
        })
    }

    fn find(&self, hash: &[u8]) -> Option<u32> {
        let first = usize::from(*hash.first()?);
        let start = match first {
            0 => 0,
            _ => self.word(self.fanout + (first - 1) * 4),
        };
        let end = self.word(self.fanout + first * 4);
        let (mut low, mut high) = (start, end.min(self.count));
        while low < high {
            let mid = low + (high - low) / 2;
            match self.hash(mid as usize).cmp(hash) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => return Some(mid),
            }
        }
        None
    }

    fn hash(&self, index: usize) -> &[u8] {
        let at = self.lookup + index * HASH_LEN;
        &self.data[at..at + HASH_LEN]
    }

    fn data_at(&self, index: usize) -> usize {
        self.commits + index * DATA_LEN
    }

    /// A big-endian word inside a chunk whose bounds `parse` checked.
    fn word(&self, at: usize) -> u32 {
        read_u32(&self.data, at).unwrap_or(0)
    }
}

fn read_u32(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at + 4)?;
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

fn read_u64(data: &[u8], at: usize) -> Option<u64> {
    let bytes = data.get(at..at + 8)?;
    Some(u64::from_be_bytes(bytes.try_into().ok()?))
}
