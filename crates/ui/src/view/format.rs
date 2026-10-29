//! Date formatting in the timestamp's own UTC offset.

use domain::Timestamp;

/// `2023-11-14 22:13`, for history rows.
pub fn short_date(time: Timestamp) -> String {
    let c = civil(time);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        c.year, c.month, c.day, c.hour, c.minute
    )
}

/// `2023-11-14 22:13:20 +01:00`, for the details column.
pub fn full_date(time: Timestamp) -> String {
    let c = civil(time);
    let sign = if time.offset_minutes < 0 { '-' } else { '+' };
    let offset = time.offset_minutes.unsigned_abs();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {sign}{:02}:{:02}",
        c.year,
        c.month,
        c.day,
        c.hour,
        c.minute,
        c.second,
        offset / 60,
        offset % 60
    )
}

struct Civil {
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

/// Converts to the calendar date and time in the timestamp's offset, using
/// Howard Hinnant's `civil_from_days` algorithm.
fn civil(time: Timestamp) -> Civil {
    let local = time.seconds + i64::from(time.offset_minutes) * 60;
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);

    // All casts below are of values already bounded to small ranges.
    Civil {
        year,
        month: month as u32,
        day: day as u32,
        hour: (secs / 3600) as u32,
        minute: (secs % 3600 / 60) as u32,
        second: (secs % 60) as u32,
    }
}
