//! When the report was made.
//!
//! A report without a date is a report you cannot trust a month later: the
//! figures may have been regenerated since, and there is no way to tell. The
//! stamp is UTC and spelled out, because a report is read on a machine other
//! than the one that wrote it as often as not.
//!
//! Written by hand rather than taken from a date library: this is the only date
//! this project formats, and the conversion is a dozen lines that can be tested
//! against dates whose answers are known.

use std::time::SystemTime;

/// `2026-08-18 16:05 UTC`, or `unknown` for a clock before the epoch.
pub fn stamp(time: SystemTime) -> String {
    let Ok(elapsed) = time.duration_since(SystemTime::UNIX_EPOCH) else {
        // A clock set before 1970 would otherwise produce a negative date,
        // which is worse than admitting the machine cannot say.
        return "unknown".to_string();
    };

    let seconds = elapsed.as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);

    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rest / 3600,
        (rest % 3600) / 60
    )
}

/// Year, month and day from a count of days since 1970-01-01.
///
/// Howard Hinnant's `civil_from_days`, which is exact over the whole range of
/// the proleptic Gregorian calendar. The trick is to count from March, so that
/// the leap day lands at the *end* of a year and the month lengths repeat on a
/// five-month cycle — which is what removes every special case except the one
/// shift at the end.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Shift the epoch to 0000-03-01.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);

    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);

    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;

    (year + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(seconds: u64) -> String {
        stamp(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
    }

    #[test]
    fn the_epoch_is_the_first_of_january_nineteen_seventy() {
        assert_eq!(at(0), "1970-01-01 00:00 UTC");
    }

    #[test]
    fn a_known_instant_is_spelled_correctly() {
        // Checked against `date -u -d @1787069100`.
        assert_eq!(at(1_787_069_100), "2026-08-18 16:05 UTC");
    }

    /// The leap-year rule is the part of a hand-written conversion that goes
    /// wrong, and it goes wrong once every four years.
    #[test]
    fn the_twenty_ninth_of_february_exists_in_a_leap_year() {
        // 2024-02-29 12:00:00 UTC
        assert_eq!(at(1_709_208_000), "2024-02-29 12:00 UTC");
    }

    /// A century that is not a leap year, which the four-year rule alone gets
    /// wrong: 1900 had no 29 February.
    #[test]
    fn the_end_of_a_non_leap_century_is_handled() {
        // 2100-03-01 00:00:00 UTC — a day that only lands correctly if 2100 is
        // not treated as a leap year.
        assert_eq!(at(4_107_542_400), "2100-03-01 00:00 UTC");
    }

    #[test]
    fn the_last_day_of_a_year_does_not_roll_over_early() {
        // 2025-12-31 23:59:00 UTC
        assert_eq!(at(1_767_225_540), "2025-12-31 23:59 UTC");
    }

    /// A machine whose clock is set before 1970 must not produce a negative
    /// date; it produces no date.
    #[test]
    fn a_clock_before_the_epoch_says_it_does_not_know() {
        let before = SystemTime::UNIX_EPOCH - Duration::from_secs(1);
        assert_eq!(stamp(before), "unknown");
    }
}
