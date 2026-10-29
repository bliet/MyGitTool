use application::GraphLayout;
use application::HistoryRow;
use domain::{Commit, CommitId, Signature, Timestamp};
use ui::view::history::{
    HistoryList, OVERSCAN, PAGE_SIZE, ROW_HEIGHT, scroll_to_reveal, visible_range,
};

fn row(n: u8) -> HistoryRow {
    let id = CommitId::from_bytes([n; 20]);
    HistoryRow {
        commit: Commit {
            id,
            parents: vec![],
            author: Signature {
                name: "a".into(),
                email: "a@x".into(),
                time: Timestamp {
                    seconds: 0,
                    offset_minutes: 0,
                },
            },
            summary: String::new(),
        },
        graph: GraphLayout::new().push(id, &[]),
    }
}

#[test]
fn visible_range_adds_overscan_and_clamps() {
    assert_eq!(
        visible_range(0.0, 10.0 * ROW_HEIGHT, 1000),
        0..10 + OVERSCAN
    );
    let top = 100.0 * ROW_HEIGHT;
    assert_eq!(
        visible_range(top, 10.0 * ROW_HEIGHT, 1000),
        100 - OVERSCAN..110 + OVERSCAN
    );
    assert_eq!(
        visible_range(top, 10.0 * ROW_HEIGHT, 105),
        100 - OVERSCAN..105
    );
    assert_eq!(visible_range(0.0, 500.0, 0), 0..0);
}

#[test]
fn reveal_only_scrolls_for_hidden_rows_and_centres_them() {
    let viewport = 10.0 * ROW_HEIGHT;
    assert_eq!(scroll_to_reveal(3, 0.0, viewport), None);
    assert_eq!(
        scroll_to_reveal(100, 0.0, viewport),
        Some(100.0 * ROW_HEIGHT - 4.5 * ROW_HEIGHT)
    );
    assert_eq!(scroll_to_reveal(1, 50.0 * ROW_HEIGHT, viewport), Some(0.0));
}

#[test]
fn list_tracks_positions_and_when_to_load_more() {
    let mut list = HistoryList::default();
    assert!(list.needs_more(0));
    list.append(vec![row(1), row(2)], false);
    list.append(vec![row(3)], true);
    assert_eq!(list.len(), 3);
    assert_eq!(list.position(&CommitId::from_bytes([3; 20])), Some(2));
    assert_eq!(list.position(&CommitId::from_bytes([9; 20])), None);
    assert!(!list.needs_more(3), "exhausted lists never need more");

    let mut long = HistoryList::default();
    long.append(
        (0..PAGE_SIZE).map(|i| row((i % 250) as u8)).collect(),
        false,
    );
    assert!(!long.needs_more(0));
    assert!(long.needs_more(PAGE_SIZE / 2));
}
