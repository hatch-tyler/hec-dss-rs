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
