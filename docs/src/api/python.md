# Python API Reference

Module: `hecdss_rs`

Install: `pip install target/wheels/dss_python-*.whl`

## DssFile Class

### File Operations

```python
# Create / Open
dss = hecdss_rs.DssFile.create("new.dss")
dss = hecdss_rs.DssFile.open("existing.dss")

# Context manager (auto-closes)
with hecdss_rs.DssFile.create("example.dss") as dss:
    ...

dss.close()                      # Explicit close (safe to call multiple times)
dss.record_count()               # -> int
dss.record_type(pathname)        # -> int (0 if not found)
dss.catalog()                    # -> list of (pathname, record_type) tuples
dss.catalog(filter="/*/*/FLOW///*/")  # With wildcard
```

### Text Records

```python
dss.write_text(pathname, text)   # text: str
dss.read_text(pathname)          # -> str or None
```

### Time Series

```python
# Regular TS
dss.write_ts(pathname, values, units, data_type)  # values: numpy array
dss.read_ts(pathname)            # -> numpy array or None

# Irregular TS
dss.write_ts_irregular(pathname, times, values, granularity, units, data_type)
# times: numpy int32 array, values: numpy float64 array

# Multi-block
dss.write_ts_multi(pathname, values, start_date, interval_seconds, units, data_type)
dss.read_ts_window(pathname, start_date, end_date)  # -> numpy array or None

# Info queries
dss.ts_get_sizes(pathname)                # -> (num_values, quality_size)
dss.ts_retrieve_info(pathname)            # -> (units, type) or None
dss.ts_get_date_time_range(pathname)      # -> (fj, fs, lj, ls) or None
```

### Paired Data

```python
dss.write_pd(pathname, ordinates, values, n_curves, units_indep, units_dep)
# ordinates, values: numpy float64 arrays

dss.read_pd(pathname)            # -> (ordinates, values) numpy arrays or None
dss.pd_retrieve_info(pathname)   # -> (n_ord, n_curves, units_i, units_d) or None
```

### Array Records

```python
dss.write_array(pathname, int_values=[], float_values=[], double_values=[])
dss.read_array(pathname)         # -> dict with 'int_values', 'float_values', 'double_values'
```

### Location

```python
dss.write_location(pathname, x=0.0, y=0.0, z=0.0,
                   coordinate_system=0, horizontal_datum=0, vertical_datum=0,
                   timezone="", supplemental="")
dss.read_location(pathname)      # -> dict or None
```

### Grid / Spatial

```python
dss.write_grid(pathname, grid_type, nx, ny, data, data_units="", cell_size=0.0)
# data: list of float (flat, ny*nx)
dss.read_grid(pathname)          # -> dict with nx, ny, data, etc. or None
```

### Record Management

```python
dss.delete(pathname)
dss.undelete(pathname)
dss.squeeze()
dss.copy_record(pathname, dest_dss)   # dest_dss: another DssFile
dss.copy_file(dest_dss)               # -> int (count copied)
dss.check_file()                      # -> list of str
```

### Aliases

```python
dss.alias_add(primary_pathname, alias_pathname)
dss.alias_remove(alias_pathname)
dss.alias_list()                 # -> list of (pathname, info_address)
```

### CRC / Change Tracking

```python
dss.get_data_crc(pathname)       # -> int (CRC32)
dss.snapshot_crcs()              # -> list of (pathname, crc)

# Static method:
changed, added, removed = hecdss_rs.DssFile.what_changed(before, after)
```

### Date Utilities (Static Methods)

```python
hecdss_rs.DssFile.date_to_julian("15MAR2020")     # -> int
hecdss_rs.DssFile.julian_to_ymd(43905)             # -> (2020, 3, 15)
hecdss_rs.DssFile.parse_date("2020-03-15")         # -> (2020, 3, 15) or None
```

## NumPy Integration

Time series values are passed as `numpy.ndarray` (float64). Arrays are returned as numpy arrays.

```python
import numpy as np

# Write
values = np.array([1.0, 2.0, 3.0, 4.0, 5.0])
dss.write_ts("/A/B/FLOW/01JAN2020/1HOUR/SIM/", values, "CFS", "INST-VAL")

# Read
result = dss.read_ts("/A/B/FLOW/01JAN2020/1HOUR/SIM/")
# result is a numpy array
print(result.mean(), result.max())
```
