//! HEC-DSS date/time utilities.
//!
//! Julian day dates in DSS use a base of 01Jan1900 = day 1 (31Dec1899 = day 0).
//! The constant `JULIAN_BASE_DATE = 693960` offsets from the astronomical Julian day.

/// Base date constant matching the C library.
/// Julian 0 = 31 Dec 1899, Julian 1 = 01 Jan 1900.
const JULIAN_BASE_DATE: i32 = 693960;

/// Days before each month (non-leap year), 0-indexed.
const NDAY: [i32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

/// Month name abbreviations for parsing.
const MONTH_NAMES: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN",
    "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

/// Check if a year is a leap year.
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Convert year, month, day to DSS Julian date.
///
/// Month is 1-12, day is 1-31. Returns the Julian day number
/// where 01Jan1900 = 1 and 31Dec1899 = 0.
pub fn year_month_day_to_julian(year: i32, month: i32, day: i32) -> i32 {
    // Adjust out-of-range months
    let mut y = year;
    let mut m = month;
    if !(1..=12).contains(&m) {
        let iyears = m / 12;
        m -= iyears * 12;
        y += iyears;
        if m < 1 {
            y -= 1;
            m += 12;
        }
    }

    let leap_days = if y > 4 {
        let y1 = y - 1;
        (y1 / 4) + (y1 / 400) - (y1 / 100)
    } else if y > 0 {
        1
    } else {
        let y1 = y + 1;
        (y1 / 4) + (y1 / 400) - (y1 / 100)
    };

    let leap_check = if is_leap_year(y) && y >= 0 && m > 2 {
        1
    } else if is_leap_year(y) && y < 0 && m < 3 {
        -1
    } else {
        0
    };

    if !(1..=12).contains(&m) {
        return i32::MIN; // UNDEFINED_TIME
    }

    (y * 365) + leap_days + NDAY[(m - 1) as usize] + day + leap_check - JULIAN_BASE_DATE
}

/// Convert a DSS Julian date to year, month, day.
pub fn julian_to_year_month_day(julian: i32) -> (i32, i32, i32) {
    let julc = julian + JULIAN_BASE_DATE;
    let mut iyear = (julc as f64 / 365.2425) as i32;

    // Find the year by searching forward
    if iyear > 0 {
        for _ in 0..100 {
            let j = year_month_day_to_julian(iyear, 1, 1);
            if j > julian {
                break;
            }
            iyear += 1;
        }
        iyear -= 1;
    } else {
        for _ in 0..100 {
            let j = year_month_day_to_julian(iyear, 1, 1);
            if julian >= j {
                break;
            }
            iyear -= 1;
        }
    }

    // Find the month
    let mut imonth = 1;
    let julb = year_month_day_to_julian(iyear, 1, 1);
    let approx = ((julian - julb + 28) / 28).min(12);
    let mut found = false;
    for i in approx..=12 {
        let j = year_month_day_to_julian(iyear, i, 1);
        if j > julian {
            imonth = i - 1;
            found = true;
            break;
        }
    }
    if !found {
        imonth = 12;
    }
    if imonth < 1 {
        imonth = 1;
    }

    let j = year_month_day_to_julian(iyear, imonth, 1);
    let iday = julian - j + 1;

    (iyear, imonth, iday)
}

/// Parse a date string into (year, month, day).
///
/// Supported formats:
/// - `"01JAN2020"`, `"15MAR2020"`, `"2Jun1985"` (DDMonYYYY)
/// - `"JAN2020"`, `"Mar2020"` (MonYYYY, assumes day 1)
/// - `"2020-01-15"`, `"1985-06-02"` (ISO YYYY-MM-DD)
/// - `"1/15/2020"`, `"6-2-1985"` (M/D/YYYY or M-D-YYYY)
///
/// Returns `None` if the date cannot be parsed.
pub fn parse_date(date_str: &str) -> Option<(i32, i32, i32)> {
    let s = date_str.trim().to_uppercase();
    if s.is_empty() {
        return None;
    }

    // Try DDMonYYYY format (e.g., "01JAN2020", "15MAR2020", "2Jun1985")
    for (mi, name) in MONTH_NAMES.iter().enumerate() {
        if let Some(pos) = s.find(name) {
            let day_part = &s[..pos];
            let year_part = &s[pos + 3..];
            let day = if day_part.is_empty() { 1 } else { day_part.parse::<i32>().ok()? };
            let mut year = year_part.parse::<i32>().ok()?;
            if year < 100 && year_part.len() <= 2 {
                year = if year >= 50 { 1900 + year } else { 2000 + year };
            }
            return Some((year, (mi + 1) as i32, day));
        }
    }

    // Try ISO format YYYY-MM-DD
    if s.len() >= 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' {
        let year = s[0..4].parse::<i32>().ok()?;
        let month = s[5..7].parse::<i32>().ok()?;
        let day = s[8..10].parse::<i32>().ok()?;
        return Some((year, month, day));
    }

    // Try M/D/YYYY or M-D-YYYY
    let sep = if s.contains('/') { '/' } else if s.contains('-') { '-' } else { return None };
    let parts: Vec<&str> = s.split(sep).collect();
    if parts.len() == 3 {
        let a = parts[0].parse::<i32>().ok()?;
        let b = parts[1].parse::<i32>().ok()?;
        let c = parts[2].parse::<i32>().ok()?;
        if a > 31 {
            // YYYY-MM-DD
            return Some((a, b, c));
        } else {
            // M/D/YYYY
            let year = if c < 100 { if c >= 50 { 1900 + c } else { 2000 + c } } else { c };
            return Some((year, a, b));
        }
    }

    None
}

/// Convert a date string to a DSS Julian date.
///
/// Returns `i32::MIN` (UNDEFINED_TIME) if the string cannot be parsed.
pub fn date_to_julian(date_str: &str) -> i32 {
    match parse_date(date_str) {
        Some((y, m, d)) => year_month_day_to_julian(y, m, d),
        None => i32::MIN,
    }
}

// ---------------------------------------------------------------------------
// Interval / block mapping for multi-block time series
// ---------------------------------------------------------------------------

/// Parse an E-part interval string to seconds and block size.
/// Returns (interval_seconds, block_months).
/// block_months: 1 = monthly blocks, 12 = yearly blocks.
pub fn parse_interval(e_part: &str) -> Option<(i32, i32)> {
    let upper = e_part.to_uppercase();
    match upper.as_str() {
        "1MIN" | "1MINUTE" => Some((60, 1)),
        "2MIN" => Some((120, 1)),
        "3MIN" => Some((180, 1)),
        "5MIN" => Some((300, 1)),
        "6MIN" => Some((360, 1)),
        "10MIN" => Some((600, 1)),
        "15MIN" => Some((900, 1)),
        "20MIN" => Some((1200, 1)),
        "30MIN" => Some((1800, 1)),
        "1HOUR" => Some((3600, 1)),
        "2HOUR" => Some((7200, 1)),
        "3HOUR" => Some((10800, 1)),
        "4HOUR" => Some((14400, 1)),
        "6HOUR" => Some((21600, 1)),
        "8HOUR" => Some((28800, 1)),
        "12HOUR" => Some((43200, 1)),
        "1DAY" => Some((86400, 12)),
        "1WEEK" => Some((604800, 12)),
        "1MON" | "1MONTH" => Some((0, 120)),  // variable interval, decade blocks
        "1YEAR" => Some((0, 1200)),            // century blocks
        _ => None,
    }
}

/// Format a Julian date as a DSS D-part string (e.g., "01Jan2020").
pub fn julian_to_dpart(julian: i32) -> String {
    let (y, m, d) = julian_to_year_month_day(julian);
    if !(1..=12).contains(&m) { return String::new(); }
    format!("{:02}{}{}", d, MONTH_NAMES[(m - 1) as usize], y)
}

/// Get the Julian date of the first day of the month containing the given Julian date.
pub fn block_start_monthly(julian: i32) -> i32 {
    let (y, m, _) = julian_to_year_month_day(julian);
    year_month_day_to_julian(y, m, 1)
}

/// Get the Julian date of the first day of the year containing the given Julian date.
pub fn block_start_yearly(julian: i32) -> i32 {
    let (y, _, _) = julian_to_year_month_day(julian);
    year_month_day_to_julian(y, 1, 1)
}

/// Generate block start Julian dates between two dates for a given block size.
pub fn generate_block_starts(start_julian: i32, end_julian: i32, block_months: i32) -> Vec<i32> {
    let mut blocks = Vec::new();
    let (mut y, mut m, _) = julian_to_year_month_day(start_julian);

    // Align to block boundary
    if block_months <= 1 {
        // monthly: already aligned by month
    } else if block_months <= 12 {
        m = 1; // yearly: align to January
    } else {
        m = 1;
        y = y - (y % 10); // decade: align to decade start
    }

    loop {
        let block_julian = year_month_day_to_julian(y, m, 1);
        if block_julian > end_julian { break; }
        if block_julian >= start_julian - 31 * block_months { // allow one block before start
            blocks.push(block_julian);
        }
        m += block_months;
        while m > 12 {
            m -= 12;
            y += 1;
        }
    }
    blocks
}

/// Number of values in a time block for a given interval.
/// For monthly blocks with hourly data: days_in_month * 24.
pub fn values_in_block(block_julian: i32, interval_seconds: i32, block_months: i32) -> i32 {
    if interval_seconds <= 0 { return 0; }
    let (y, m, _) = julian_to_year_month_day(block_julian);
    let next_m = m + block_months;
    let (ny, nm) = if next_m > 12 { (y + 1, next_m - 12) } else { (y, next_m) };
    let next_julian = year_month_day_to_julian(ny, nm, 1);
    let days = next_julian - block_julian;
    (days as i64 * 86400 / interval_seconds as i64) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ymd_to_julian_base() {
        // 01Jan1900 = Julian 1
        assert_eq!(year_month_day_to_julian(1900, 1, 1), 1);
        // 31Dec1899 = Julian 0
        assert_eq!(year_month_day_to_julian(1899, 12, 31), 0);
    }

    #[test]
    fn test_julian_roundtrip() {
        for julian in [0, 1, 100, 1000, 36524, 43831, 44000] {
            let (y, m, d) = julian_to_year_month_day(julian);
            let j2 = year_month_day_to_julian(y, m, d);
            assert_eq!(j2, julian, "Roundtrip failed for julian={julian}: {y}-{m}-{d}");
        }
    }

    #[test]
    fn test_known_dates() {
        // 15Mar2020
        let j = year_month_day_to_julian(2020, 3, 15);
        let (y, m, d) = julian_to_year_month_day(j);
        assert_eq!((y, m, d), (2020, 3, 15));

        // 01Jan2020
        let j2 = year_month_day_to_julian(2020, 1, 1);
        let (y2, m2, d2) = julian_to_year_month_day(j2);
        assert_eq!((y2, m2, d2), (2020, 1, 1));
    }

    #[test]
    fn test_parse_dss_format() {
        assert_eq!(parse_date("01JAN2020"), Some((2020, 1, 1)));
        assert_eq!(parse_date("15MAR2020"), Some((2020, 3, 15)));
        assert_eq!(parse_date("2Jun1985"), Some((1985, 6, 2)));
        assert_eq!(parse_date("JAN2020"), Some((2020, 1, 1)));
    }

    #[test]
    fn test_parse_iso_format() {
        assert_eq!(parse_date("2020-01-15"), Some((2020, 1, 15)));
        assert_eq!(parse_date("1985-06-02"), Some((1985, 6, 2)));
    }

    #[test]
    fn test_parse_us_format() {
        assert_eq!(parse_date("1/15/2020"), Some((2020, 1, 15)));
        assert_eq!(parse_date("6-2-1985"), Some((1985, 6, 2)));
    }

    #[test]
    fn test_date_to_julian_string() {
        let j1 = date_to_julian("15MAR2020");
        let j2 = year_month_day_to_julian(2020, 3, 15);
        assert_eq!(j1, j2);

        assert_eq!(date_to_julian(""), i32::MIN);
        assert_eq!(date_to_julian("garbage"), i32::MIN);
    }

    #[test]
    fn test_parse_interval() {
        assert_eq!(parse_interval("1HOUR"), Some((3600, 1)));
        assert_eq!(parse_interval("1DAY"), Some((86400, 12)));
        assert_eq!(parse_interval("15MIN"), Some((900, 1)));
        assert!(parse_interval("UNKNOWN").is_none());
    }

    #[test]
    fn test_julian_to_dpart() {
        let j = year_month_day_to_julian(2020, 1, 1);
        let dp = julian_to_dpart(j);
        assert_eq!(dp, "01JAN2020");
    }

    #[test]
    fn test_values_in_block() {
        // January 2020: 31 days * 24 hours = 744
        let jan = year_month_day_to_julian(2020, 1, 1);
        assert_eq!(values_in_block(jan, 3600, 1), 744);
        // February 2020 (leap): 29 * 24 = 696
        let feb = year_month_day_to_julian(2020, 2, 1);
        assert_eq!(values_in_block(feb, 3600, 1), 696);
    }

    #[test]
    fn test_generate_block_starts() {
        let start = year_month_day_to_julian(2020, 1, 15);
        let end = year_month_day_to_julian(2020, 4, 15);
        let blocks = generate_block_starts(start, end, 1);
        assert!(blocks.len() >= 3); // Jan, Feb, Mar, Apr
    }

    #[test]
    fn test_leap_years() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2020));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2019));

        // 29 Feb 2020 should be valid
        let j = year_month_day_to_julian(2020, 2, 29);
        let (y, m, d) = julian_to_year_month_day(j);
        assert_eq!((y, m, d), (2020, 2, 29));
    }
}
