# Rust API Reference

## NativeDssFile

The main entry point for all DSS operations. Pure Rust, zero C dependencies.

### File Operations

| Method | Signature | Description |
|--------|-----------|-------------|
| `create` | `fn create(path: &str) -> io::Result<Self>` | Create a new empty DSS7 file |
| `open` | `fn open(path: &str) -> io::Result<Self>` | Open an existing DSS7 file |
| `record_count` | `fn record_count(&self) -> i64` | Number of records (including aliases) |
| `record_type` | `fn record_type(&mut self, pathname: &str) -> io::Result<i32>` | Get data type code (0 if not found) |
| `catalog` | `fn catalog(&mut self) -> io::Result<Vec<CatalogEntry>>` | List all records |
| `catalog_filtered` | `fn catalog_filtered(&mut self, filter: Option<&str>) -> io::Result<Vec<CatalogEntry>>` | List with wildcard filter |

### Text Records

| Method | Signature | Description |
|--------|-----------|-------------|
| `read_text` | `fn read_text(&mut self, pathname: &str) -> io::Result<Option<String>>` | Read text (None if not found) |
| `write_text` | `fn write_text(&mut self, pathname: &str, text: &str) -> io::Result<()>` | Write text record |

### Time Series

| Method | Signature | Description |
|--------|-----------|-------------|
| `read_ts` | `fn read_ts(&mut self, pathname: &str) -> io::Result<Option<TimeSeriesRecord>>` | Read single-block TS |
| `write_ts` | `fn write_ts(&mut self, pathname: &str, values: &[f64], units: &str, data_type: &str) -> io::Result<()>` | Write regular TS (doubles) |
| `write_ts_irregular` | `fn write_ts_irregular(&mut self, pathname: &str, times: &[i32], values: &[f64], granularity: i32, units: &str, data_type: &str) -> io::Result<()>` | Write irregular TS |
| `read_ts_window` | `fn read_ts_window(&mut self, pathname: &str, start_date: &str, end_date: &str) -> io::Result<Option<TimeSeriesRecord>>` | Read across blocks with date filter |
| `write_ts_multi` | `fn write_ts_multi(&mut self, pathname: &str, values: &[f64], start_date: &str, interval_sec: i32, units: &str, data_type: &str) -> io::Result<()>` | Write multi-block TS |
| `ts_get_sizes` | `fn ts_get_sizes(&mut self, pathname: &str) -> io::Result<(i32, i32)>` | Get (num_values, quality_size) |
| `ts_retrieve_info` | `fn ts_retrieve_info(&mut self, pathname: &str) -> io::Result<Option<(String, String)>>` | Get (units, type) without data |
| `ts_get_date_time_range` | `fn ts_get_date_time_range(&mut self, pathname: &str) -> io::Result<Option<(i32,i32,i32,i32)>>` | Get first/last Julian dates |

### Paired Data

| Method | Signature | Description |
|--------|-----------|-------------|
| `read_pd` | `fn read_pd(&mut self, pathname: &str) -> io::Result<Option<PairedDataRecord>>` | Read paired data |
| `write_pd` | `fn write_pd(&mut self, ..., n_curves: usize, ...) -> io::Result<()>` | Write paired data (doubles) |
| `pd_retrieve_info` | `fn pd_retrieve_info(&mut self, pathname: &str) -> io::Result<Option<(i32,i32,String,String)>>` | Get (n_ord, n_curves, units_i, units_d) |

### Array

| Method | Signature | Description |
|--------|-----------|-------------|
| `read_array` | `fn read_array(&mut self, pathname: &str) -> io::Result<Option<ArrayRecord>>` | Read int/float/double arrays |
| `write_array` | `fn write_array(&mut self, pathname: &str, ints: &[i32], floats: &[f32], doubles: &[f64]) -> io::Result<()>` | Write array record |

### Location

| Method | Signature | Description |
|--------|-----------|-------------|
| `read_location` | `fn read_location(&mut self, pathname: &str) -> io::Result<Option<LocationRecord>>` | Read coordinates |
| `write_location` | `fn write_location(&mut self, pathname: &str, loc: &LocationRecord) -> io::Result<()>` | Write coordinates |

### Grid / Spatial

| Method | Signature | Description |
|--------|-----------|-------------|
| `read_grid` | `fn read_grid(&mut self, pathname: &str) -> io::Result<Option<GridRecord>>` | Read grid (auto-decompresses) |
| `write_grid` | `fn write_grid(&mut self, ..., nx: i32, ny: i32, data: &[f32], ...) -> io::Result<()>` | Write grid (auto-compresses) |

### Record Management

| Method | Signature | Description |
|--------|-----------|-------------|
| `delete` | `fn delete(&mut self, pathname: &str) -> io::Result<()>` | Mark record as deleted |
| `undelete` | `fn undelete(&mut self, pathname: &str) -> io::Result<()>` | Restore deleted record |
| `squeeze` | `fn squeeze(&mut self) -> io::Result<()>` | Compact file (reclaim space) |
| `copy_record` | `fn copy_record(&mut self, pathname: &str, dest: &mut NativeDssFile) -> io::Result<bool>` | Copy one record to another file |
| `copy_file` | `fn copy_file(&mut self, dest: &mut NativeDssFile) -> io::Result<usize>` | Copy all records |
| `check_file` | `fn check_file(&mut self) -> io::Result<Vec<String>>` | Validate file integrity |

### Aliases

| Method | Signature | Description |
|--------|-----------|-------------|
| `alias_add` | `fn alias_add(&mut self, primary: &str, alias: &str) -> io::Result<()>` | Create alias to primary |
| `alias_remove` | `fn alias_remove(&mut self, alias: &str) -> io::Result<()>` | Remove alias |
| `alias_list` | `fn alias_list(&mut self) -> io::Result<Vec<(String, i64)>>` | List all aliases |

### CRC / Change Tracking

| Method | Signature | Description |
|--------|-----------|-------------|
| `get_data_crc` | `fn get_data_crc(&mut self, pathname: &str) -> io::Result<u32>` | CRC32 of record data |
| `snapshot_crcs` | `fn snapshot_crcs(&mut self) -> io::Result<Vec<(String, u32)>>` | Snapshot all CRCs |
| `what_changed` | `fn what_changed(before: &[(String,u32)], after: &[(String,u32)]) -> (Vec<String>, Vec<String>, Vec<String>)` | Compare snapshots (static) |

## Data Types

### TimeSeriesRecord

```rust
pub struct TimeSeriesRecord {
    pub pathname: String,
    pub values: Vec<f64>,
    pub times: Vec<i32>,
    pub quality: Option<Vec<i32>>,
    pub units: String,
    pub data_type_str: String,
    pub record_type: i32,
    pub time_granularity: i32,
    pub precision: i32,
    pub block_start: i32,
    pub block_end: i32,
    pub number_values: usize,
}
```

### PairedDataRecord

```rust
pub struct PairedDataRecord {
    pub pathname: String,
    pub ordinates: Vec<f64>,
    pub values: Vec<f64>,
    pub number_ordinates: usize,
    pub number_curves: usize,
    pub units_independent: String,
    pub units_dependent: String,
    pub labels: Vec<String>,
    pub record_type: i32,
}
```

### ArrayRecord, LocationRecord, GridRecord

See source code for complete field listings.

## Error Handling

All methods return `io::Result<T>`. Errors include:
- File I/O errors (not found, permission denied)
- Invalid pathname format
- Empty data arrays
- Corrupt file structure
- Record not found (for delete/undelete)
