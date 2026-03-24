# File Management

## Delete and Restore Records

```rust
// Delete a record (marks as deleted, space not reclaimed)
dss.delete("/OLD/PATH/DATA///REMOVE/")?;

// Restore a deleted record
dss.undelete("/OLD/PATH/DATA///REMOVE/")?;
```

**Python:**
```python
dss.delete("/OLD/PATH/DATA///REMOVE/")
dss.undelete("/OLD/PATH/DATA///REMOVE/")
```

## Squeeze (Compact)

Reclaim space from deleted records by copying all live records to a new file:

```rust
dss.squeeze()?;
```

**Python:**
```python
dss.squeeze()
```

## Copy Records Between Files

```rust
let mut src = NativeDssFile::open("source.dss")?;
let mut dst = NativeDssFile::create("destination.dss")?;

// Copy a single record
src.copy_record("/BASIN/LOC/FLOW/01JAN2020/1HOUR/OBS/", &mut dst)?;

// Copy all records
let count = src.copy_file(&mut dst)?;
println!("Copied {count} records");
```

**Python:**
```python
src = hecdss_rs.DssFile.open("source.dss")
dst = hecdss_rs.DssFile.create("destination.dss")
src.copy_record("/BASIN/LOC/FLOW/01JAN2020/1HOUR/OBS/", dst)
count = src.copy_file(dst)
```

## File Integrity Check

Validate the DSS file structure:

```rust
let issues = dss.check_file()?;
for issue in &issues {
    println!("{issue}");
}
// Last entry is "File integrity OK" if no problems found
```

**Python:**
```python
issues = dss.check_file()
print(issues[-1])  # "File integrity OK" or description of problem
```

## Catalog with Wildcard Filtering

```rust
// All records
let all = dss.catalog()?;

// Filter by parameter
let flow_records = dss.catalog_filtered(Some("/*/FLOW///*/"))?;

// Filter by location and parameter
let specific = dss.catalog_filtered(Some("/BASIN/GAGE1/FLOW///*/"))?;
```

**Python:**
```python
all_records = dss.catalog()
flow_records = dss.catalog(filter="/*/*/FLOW///*/")
```

Wildcard `*` matches any string in a pathname part. Empty filter parts match empty pathname parts.

## Query Record Type

```rust
let rtype = dss.record_type("/BASIN/LOC/FLOW/01JAN2020/1HOUR/OBS/")?;
match rtype {
    100..=119 => println!("Time series"),
    200..=209 => println!("Paired data"),
    300 => println!("Text"),
    0 => println!("Not found"),
    _ => println!("Type {rtype}"),
}
```
