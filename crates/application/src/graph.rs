//! Lane layout for the commit graph.
//!
//! Rows arrive in topological order. Each lane waits for one commit (the
//! parent of the commit above it). A row's graph is split into a top half
//! (from the row's top edge to the commit dot) and a bottom half (from the dot
//! to the bottom edge), which is all a renderer needs to draw it.

use domain::CommitId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeHalf {
    /// From lane `from` at the top of the row to lane `to` at its centre.
    Top,
    /// From lane `from` at the centre of the row to lane `to` at its bottom.
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphEdge {
    pub from: usize,
    pub to: usize,
    pub half: EdgeHalf,
    /// An index into the renderer's palette, stable along a lane.
    pub color: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow {
    /// The lane of the commit dot.
    pub node: usize,
    pub node_color: usize,
    /// The number of lanes the row needs room for.
    pub width: usize,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Copy)]
struct Lane {
    expects: CommitId,
    color: usize,
}

/// Computes graph rows incrementally, one commit at a time.
#[derive(Debug, Default, Clone)]
pub struct GraphLayout {
    lanes: Vec<Option<Lane>>,
    next_color: usize,
}

impl GraphLayout {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, id: CommitId, parents: &[CommitId]) -> GraphRow {
        let width_before = self.lanes.len();
        let mut edges = Vec::new();

        let incoming: Vec<usize> = self.lanes_expecting(id).collect();
        let (node, node_color) = match incoming.first() {
            Some(&lane) => (lane, self.color_of(lane)),
            None => (self.free_lane(), self.new_color()),
        };

        // Top half: lanes that wait for this commit bend into the dot, all
        // others pass straight through.
        let mut passing = Vec::new();
        for (i, lane) in self.lanes.iter().enumerate() {
            let Some(lane) = lane else { continue };
            if lane.expects == id {
                edges.push(edge(i, node, EdgeHalf::Top, lane.color));
            } else {
                edges.push(edge(i, i, EdgeHalf::Top, lane.color));
                passing.push(i);
            }
        }
        for &lane in &incoming {
            self.lanes[lane] = None;
        }

        // Bottom half: pass-through lanes continue, and the dot connects to
        // a lane for each parent. The first parent keeps the commit's lane
        // unless another lane already waits for it.
        for &i in &passing {
            edges.push(edge(i, i, EdgeHalf::Bottom, self.color_of(i)));
        }
        for (index, &parent) in parents.iter().enumerate() {
            let existing = self.lanes_expecting(parent).next();
            let target = match existing {
                Some(existing) => existing,
                None => {
                    let (lane, color) =
                        if index == 0 && self.lanes.get(node).is_none_or(Option::is_none) {
                            (node, node_color)
                        } else {
                            (self.free_lane(), self.new_color())
                        };
                    self.set_lane(
                        lane,
                        Lane {
                            expects: parent,
                            color,
                        },
                    );
                    lane
                }
            };
            edges.push(edge(node, target, EdgeHalf::Bottom, self.color_of(target)));
        }

        while matches!(self.lanes.last(), Some(None)) {
            self.lanes.pop();
        }
        let width = width_before.max(self.lanes.len()).max(node + 1);
        GraphRow {
            node,
            node_color,
            width,
            edges,
        }
    }

    fn lanes_expecting(&self, id: CommitId) -> impl Iterator<Item = usize> + '_ {
        self.lanes
            .iter()
            .enumerate()
            .filter(move |(_, lane)| lane.is_some_and(|lane| lane.expects == id))
            .map(|(i, _)| i)
    }

    fn color_of(&self, lane: usize) -> usize {
        self.lanes
            .get(lane)
            .copied()
            .flatten()
            .map_or(0, |lane| lane.color)
    }

    fn new_color(&mut self) -> usize {
        let color = self.next_color;
        self.next_color += 1;
        color
    }

    /// The leftmost empty lane, or a new one on the right.
    fn free_lane(&self) -> usize {
        self.lanes
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.lanes.len())
    }

    fn set_lane(&mut self, index: usize, lane: Lane) {
        if index >= self.lanes.len() {
            self.lanes.resize(index + 1, None);
        }
        self.lanes[index] = Some(lane);
    }
}

fn edge(from: usize, to: usize, half: EdgeHalf, color: usize) -> GraphEdge {
    GraphEdge {
        from,
        to,
        half,
        color,
    }
}
