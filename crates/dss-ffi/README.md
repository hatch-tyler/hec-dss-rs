# dss-ffi

C-compatible FFI layer for HEC-DSS. Drop-in replacement for `hecdss.dll` / `libhecdss.so`.

All 40 `hec_dss_*` functions from `hecdss.h` are implemented. 200 KB DLL with zero C dependencies.

## Building

```bash
cargo build -p dss-ffi --release
# Output: target/release/dss_ffi.dll (Windows) or libdss_ffi.so (Linux)
```

## Usage

Replace your existing `hecdss.dll` with the built `dss_ffi.dll`. Existing C, .NET, Fortran, and Python consumers work without modification.

## Thread Safety

All functions are thread-safe via a per-handle mutex. Multiple threads can safely call any `hec_dss_*` function concurrently.

## Documentation

Full documentation at [hatch-tyler.github.io/hec-dss-rs](https://hatch-tyler.github.io/hec-dss-rs/)
