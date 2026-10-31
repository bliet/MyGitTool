//! Date formatting in the timestamp's own UTC offset.

use application::DateFormat;
use domain::Timestamp;

/// The date of a history row in `format`. `now` (seconds since the epoch)
/// is the reference for relative dates.
pub fn history_date(time: Timestamp, format: DateFormat, now: i64) -> String {
    let c = civil(time);
    match format {
        DateFormat::Iso => short_date(time),
        DateFormat::DayMonthYear => format!(
            "{:02}.{:02}.{:04} {:02}:{:02}",
            c.day, c.month, c.year, c.hour, c.minute
        ),
        DateFormat::MonthDayYear => {
            let (hour, half) = twelve_hour(c.hour);
            format!(
                "{:02}/{:02}/{:04} {hour}:{:02} {half}",
                c.month, c.day, c.year, c.minute
            )
        }
        DateFormat::Relative => relative(time, now).unwrap_or_else(|| short_date(time)),
    }
}

/// The date for the details column in `format`, with seconds and offset.
/// The relative format shows the full date with the relative one after it.
pub fn details_date(time: Timestamp, format: DateFormat, now: i64) -> String {
    let c = civil(time);
    let offset = offset(time);
    match format {
        DateFormat::Iso => full_date(time),
        DateFormat::DayMonthYear => format!(
            "{:02}.{:02}.{:04} {:02}:{:02}:{:02} {offset}",
            c.day, c.month, c.year, c.hour, c.minute, c.second
        ),
        DateFormat::MonthDayYear => {
            let (hour, half) = twelve_hour(c.hour);
            format!(
                "{:02}/{:02}/{:04} {hour}:{:02}:{:02} {half} {offset}",
                c.month, c.day, c.year, c.minute, c.second
            )
        }
        DateFormat::Relative => match relative(time, now) {
            Some(ago) => format!("{} ({ago})", full_date(time)),
            None => full_date(time),
        },
    }
}

/// How long before `now` the time was, e.g. `3 days ago`. `None` for a
/// time more than a minute in the future (a skewed clock).
pub fn relative(time: Timestamp, now: i64) -> Option<String> {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    let ago = now - time.seconds;
    if ago < -MINUTE {
        return None;
    }
    let (count, unit) = match ago {
        _ if ago < MINUTE => return Some("just now".to_owned()),
        _ if ago < HOUR => (ago / MINUTE, "minute"),
        _ if ago < DAY => (ago / HOUR, "hour"),
        // Days up to two weeks, then weeks up to two months.
        _ if ago < 14 * DAY => (ago / DAY, "day"),
        _ if ago < 60 * DAY => (ago / (7 * DAY), "week"),
        _ if ago < 365 * DAY => (ago / (30 * DAY), "month"),
        _ => (ago / (365 * DAY), "year"),
    };
    Some(match count {
        1 => format!("1 {unit} ago"),
        n => format!("{n} {unit}s ago"),
    })
}

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
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {}",
        c.year,
        c.month,
        c.day,
        c.hour,
        c.minute,
        c.second,
        offset(time)
    )
}

/// `+01:00`
fn offset(time: Timestamp) -> String {
    let sign = if time.offset_minutes < 0 { '-' } else { '+' };
    let offset = time.offset_minutes.unsigned_abs();
    format!("{sign}{:02}:{:02}", offset / 60, offset % 60)
}

/// The hour on a 12-hour clock and `AM` or `PM`.
fn twelve_hour(hour: u32) -> (u32, &'static str) {
    let half = if hour < 12 { "AM" } else { "PM" };
    let hour = match hour % 12 {
        0 => 12,
        h => h,
    };
    (hour, half)
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
