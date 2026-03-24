# dss-sys

Raw FFI bindings to the HEC-DSS (hecdss) C shared library.

Used for cross-validation testing of the pure Rust implementation. Most users should use `dss-core` instead.

## Requirements

Set `HEC_DSS_DIR` to point to a built copy of the [HEC-DSS C library](https://github.com/HydrologicEngineeringCenter/hec-dss):

```bash
HEC_DSS_DIR=/path/to/hec-dss cargo build -p dss-sys
```

Optionally set `HEC_DSS_LIB_DIR` to override the library search path.
