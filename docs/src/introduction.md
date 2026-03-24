# HEC-DSS Rust Library

A pure Rust implementation of the HEC-DSS (Data Storage System) version 7 file format used by the U.S. Army Corps of Engineers for hydrologic and hydraulic data.

## What is HEC-DSS?

HEC-DSS is a binary file format for storing time series, paired data, spatial grids, and other hydrologic data. It is used by USACE software including HEC-RAS, HEC-HMS, HEC-ResSim, and HEC-DSSVue.

## Why Rust?

The original HEC-DSS library is written in C with Fortran components. This Rust implementation provides:

- **Memory safety** - no buffer overflows, null dereferences, or use-after-free
- **Thread safety** - each file handle is protected by a mutex
- **Zero dependencies** - no C compiler, zlib, or Fortran runtime needed
- **Cross-platform** - builds on Windows, Linux, and macOS
- **Small footprint** - 200 KB DLL vs 705 KB for the C version
- **Multiple language bindings** - Rust, Python (PyO3), C (FFI), Fortran

## Features

- Read and write all DSS7 data types (text, time series, paired data, arrays, location, grids)
- Multi-block time series with time window filtering
- Record management (delete, undelete, squeeze, copy)
- Alias system for record aliasing
- CRC-based change detection
- File integrity checking
- zlib compression for grid data
- DSS v6 to v7 conversion (via CLI tool)
- Wildcard catalog filtering
- 40/40 hecdss.h FFI functions implemented (100% drop-in compatible)
