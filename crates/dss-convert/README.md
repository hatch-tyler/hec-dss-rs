# dss-convert

CLI tool for DSS file conversion and compaction.

## Usage

```bash
dss-convert <input.dss> <output.dss>
```

## Features

- **v6 to v7 conversion**: Reads DSS version 6 files (via C library bridge) and writes version 7 using pure Rust
- **v7 compaction**: Copies live records to a clean v7 file, skipping deleted records (equivalent to squeeze)
- **Format detection**: Automatically detects input file version

## Examples

```bash
# Convert v6 to v7
dss-convert old_v6_data.dss new_v7_data.dss

# Compact a v7 file
dss-convert bloated.dss clean.dss
```

## Building

```bash
# Requires C library for v6 reading
HEC_DSS_DIR=/path/to/hec-dss cargo build -p dss-convert --release
```

## Limitations

- v6 reading requires the C library (with Fortran) at runtime
- Only text and time series records are currently converted from v6
- The pure Rust v6 reader can detect headers but full catalog requires the legacy code
