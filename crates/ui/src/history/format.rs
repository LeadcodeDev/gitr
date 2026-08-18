//! Pure text formatting for the history table's `SHA` and `Date` columns.
//!
//! No `gpui` types here either: a commit's identifier and timestamp render the same way
//! regardless of the window they end up painted in.

use domain::{ObjectId, Timestamp};

/// Length of the abbreviated identifier the `SHA` column shows.
pub const ABBREV_LEN: usize = 7;

/// The commit identifier as the `SHA` column shows it.
pub fn abbreviate(id: ObjectId) -> String {
    id.to_hex_prefix(ABBREV_LEN)
}

const MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

const SECONDS_PER_DAY: i64 = 86_400;

/// Civil (year, month, day) for the number of days since the Unix epoch.
///
/// Howard Hinnant's `civil_from_days`: exact for the proleptic Gregorian calendar over
/// the whole `i64` range, using only integer arithmetic. `days` may be negative.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

/// The commit date as the `Date` column shows it, e.g. `"13 Aug"`.
///
/// Rendered in the author's own timezone, per [`Timestamp::offset_minutes`], so it
/// matches the day `git log` prints rather than shifting across midnight in UTC.
pub fn format_commit_date(timestamp: &Timestamp) -> String {
    let local_seconds = timestamp.seconds + i64::from(timestamp.offset_minutes) * 60;
    let days = local_seconds.div_euclid(SECONDS_PER_DAY);
    let (_, month, day) = civil_from_days(days);
    format!("{day} {}", MONTH_NAMES[(month - 1) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(nibble: char) -> ObjectId {
        nibble.to_string().repeat(40).parse().unwrap()
    }

    #[test]
    fn abbreviate_keeps_the_first_seven_hex_characters() {
        assert_eq!(abbreviate(id('a')), "aaaaaaa");
    }

    #[test]
    fn format_commit_date_renders_day_and_month() {
        let timestamp = Timestamp {
            seconds: 1_723_561_445,
            offset_minutes: 0,
        };
        assert_eq!(format_commit_date(&timestamp), "13 Aug");
    }

    #[test]
    fn format_commit_date_handles_a_leap_day() {
        let timestamp = Timestamp {
            seconds: 951_782_400,
            offset_minutes: 0,
        };
        assert_eq!(format_commit_date(&timestamp), "29 Feb");
    }

    #[test]
    fn format_commit_date_handles_timestamps_before_the_epoch() {
        let timestamp = Timestamp {
            seconds: -3_600,
            offset_minutes: 0,
        };
        assert_eq!(format_commit_date(&timestamp), "31 Dec");
    }

    #[test]
    fn format_commit_date_applies_the_authors_offset_across_a_day_boundary() {
        let timestamp = Timestamp {
            seconds: 1_800,
            offset_minutes: -60,
        };
        assert_eq!(format_commit_date(&timestamp), "31 Dec");
    }
}
