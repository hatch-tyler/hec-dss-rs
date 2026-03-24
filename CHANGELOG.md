# Changelog

## 0.1.0 (2026-03-24)

### Initial Release

#### dss-core — Pure Rust DSS7 Engine (37 public methods)

**File Operations:** open, create, record_count, record_type, catalog, catalog_filtered

**Text Records:** read_text, write_text

**Time Series:** read_ts, write_ts, write_ts_irregular, read_ts_window, write_ts_multi, ts_get_sizes, ts_retrieve_info, ts_get_date_time_range

**Paired Data:** read_pd, write_pd, pd_retrieve_info

**Array Records:** read_array, write_array

**Location Data:** read_location, write_location

**Grid/Spatial:** read_grid, write_grid (with zlib compression)

**Record Management:** delete, undelete, squeeze, copy_record, copy_file, check_file

**Aliases:** alias_add, alias_remove, alias_list

**CRC/Change Tracking:** get_data_crc, snapshot_crcs, what_changed

**Date/Time:** date_to_julian, julian_to_ymd, parse_date, parse_interval, generate_block_starts, values_in_block

**V6 Support:** detect_version, read_v6_header, read_v6_records, scan_v7_records (brute-force)

#### dss-ffi — Drop-in C DLL Replacement (40/40 hecdss.h functions)

All `hec_dss_*` functions implemented. 200 KB DLL, zero C dependencies. Thread-safe via mutex per handle.

#### dss-python — PyO3 Native Python Module (35+ methods)

All NativeDssFile operations available from Python with NumPy array support, context manager, static date utility methods.

#### dss-fortran — Fortran Interop

ISO_C_BINDING module with interfaces for all `hec_dss_*` functions. Verified with Intel ifx 2025.3 (10/10 tests).

#### dss-convert — CLI Conversion Tool

v6-to-v7 conversion and v7 file compaction.

#### dss-sys — C Library FFI Bindings

Raw unsafe bindings to the original C `hecdss` library. Used for cross-validation testing only.

### Testing

- 96 Rust tests (unit + integration + cross-validation + property-based)
- 10 Fortran tests
- Python integration verified
- CI: Ubuntu, Windows, macOS + Python wheels (all green)
