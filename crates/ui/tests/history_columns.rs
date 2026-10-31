use ui::view::history_columns::{HistoryColumns, HistoryField, MAX_WIDTH, MIN_WIDTH};

#[test]
fn resize_sets_one_column() {
    let mut columns = HistoryColumns::default();
    columns.resize(HistoryField::Date, 200.0);
    assert_eq!(columns.width(HistoryField::Date), 200.0);
    assert_eq!(columns.author, HistoryColumns::default().author);
    assert_eq!(columns.hash, HistoryColumns::default().hash);
}

#[test]
fn resize_keeps_the_limits() {
    let mut columns = HistoryColumns::default();
    columns.resize(HistoryField::Author, 5.0);
    columns.resize(HistoryField::Hash, 5000.0);
    assert_eq!(columns.author, MIN_WIDTH);
    assert_eq!(columns.hash, MAX_WIDTH);
}

#[test]
fn dragging_the_left_edge_left_widens_the_column() {
    assert_eq!(HistoryColumns::dragged_width(150.0, 500.0, 450.0), 200.0);
    assert_eq!(HistoryColumns::dragged_width(150.0, 500.0, 530.0), 120.0);
}

#[test]
fn restored_widths_are_clamped() {
    let columns = HistoryColumns::restored(1.0, 100.0, 9999.0);
    assert_eq!(
        (columns.author, columns.date, columns.hash),
        (MIN_WIDTH, 100.0, MAX_WIDTH)
    );
}

#[test]
fn style_sets_the_css_widths() {
    let style = HistoryColumns::default().style();
    assert!(style.contains("--author-width: 150px;"));
    assert!(style.contains("--date-width: 120px;"));
    assert!(style.contains("--hash-width: 64px;"));
}
