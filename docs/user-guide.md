# HEC-DSS Rust Library User Guide

## Overview

`hec-dss-rs` is a pure Rust implementation of the HEC-DSS (Data Storage System) version 7 file format. It reads and writes `.dss` files that are fully compatible with HEC-RAS, HEC-HMS, HEC-DSSVue, and all other DSS7 software.

## DSS Concepts

### Pathnames

Every record in a DSS file is identified by a **pathname** with six parts:

```
/A-Part/B-Part/C-Part/D-Part/E-Part/F-Part/
```

| Part | Name | Example |
|------|------|---------|
| A | Project/Basin | `SACRAMENTO` |
| B | Location | `FOLSOM DAM` |
| C | Parameter | `FLOW`, `STAGE`, `PRECIP` |
| D | Date | `01JAN2020` |
| E | Interval | `1HOUR`, `1DAY`, `IR-MONTH` |
| F | Version | `OBS`, `SIM`, `COMPUTED` |

Maximum pathname length: 393 characters.

### Record Types

| Code | Name | Description |
|------|------|-------------|
| 100 | RTS | Regular time series (floats) |
| 105 | RTD | Regular time series (doubles) |
| 110 | ITS | Irregular time series (floats) |
| 115 | ITD | Irregular time series (doubles) |
| 200 | PD | Paired data (floats) |
| 205 | PDD | Paired data (doubles) |
| 300 | TXT | Text record |

## Rust API

### Opening Files

```rust
use dss_core::NativeDssFile;

// Open existing file
let mut dss = NativeDssFile::open("data.dss")?;

// Create new file
let mut dss = NativeDssFile::create("new.dss")?;
```

Files are automatically closed when dropped.

### Catalog

```rust
let entries = dss.catalog()?;
for entry in &entries {
    println!("{} [type={}]", entry.pathname, entry.record_type);
}
println!("Total records: {}", dss.record_count());
```

### Text Records

```rust
// Write
dss.write_text("/PROJECT/LOC/NOTE///VER/", "Description text")?;

// Read (returns None if not found)
if let Some(text) = dss.read_text("/PROJECT/LOC/NOTE///VER/")? {
    println!("Text: {text}");
}
```

### Time Series

```rust
// Write regular time series (doubles)
let values = vec![100.0, 200.0, 300.0, 400.0, 500.0];
dss.write_ts(
    "/BASIN/LOC/FLOW/01JAN2020/1HOUR/SIM/",
    &values,
    "CFS",       // units
    "INST-VAL",  // data type
)?;

// Read
if let Some(ts) = dss.read_ts("/BASIN/LOC/FLOW/01JAN2020/1HOUR/SIM/")? {
    println!("Values: {:?}", ts.values);
    println!("Units: {}", ts.units);
    println!("Record type: {}", ts.record_type);
}
```

### Paired Data

```rust
// Write (ordinates + values for 1 curve)
dss.write_pd(
    "/BASIN/LOC/FREQ-FLOW///COMPUTED/",
    &[1.0, 5.0, 10.0, 50.0, 100.0],     // ordinates
    &[500.0, 1000.0, 2000.0, 5000.0, 10000.0], // values
    1,                                      // number of curves
    "PERCENT",                              // units for ordinates
    "CFS",                                  // units for values
    None,                                   // labels
)?;

// Read
if let Some(pd) = dss.read_pd("/BASIN/LOC/FREQ-FLOW///COMPUTED/")? {
    println!("Ordinates: {:?}", pd.ordinates);
    println!("Values: {:?}", pd.values);
    println!("{} ordinates, {} curves", pd.number_ordinates, pd.number_curves);
}
```

### Error Handling

All operations return `io::Result`. Errors include descriptive messages:

```rust
match dss.write_text("/bad pathname", "text") {
    Ok(()) => println!("Success"),
    Err(e) => eprintln!("Error: {e}"),
    // Output: "Error: Pathname must start and end with '/'"
}
```

## Python API (PyO3)

Install the wheel built by `maturin`:

```bash
pip install target/wheels/dss_python-*.whl
```

```python
import hecdss_rs
import numpy as np

# Create and write
with hecdss_rs.DssFile.create("example.dss") as dss:
    dss.write_text("/A/B/NOTE///V/", "Hello from Python!")
    dss.write_ts("/A/B/FLOW/01JAN2020/1HOUR/SIM/",
                 np.array([10.0, 20.0, 30.0]), "CFS", "INST-VAL")
    print(dss.catalog())  # [(pathname, record_type), ...]

# Read
with hecdss_rs.DssFile.open("example.dss") as dss:
    text = dss.read_text("/A/B/NOTE///V/")
    values = dss.read_ts("/A/B/FLOW/01JAN2020/1HOUR/SIM/")  # numpy array
```

## Fortran API

Link against `dss_ffi.dll` and use the `hecdss` module:

```fortran
use hecdss
use iso_c_binding

type(c_ptr) :: dss
integer(c_int) :: status

status = hec_dss_open("example.dss"//c_null_char, dss)
status = hec_dss_textStore(dss, "/A/B/NOTE///F/"//c_null_char, &
    "Hello from Fortran"//c_null_char, 18)
status = hec_dss_close(dss)
```

Note: Fortran strings must be null-terminated with `//c_null_char`.

## C FFI API

The `dss_ffi.dll` is a drop-in replacement for the C `hecdss.dll`:

```c
#include <stdio.h>

typedef struct dss_file dss_file;
extern int hec_dss_open(const char* filename, dss_file** dss);
extern int hec_dss_close(dss_file* dss);
extern int hec_dss_textStore(dss_file* dss, const char* pathname,
    const char* text, int length);

int main() {
    dss_file* dss = NULL;
    hec_dss_open("example.dss", &dss);
    hec_dss_textStore(dss, "/A/B/NOTE///C/", "Hello from C", 12);
    hec_dss_close(dss);
    return 0;
}
```

## Performance Tips

- **Batch writes**: Open the file once, write all records, then close.
- **Catalog first**: Call `catalog()` once and cache results rather than
  calling `read_text`/`read_ts` with pathnames that may not exist.
- **Reuse files**: Opening a DSS file reads the header; reuse the handle
  for multiple operations.
