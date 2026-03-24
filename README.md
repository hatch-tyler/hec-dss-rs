# hec-dss-rs

Pure Rust implementation of the HEC-DSS (Data Storage System) version 7 file format.

Produces a **drop-in replacement DLL** for the C `hecdss` library. Existing consumers (Python, .NET, Fortran) work without modification. Files are fully compatible with HEC-RAS, HEC-HMS, HEC-DSSVue, and all DSS7 software.

## Architecture

```
┌─────────────┐  ┌──────────────┐  ┌───────────────┐  ┌────────────┐
│   Python     │  │   Fortran    │  │   .NET / C    │  │ dss-convert│
│ (hecdss_rs)  │  │ (hecdss_mod) │  │  (P/Invoke)   │  │  (CLI)     │
└──────┬───────┘  └──────┬───────┘  └───────┬───────┘  └─────┬──────┘
       │                 │                   │                │
       └────────────┬────┴───────────────────┘                │
                    │  hec_dss_* C ABI                        │
              ┌─────┴─────┐                             ┌─────┴─────┐
              │  dss-ffi   │  ← Drop-in DLL (200 KB)    │  dss-core  │
              └─────┬─────┘                             └─────┬─────┘
              ┌─────┴─────┐                                   │
              │  dss-core  │  ← Pure Rust DSS7 engine ────────┘
              └─────┬─────┘
              ┌─────┴─────┐
              │  DSS7 File │  ← Binary file format
              └───────────┘
```

## Crates

| Crate | Description |
|-------|-------------|
| **dss-core** | Pure Rust DSS7 reader/writer. All data types, compression, aliases, CRC. Zero C dependencies. |
| **dss-ffi** | C-compatible shared library. Exports `hec_dss_*` functions matching `hecdss.h`. Drop-in replacement. |
| **dss-python** | PyO3 native Python module (`hecdss_rs`). NumPy integration. Context manager. |
| **dss-fortran** | Fortran module (`hecdss_mod.f90`) with ISO_C_BINDING interfaces. |
| **dss-convert** | CLI tool for v6-to-v7 conversion and v7 file compaction. |
| **dss-sys** | Raw FFI bindings to the original C library (for testing/comparison only). |

## Supported Data Types

| Type | Read | Write | Description |
|------|------|-------|-------------|
| Text (300) | read_text | write_text | Text strings and notes |
| Regular TS (105) | read_ts, read_ts_window | write_ts, write_ts_multi | Regular-interval time series |
| Irregular TS (115) | read_ts | write_ts_irregular | Irregular-interval time series |
| Paired Data (205) | read_pd | write_pd | X-Y curve data |
| Array (90-93) | read_array | write_array | Int, float, double arrays |
| Location (20) | read_location | write_location | Coordinates and metadata |
| Grid (400-431) | read_grid | write_grid | Spatial grids with zlib compression |

## Operations

| Category | Methods |
|----------|---------|
| **File** | open, create, record_count, record_type, catalog, catalog_filtered |
| **Delete/Restore** | delete, undelete, squeeze |
| **Copy** | copy_record, copy_file |
| **Aliases** | alias_add, alias_remove, alias_list |
| **Integrity** | check_file, get_data_crc, snapshot_crcs, what_changed |
| **TS Info** | ts_get_sizes, ts_retrieve_info, ts_get_date_time_range |
| **PD Info** | pd_retrieve_info |
| **Dates** | date_to_julian, julian_to_ymd, parse_date |

## Quick Start

### Rust

```rust
use dss_core::NativeDssFile;

let mut dss = NativeDssFile::create("example.dss")?;

// Write time series
dss.write_ts("/BASIN/LOC/FLOW/01JAN2020/1HOUR/SIM/",
    &[100.0, 200.0, 300.0], "CFS", "INST-VAL")?;

// Write paired data
dss.write_pd("/BASIN/LOC/FREQ-FLOW///COMPUTED/",
    &[1.0, 10.0, 100.0], &[500.0, 5000.0, 50000.0],
    1, "PERCENT", "CFS", None)?;

// Write grid
dss.write_grid("/SHG/BASIN/PRECIP/01JAN2020/01JAN2020/SIM/",
    430, 100, 50, &grid_data, "MM", 2000.0)?;

// Catalog with wildcard filter
let entries = dss.catalog_filtered(Some("/BASIN/*/FLOW///*/"))?;

// Change tracking
let before = dss.snapshot_crcs()?;
// ... modify records ...
let after = dss.snapshot_crcs()?;
let (changed, added, removed) = NativeDssFile::what_changed(&before, &after);
```

### Python

```python
import hecdss_rs
import numpy as np

with hecdss_rs.DssFile.create("example.dss") as dss:
    # Time series with NumPy
    dss.write_ts("/A/B/FLOW/01JAN2020/1HOUR/SIM/",
                 np.array([100.0, 200.0, 300.0]), "CFS", "INST-VAL")

    # Catalog with wildcard
    entries = dss.catalog(filter="/*/*/FLOW///*/")

    # Date conversion
    j = hecdss_rs.DssFile.date_to_julian("15MAR2020")
    y, m, d = hecdss_rs.DssFile.julian_to_ymd(j)

    # Change detection
    before = dss.snapshot_crcs()
    dss.write_text("/A/B/NOTE///V/", "new data")
    after = dss.snapshot_crcs()
    changed, added, removed = hecdss_rs.DssFile.what_changed(before, after)
```

### Fortran

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

### v6 to v7 Conversion

```bash
dss-convert input_v6.dss output_v7.dss
```

## Building

```bash
# Build pure Rust crates (no C dependency)
cargo build -p dss-core -p dss-ffi --release

# Build Python wheel
cd crates/dss-python && maturin build --release

# Build with C library for cross-validation
HEC_DSS_DIR=/path/to/hec-dss cargo build --all-features --release
```

## Documentation

See [docs/user-guide.md](docs/user-guide.md) for complete tutorials and API reference.

## Tests

- **96 Rust tests** (unit, integration, cross-validation, property-based)
- **10 Fortran tests** (Intel ifx 2025.3)
- **Python integration** verified across all data types
- **CI:** Ubuntu, Windows, macOS + Python wheels

## Comparison with C Library

| | C `hecdss.dll` | Rust `dss_ffi.dll` |
|---|---|---|
| Size | 705 KB | **200 KB** |
| Dependencies | zlib, 432 C files | **None** |
| Thread safety | Global mutable state | **Mutex per handle** |
| Memory safety | Manual (bugs found & fixed) | **Guaranteed** |
| Buffer overflows | 8 HIGH findings | **Impossible** |
| Data types | 6 | **7** (+ arrays) |
| Operations | 40 FFI functions | **40 + aliases, CRC, copy, check** |

## License

MIT
