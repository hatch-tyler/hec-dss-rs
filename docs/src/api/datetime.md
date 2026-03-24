# Date/Time Utilities

## Julian Date System

DSS uses a modified Julian date where:
- Julian 0 = 31 December 1899
- Julian 1 = 1 January 1900

## Rust API

```rust
use dss_core::datetime;

// String to Julian
let j = datetime::date_to_julian("15MAR2020");

// Julian to Y/M/D
let (y, m, d) = datetime::julian_to_year_month_day(j);

// Parse various date formats
let ymd = datetime::parse_date("01JAN2020");      // DDMonYYYY
let ymd = datetime::parse_date("2020-03-15");      // ISO
let ymd = datetime::parse_date("3/15/2020");       // US

// Y/M/D to Julian
let j = datetime::year_month_day_to_julian(2020, 3, 15);

// Interval parsing
let (interval_sec, block_months) = datetime::parse_interval("1HOUR").unwrap();
// (3600, 1) = 3600 seconds per value, monthly blocks

// Block operations
let blocks = datetime::generate_block_starts(start_j, end_j, 1); // monthly
let n = datetime::values_in_block(jan1_julian, 3600, 1);          // 744 for January hourly
```

## Python API

```python
j = hecdss_rs.DssFile.date_to_julian("15MAR2020")
y, m, d = hecdss_rs.DssFile.julian_to_ymd(j)
result = hecdss_rs.DssFile.parse_date("2020-03-15")  # (2020, 3, 15) or None
```

## Supported Date Formats

| Format | Example | Notes |
|--------|---------|-------|
| DDMonYYYY | `01JAN2020`, `15MAR2020` | Standard DSS format |
| MonYYYY | `JAN2020` | Day defaults to 1 |
| ISO 8601 | `2020-01-15` | YYYY-MM-DD |
| US | `1/15/2020`, `3-15-2020` | M/D/YYYY or M-D-YYYY |

## Supported Intervals

| E-Part | Seconds | Block Size |
|--------|---------|------------|
| 1MIN | 60 | Monthly |
| 5MIN | 300 | Monthly |
| 15MIN | 900 | Monthly |
| 30MIN | 1800 | Monthly |
| 1HOUR | 3600 | Monthly |
| 6HOUR | 21600 | Monthly |
| 1DAY | 86400 | Yearly |
| 1WEEK | 604800 | Yearly |
| 1MON | Variable | Decade |
| 1YEAR | Variable | Century |
