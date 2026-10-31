use application::DateFormat;
use domain::Timestamp;
use ui::view::format::{details_date, full_date, history_date, relative, short_date};

fn at(seconds: i64, offset_minutes: i32) -> Timestamp {
    Timestamp {
        seconds,
        offset_minutes,
    }
}

#[test]
fn formats_in_utc() {
    assert_eq!(short_date(at(1_700_000_000, 0)), "2023-11-14 22:13");
    assert_eq!(
        full_date(at(1_700_000_000, 0)),
        "2023-11-14 22:13:20 +00:00"
    );
}

#[test]
fn applies_the_offset_across_midnight() {
    assert_eq!(
        full_date(at(1_700_000_000, 120)),
        "2023-11-15 00:13:20 +02:00"
    );
    assert_eq!(
        full_date(at(1_700_000_000, -330)),
        "2023-11-14 16:43:20 -05:30"
    );
}

#[test]
fn handles_leap_days_and_the_epoch() {
    assert_eq!(short_date(at(951_782_400, 0)), "2000-02-29 00:00");
    assert_eq!(short_date(at(0, 0)), "1970-01-01 00:00");
    assert_eq!(short_date(at(-1, 0)), "1969-12-31 23:59");
}

/// 2023-11-14 22:13:20 UTC.
const T: i64 = 1_700_000_000;
const DAY: i64 = 86_400;

#[test]
fn history_dates_in_every_format() {
    let time = at(T, 60);
    assert_eq!(history_date(time, DateFormat::Iso, T), "2023-11-14 23:13");
    assert_eq!(
        history_date(time, DateFormat::DayMonthYear, T),
        "14.11.2023 23:13"
    );
    assert_eq!(
        history_date(time, DateFormat::MonthDayYear, T),
        "11/14/2023 11:13 PM"
    );
    assert_eq!(
        history_date(time, DateFormat::Relative, T + 3 * DAY),
        "3 days ago"
    );
}

#[test]
fn twelve_hour_clock_around_midnight_and_noon() {
    let midnight = at(T - 22 * 3600 - 13 * 60, 0);
    assert_eq!(
        history_date(midnight, DateFormat::MonthDayYear, T),
        "11/14/2023 12:00 AM"
    );
    let noon = at(T - 10 * 3600 - 13 * 60, 0);
    assert_eq!(
        history_date(noon, DateFormat::MonthDayYear, T),
        "11/14/2023 12:00 PM"
    );
}

#[test]
fn details_dates_in_every_format() {
    let time = at(T, -330);
    assert_eq!(
        details_date(time, DateFormat::Iso, T),
        "2023-11-14 16:43:20 -05:30"
    );
    assert_eq!(
        details_date(time, DateFormat::DayMonthYear, T),
        "14.11.2023 16:43:20 -05:30"
    );
    assert_eq!(
        details_date(time, DateFormat::MonthDayYear, T),
        "11/14/2023 4:43:20 PM -05:30"
    );
    assert_eq!(
        details_date(time, DateFormat::Relative, T + 2 * 3600),
        "2023-11-14 16:43:20 -05:30 (2 hours ago)"
    );
}

#[test]
fn relative_dates_pick_a_fitting_unit() {
    let time = at(T, 0);
    let ago = |seconds: i64| relative(time, T + seconds).unwrap();
    assert_eq!(ago(0), "just now");
    assert_eq!(ago(59), "just now");
    assert_eq!(ago(60), "1 minute ago");
    assert_eq!(ago(45 * 60), "45 minutes ago");
    assert_eq!(ago(3600), "1 hour ago");
    assert_eq!(ago(DAY), "1 day ago");
    assert_eq!(ago(13 * DAY), "13 days ago");
    assert_eq!(ago(14 * DAY), "2 weeks ago");
    assert_eq!(ago(90 * DAY), "3 months ago");
    assert_eq!(ago(365 * DAY), "1 year ago");
    assert_eq!(ago(800 * DAY), "2 years ago");
}

#[test]
fn future_dates_are_not_relative() {
    let time = at(T + 3600, 0);
    assert_eq!(relative(time, T), None);
    assert_eq!(
        history_date(time, DateFormat::Relative, T),
        "2023-11-14 23:13"
    );
    assert_eq!(relative(at(T + 30, 0), T).as_deref(), Some("just now"));
}
