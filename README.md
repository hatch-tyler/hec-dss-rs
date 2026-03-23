# hec-dss-rs

Pure Rust implementation of the HEC-DSS (Data Storage System) version 7 file format.

Produces a **drop-in replacement DLL** for the C `hecdss` library — existing consumers (Python, .NET, Fortran) work without modification. Files are fully compatible with HEC-RAS, HEC-HMS, HEC-DSSVue, and all other DSS7 software.

## Architecture

```
┌─────────────┐  ┌──────────────┐  ┌───────────────┐
│   Python     │  │   Fortran    │  │   .NET / C    │
│  (hecdss)    │  │ (hecdss_mod) │  │  (P/Invoke)   │
└──────┬───────┘  └──────┬───────┘  └───────┬───────┘
       │                 │                   │
       └────────────┬────┴───────────────────┘
                    │  hec_dss_* C ABI
              ┌─────┴─────┐
              │  dss-ffi   │  ← Drop-in DLL (200 KB)
              └─────┬─────┘
              ┌─────┴─────┐
              │  dss-core  │  ← Pure Rust DSS7 engine
              └─────┬─────┘
              ┌─────┴─────┐
              │  DSS7 File │  ← Binary file format
              └───────────┘
```

## Crates

| Crate | Description |
|-------|-------------|
| **dss-core** | Pure Rust DSS7 reader/writer. Hash table, bins, record I/O, text, time series, paired data. Zero C dependencies. |
| **dss-ffi** | C-compatible shared library (`dss_ffi.dll`). Exposes `hec_dss_*` functions matching `hecdss.h`. |
| **dss-sys** | Raw FFI bindings to the original C `hecdss` library (for testing/comparison). |
| **dss-fortran** | Fortran module (`hecdss_mod.f90`) with ISO_C_BINDING interfaces. |

## Quick Start

### Rust

```rust
use dss_core::NativeDssFile;

// Create a new DSS file
let mut dss = NativeDssFile::create("example.dss")?;

// Write text
dss.write_text("/PROJECT/LOC/NOTE///VER/", "Hello from Rust")?;

// Write time series
dss.write_ts(
    "/BASIN/LOC/FLOW/01JAN2020/1HOUR/SIM/",
    &[100.0, 200.0, 300.0],
    "CFS", "INST-VAL",
)?;

// Write paired data
dss.write_pd(
    "/BASIN/LOC/FREQ-FLOW///COMPUTED/",
    &[1.0, 10.0, 100.0],       // ordinates
    &[500.0, 5000.0, 50000.0],  // values
    1, "PERCENT", "CFS", None,
)?;

// Read back
let text = dss.read_text("/PROJECT/LOC/NOTE///VER/")?.unwrap();
let ts = dss.read_ts("/BASIN/LOC/FLOW/01JAN2020/1HOUR/SIM/")?.unwrap();
let pd = dss.read_pd("/BASIN/LOC/FREQ-FLOW///COMPUTED/")?.unwrap();

// Catalog
for entry in dss.catalog()? {
    println!("{} [type={}]", entry.pathname, entry.record_type);
}
```

### Python (drop-in DLL replacement)

```bash
HECDSS_LIBRARY=path/to/dss_ffi.dll pip install hecdss
```

```python
from hecdss import DssFile
import numpy as np

with DssFile("example.dss") as dss:
    dss.write_ts("/BASIN/LOC/FLOW/01JAN2020/1HOUR/SIM/",
                 np.array([100.0, 200.0, 300.0]),
                 start_date="01JAN2020", start_time="01:00",
                 units="CFS", data_type="INST-VAL")
    df = dss.read_ts("/BASIN/LOC/FLOW/01JAN2020/1HOUR/SIM/",
                     start_date="01JAN2020", start_time="01:00",
                     end_date="01JAN2020", end_time="03:00").to_dataframe()
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

## Building

```bash
# Build all crates
cargo build --release

# Run tests (requires C hecdss.dll for cross-validation)
export HEC_DSS_DIR=/path/to/hec-dss   # C library repo with build/
cargo test

# Build only the pure Rust crates (no C dependency)
cargo build -p dss-core -p dss-ffi --release
```

The release DLL is at `target/release/dss_ffi.dll` (Windows) or `target/release/libdss_ffi.so` (Linux).

## Test Results

**54 Rust tests** covering:
- File format: hash algorithm, pathname parsing, header reading, word I/O
- Cross-validation: C writes → Rust reads, Rust writes → C reads
- NativeDssFile: text, time series, paired data round-trips
- FFI: open/close, catalog, text, TS via C-backed DssFile

**10 Fortran tests** (Intel ifx 2025.3):
- File open/close/version, text round-trip, TS store, record counting

**Python integration** verified:
- All 94 Python tests pass with the Rust DLL as drop-in replacement
- Fortran-created files readable by Python via Rust DLL

## Comparison with C Library

| | C `hecdss.dll` | Rust `dss_ffi.dll` |
|---|---|---|
| Size | 705 KB | **200 KB** |
| Dependencies | zlib, 432 C files | **None** |
| Thread safety | Global mutable state | **Mutex per handle** |
| Memory safety | Manual (bugs found & fixed) | **Guaranteed** |
| Buffer overflows | 5 HIGH findings | **Impossible** |
| Languages | C, JNI | **Rust, C ABI, Python, Fortran** |

## License

MIT
