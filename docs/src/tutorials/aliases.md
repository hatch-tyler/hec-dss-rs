# Aliases

Aliases allow multiple pathnames to reference the same underlying record data. This is useful when a record needs to be accessible under different naming conventions.

## Adding an Alias

```rust
// First, write a primary record
dss.write_ts("/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/OBS/",
    &[100.0, 200.0, 300.0], "CFS", "INST-VAL")?;

// Create an alias that points to the same data
dss.alias_add(
    "/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/OBS/",     // primary pathname
    "/ALTERNATE/NAME/FLOW/01JAN2020/1HOUR/ALIAS/",  // alias pathname
)?;
```

**Python:**
```python
dss.alias_add("/BASIN/GAGE1/FLOW/01JAN2020/1HOUR/OBS/",
              "/ALTERNATE/NAME/FLOW/01JAN2020/1HOUR/ALIAS/")
```

Both pathnames now appear in the catalog and reference the same data.

## Listing Aliases

```rust
let aliases = dss.alias_list()?;
for (alias_path, info_addr) in &aliases {
    println!("Alias: {alias_path} -> info at {info_addr}");
}
```

## Removing an Alias

```rust
dss.alias_remove("/ALTERNATE/NAME/FLOW/01JAN2020/1HOUR/ALIAS/")?;
```

Removing an alias does not delete the primary record.
