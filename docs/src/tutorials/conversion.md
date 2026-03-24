# v6 to v7 Conversion

## Using the CLI Tool

```bash
dss-convert input_v6.dss output_v7.dss
```

The tool:
1. Detects the input file version (v6 or v7)
2. Reads all records from the source
3. Writes each record to a new v7 file using pure Rust
4. Reports conversion statistics

```
Input: DSS version 6 file
Reading from: input_v6.dss
Writing to:   output_v7.dss
Records in source: 3

Conversion complete:
  Copied:  3
  Skipped: 0
  Output:  output_v7.dss
  Output records: 3
```

## File Version Detection

### Rust

```rust
use dss_core::format::v6;

let mut file = std::fs::File::open("unknown.dss")?;
let version = v6::detect_version(&mut file)?;
match version {
    6 => println!("DSS version 6"),
    7 => println!("DSS version 7"),
    0 => println!("Not a DSS file"),
    _ => println!("Unknown version"),
}
```

### Python

```python
# The DssFile.open() method handles both v6 and v7 automatically
# For explicit version checking, examine the file header
```

## v6 vs v7 Differences

| Feature | DSS v6 | DSS v7 |
|---------|--------|--------|
| Addressing | 32-bit (4GB limit) | 64-bit (unlimited) |
| Missing value | -901.0 | -3.4028235e+38 |
| Bin format | Fortran COMMON blocks | C structs |
| Info flag | None | -97534 |
| Compression | Limited | zlib |
| Max pathname | ~128 chars | 393 chars |

## v7-to-v7 Compaction

`dss-convert` also works as a compactor for v7 files, equivalent to the squeeze operation:

```bash
dss-convert bloated_v7.dss clean_v7.dss
```

This copies all live records to a new file, skipping deleted records and reclaiming wasted space.

## Limitations

- v6 reading requires the C library (with Fortran) as a bridge. The pure Rust v6 reader can detect and parse headers but full catalog scanning requires the legacy Fortran code paths.
- Some v6 record types (images, binary files) may not convert.
- v6 files with non-standard extensions may not be detected.
