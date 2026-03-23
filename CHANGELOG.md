# Changelog

## 0.1.0 (2026-03-23)

### Initial Release

**dss-core** - Pure Rust DSS7 engine:
- `NativeDssFile`: open, create, catalog, record_count
- Text records: read_text, write_text
- Time series: read_ts, write_ts (regular, double precision)
- Paired data: read_pd, write_pd (double precision)
- File format: hash algorithm, header reading, bin traversal, record info parsing
- Bin block overflow (automatic allocation of new blocks)
- Pathname validation (format, length, null bytes)
- Input validation on all write operations
- Cross-validated with C library in both directions

**dss-ffi** - Drop-in DLL replacement:
- All `hec_dss_*` functions matching `hecdss.h`
- Thread-safe (Mutex per handle)
- No panic paths (all errors return status codes)
- 200 KB (vs 705 KB for C version)

**dss-python** - PyO3 native Python module:
- `hecdss_rs.DssFile` with context manager
- NumPy array support for time series and paired data
- Zero C dependency

**dss-fortran** - Fortran interop:
- `hecdss_mod.f90` with ISO_C_BINDING interfaces
- Verified with Intel ifx 2025.3 (10/10 tests pass)

**dss-sys** - Raw FFI bindings to C library (for testing)

### Testing
- 62 Rust tests (unit + integration + cross-validation + property-based)
- 10 Fortran tests
- Python integration verified
- Cross-language interop: Rust <-> C, Rust <-> Python, Fortran -> Rust -> Python
