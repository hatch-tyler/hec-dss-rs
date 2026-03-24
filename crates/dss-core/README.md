# dss-core

Pure Rust implementation of the HEC-DSS (Data Storage System) version 7 binary file format.

No C library, no Fortran, no external dependencies beyond `thiserror`, `fs2`, and `flate2`.

## Features

- **All data types**: text, regular/irregular time series, paired data, arrays, location, grids (with zlib compression)
- **Full record management**: read, write, delete, undelete, squeeze, copy
- **Aliases**: create, remove, list path aliases
- **CRC change tracking**: snapshot and diff record checksums
- **Catalog with wildcards**: filter records using `/*/*/FLOW///*/` patterns
- **Date/time utilities**: Julian dates, interval mapping, block boundaries
- **V6 detection**: detect and read DSS version 6 file headers
- **Thread-safe**: file locking via `fs2`

## Quick Start

```rust
use dss_core::NativeDssFile;

// Create a new DSS7 file
let mut dss = NativeDssFile::create("example.dss").unwrap();

// Write text
dss.write_text("/A/B/NOTE///V/", "Hello from Rust!").unwrap();

// Write time series
let values = vec![100.0, 200.0, 300.0];
dss.write_ts("/A/B/FLOW/01JAN2020/1HOUR/SIM/", &values, "CFS", "INST-VAL").unwrap();

// Read it back
let ts = dss.read_ts("/A/B/FLOW/01JAN2020/1HOUR/SIM/").unwrap();
println!("Values: {:?}, Units: {}", ts.values, ts.units);

// Catalog
let records = dss.catalog(None).unwrap();
```

## Compatibility

Files produced by `dss-core` are fully compatible with HEC-RAS, HEC-HMS, HEC-DSSVue, and all DSS7 software.

## Documentation

Full documentation at [hatch-tyler.github.io/hec-dss-rs](https://hatch-tyler.github.io/hec-dss-rs/)
