use ui::view::columns::{ColumnLayout, MIN_MIDDLE, MIN_SIDE, SPLITTER, Side};

#[test]
fn resize_within_limits() {
    let mut layout = ColumnLayout::default();
    layout.resize(Side::Left, 300.0, 1400.0);
    assert_eq!(layout.left, 300.0);
}

#[test]
fn resize_keeps_the_minimum_side_width() {
    let mut layout = ColumnLayout::default();
    layout.resize(Side::Right, 10.0, 1400.0);
    assert_eq!(layout.right, MIN_SIDE);
}

#[test]
fn resize_keeps_room_for_the_middle_column() {
    let mut layout = ColumnLayout::default();
    layout.resize(Side::Left, 5000.0, 1400.0);
    assert_eq!(
        layout.left,
        1400.0 - layout.right - MIN_MIDDLE - 2.0 * SPLITTER
    );
}

#[test]
fn dragging_the_right_splitter_left_widens_the_right_column() {
    assert_eq!(
        ColumnLayout::dragged_width(Side::Right, 380.0, 1000.0, 950.0),
        430.0
    );
    assert_eq!(
        ColumnLayout::dragged_width(Side::Left, 260.0, 260.0, 300.0),
        300.0
    );
}

#[test]
fn grid_template_lists_all_five_tracks() {
    let layout = ColumnLayout {
        left: 200.0,
        right: 300.0,
    };
    assert_eq!(layout.grid_template(), "200px 4px minmax(0, 1fr) 4px 300px");
}

#[test]
fn restored_widths_keep_the_minimum() {
    let layout = ColumnLayout::restored(10.0, 500.0);
    assert_eq!((layout.left, layout.right), (MIN_SIDE, 500.0));
}

#[test]
fn fitted_leaves_widths_that_fit_alone() {
    let layout = ColumnLayout::default();
    assert_eq!(layout.fitted(1400.0), layout);
}

#[test]
fn fitted_shrinks_both_sides_to_keep_the_middle() {
    let layout = ColumnLayout::restored(460.0, 560.0);
    let total = 1000.0;
    let fitted = layout.fitted(total);
    assert_eq!(
        fitted.left + fitted.right,
        total - MIN_MIDDLE - 2.0 * SPLITTER
    );
    // The right side had more to spare, so it gives up more.
    assert!(layout.right - fitted.right > layout.left - fitted.left);
}

#[test]
fn fitted_never_goes_below_the_minimum() {
    let layout = ColumnLayout::restored(400.0, 400.0);
    let fitted = layout.fitted(500.0);
    assert_eq!((fitted.left, fitted.right), (MIN_SIDE, MIN_SIDE));
}
