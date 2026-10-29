use domain::Timestamp;
use ui::view::format::{full_date, short_date};

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
