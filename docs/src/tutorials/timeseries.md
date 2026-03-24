# Time Series

Time series are the most common DSS data type. They store sequences of values at regular or irregular time intervals.

## Regular Time Series

### Writing

```rust
use dss_core::NativeDssFile;

let mut dss = NativeDssFile::create("flow.dss")?;
let values = vec![100.0, 200.0, 300.0, 400.0, 500.0];
dss.write_ts(
    "/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/OBS/",
    &values,
    "CFS",        // units
    "INST-VAL",   // data type (INST-VAL, PER-AVER, PER-CUM)
)?;
```

**Python:**
```python
import numpy as np
dss.write_ts("/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/OBS/",
             np.array([100.0, 200.0, 300.0, 400.0, 500.0]),
             "CFS", "INST-VAL")
```

### Reading

```rust
if let Some(ts) = dss.read_ts("/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/OBS/")? {
    println!("Values: {:?}", &ts.values[..5]);
    println!("Units: {}", ts.units);
    println!("Type: {}", ts.data_type_str);
    println!("Record type: {}", ts.record_type);  // 105 = RTD (doubles)
}
```

**Python:**
```python
values = dss.read_ts("/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/OBS/")
# values is a numpy array
print(f"First 5: {values[:5]}")
```

## Multi-Block Time Series

For data spanning multiple months, use `write_ts_multi` which automatically splits into monthly blocks:

```rust
let year_of_hourly = vec![0.0f64; 8760]; // 365 days * 24 hours
dss.write_ts_multi(
    "/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/SIM/",
    &year_of_hourly,
    "01JAN2020",  // start date
    3600,         // interval in seconds
    "CFS",
    "INST-VAL",
)?;
```

**Python:**
```python
dss.write_ts_multi("/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/SIM/",
                   np.zeros(8760), "01JAN2020", 3600, "CFS", "INST-VAL")
```

## Time Window Filtering

Read data across multiple blocks within a specific time window:

```rust
if let Some(ts) = dss.read_ts_window(
    "/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/SIM/",
    "15JAN2020",   // start date
    "15FEB2020",   // end date
)? {
    println!("Got {} values in window", ts.values.len());
}
```

**Python:**
```python
values = dss.read_ts_window("/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/SIM/",
                            "15JAN2020", "15FEB2020")
```

## Irregular Time Series

For data at non-uniform intervals:

```rust
let times = vec![60, 120, 300, 600];  // offsets in seconds from base date
let values = vec![10.0, 20.0, 15.0, 25.0];
dss.write_ts_irregular(
    "/BASIN/GAGE1/STAGE//IR-MONTH/OBS/",
    &times,
    &values,
    60,           // time_granularity_seconds (60 = minutes)
    "FT",
    "INST-VAL",
)?;
```

**Python:**
```python
dss.write_ts_irregular("/BASIN/GAGE1/STAGE//IR-MONTH/OBS/",
                       np.array([60, 120, 300, 600], dtype=np.int32),
                       np.array([10.0, 20.0, 15.0, 25.0]),
                       60, "FT", "INST-VAL")
```

## Query Without Reading Data

```rust
// Get sizes for pre-allocation
let (num_values, quality_size) = dss.ts_get_sizes("/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/OBS/")?;

// Get units and type without reading values
if let Some((units, dtype)) = dss.ts_retrieve_info("/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/OBS/")? {
    println!("Units: {units}, Type: {dtype}");
}

// Get date range
if let Some((first_j, _, last_j, _)) = dss.ts_get_date_time_range("/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/OBS/")? {
    println!("From Julian {first_j} to {last_j}");
}
```
