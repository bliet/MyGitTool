mod common;

use application::{EdgeHalf, GraphEdge, GraphLayout, GraphRow};
use common::id;

/// Pushes `(commit, parents)` rows and returns the graph rows.
fn layout(rows: &[(u8, &[u8])]) -> Vec<GraphRow> {
    let mut layout = GraphLayout::new();
    rows.iter()
        .map(|(n, parents)| {
            let parents: Vec<_> = parents.iter().map(|&p| id(p)).collect();
            layout.push(id(*n), &parents)
        })
        .collect()
}

/// Edges as `(from, to)` pairs of one half, ignoring colours.
fn half(row: &GraphRow, half: EdgeHalf) -> Vec<(usize, usize)> {
    let mut pairs: Vec<_> = row
        .edges
        .iter()
        .filter(|e| e.half == half)
        .map(|e| (e.from, e.to))
        .collect();
    pairs.sort();
    pairs
}

fn nodes(rows: &[GraphRow]) -> Vec<usize> {
    rows.iter().map(|r| r.node).collect()
}

#[test]
fn linear_history_stays_in_lane_zero() {
    let rows = layout(&[(3, &[2]), (2, &[1]), (1, &[])]);
    assert_eq!(nodes(&rows), [0, 0, 0]);
    assert_eq!(half(&rows[0], EdgeHalf::Top), []);
    assert_eq!(half(&rows[0], EdgeHalf::Bottom), [(0, 0)]);
    assert_eq!(half(&rows[1], EdgeHalf::Top), [(0, 0)]);
    assert_eq!(
        half(&rows[2], EdgeHalf::Bottom),
        [],
        "root commit ends the lane"
    );
    assert!(rows.iter().all(|r| r.width == 1));
    let colors: Vec<_> = rows.iter().map(|r| r.node_color).collect();
    assert_eq!(colors, [0, 0, 0], "one lane keeps one colour");
}

#[test]
fn merge_opens_a_lane_for_the_second_parent_and_closes_it_at_the_fork() {
    // 4 merges 3 (main) and 2 (feature), both based on 1.
    let rows = layout(&[(4, &[3, 2]), (3, &[1]), (2, &[1]), (1, &[])]);
    assert_eq!(nodes(&rows), [0, 0, 1, 0]);

    assert_eq!(half(&rows[0], EdgeHalf::Bottom), [(0, 0), (0, 1)]);
    // 3 continues in lane 0 while lane 1 passes through.
    assert_eq!(half(&rows[1], EdgeHalf::Top), [(0, 0), (1, 1)]);
    assert_eq!(half(&rows[1], EdgeHalf::Bottom), [(0, 0), (1, 1)]);
    // 2's first parent 1 is already awaited by lane 0, so lane 1 bends into it.
    assert_eq!(half(&rows[2], EdgeHalf::Bottom), [(0, 0), (1, 0)]);
    assert_eq!(half(&rows[3], EdgeHalf::Top), [(0, 0)]);
    assert_eq!(rows[3].width, 1, "the closed lane is trimmed");
    assert_ne!(rows[0].node_color, rows[2].node_color);
}

#[test]
fn two_branch_tips_with_a_common_parent_join_into_one_lane() {
    // Tips 3 and 2 both have parent 1; no merge commit.
    let rows = layout(&[(3, &[1]), (2, &[1]), (1, &[])]);
    assert_eq!(nodes(&rows), [0, 1, 0]);
    assert_eq!(
        half(&rows[1], EdgeHalf::Top),
        [(0, 0)],
        "lane 0 passes, nothing enters 2 from above"
    );
    assert_eq!(half(&rows[1], EdgeHalf::Bottom), [(0, 0), (1, 0)]);
    assert_eq!(rows[1].width, 2);
}

#[test]
fn freed_lanes_are_reused_from_the_left() {
    // Lane 1 opens for tip 4, closes at 3; tip 5's history then reuses lane 1.
    let rows = layout(&[(6, &[2]), (4, &[3]), (3, &[2]), (5, &[2]), (2, &[])]);
    assert_eq!(nodes(&rows), [0, 1, 1, 1, 0]);
}

#[test]
fn edge_colours_follow_the_lane_they_belong_to() {
    let rows = layout(&[(4, &[3, 2]), (3, &[1]), (2, &[1])]);
    let feature_color = rows[2].node_color;
    let into_feature = rows[0]
        .edges
        .iter()
        .find(|e| e.half == EdgeHalf::Bottom && e.to == 1)
        .copied();
    assert_eq!(
        into_feature,
        Some(GraphEdge {
            from: 0,
            to: 1,
            half: EdgeHalf::Bottom,
            color: feature_color
        })
    );
}
