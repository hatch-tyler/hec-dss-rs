# Change Tracking

CRC-based change detection enables tracking which records have been modified, added, or removed between two points in time.

## Computing CRC for a Single Record

```rust
let crc = dss.get_data_crc("/BASIN/LOC/FLOW/01JAN2020/1HOUR/OBS/")?;
println!("CRC32: {crc}");
```

## Snapshot All Records

Take a snapshot of CRC values for all records:

```rust
let snapshot = dss.snapshot_crcs()?;
for (pathname, crc) in &snapshot {
    println!("{pathname}: {crc}");
}
```

## Detecting Changes

Compare two snapshots to find what changed:

```rust
let before = dss.snapshot_crcs()?;

// ... modify records ...
dss.write_text("/A/B/NOTE///NEW/", "added later")?;

let after = dss.snapshot_crcs()?;

let (changed, added, removed) = NativeDssFile::what_changed(&before, &after);

println!("Changed: {changed:?}");
println!("Added: {added:?}");
println!("Removed: {removed:?}");
```

**Python:**
```python
before = dss.snapshot_crcs()

dss.write_text("/A/B/NOTE///NEW/", "added later")

after = dss.snapshot_crcs()
changed, added, removed = hecdss_rs.DssFile.what_changed(before, after)
print(f"Added: {added}")
```

## Use Cases

- **Data synchronization**: Detect which records changed since last sync
- **Audit trail**: Track modifications to a DSS file over time
- **Incremental backup**: Only copy changed records to backup
