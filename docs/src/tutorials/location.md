# Location Data

Location records store geographic coordinates, datum information, and supplemental metadata associated with a DSS record.

## Writing

```rust
use dss_core::{NativeDssFile, LocationRecord};

let loc = LocationRecord {
    x: -121.5,               // longitude or easting
    y: 38.5,                 // latitude or northing
    z: 100.0,                // elevation
    coordinate_system: 2,     // 2 = lat/lon
    horizontal_datum: 1,      // 1 = NAD83
    vertical_datum: 2,        // 2 = NAVD88
    timezone: "PST".to_string(),
    supplemental: "USGS gage 11446500".to_string(),
    ..Default::default()
};
dss.write_location("/BASIN/GAGE1/LOC///INFO/", &loc)?;
```

**Python:**
```python
dss.write_location("/BASIN/GAGE1/LOC///INFO/",
                   x=-121.5, y=38.5, z=100.0,
                   coordinate_system=2, horizontal_datum=1,
                   timezone="PST", supplemental="USGS gage 11446500")
```

## Reading

```rust
if let Some(loc) = dss.read_location("/BASIN/GAGE1/LOC///INFO/")? {
    println!("({}, {}) elev={}", loc.x, loc.y, loc.z);
    println!("Datum: H={}, V={}", loc.horizontal_datum, loc.vertical_datum);
    println!("Info: {}", loc.supplemental);
}
```

**Python:**
```python
loc = dss.read_location("/BASIN/GAGE1/LOC///INFO/")
if loc is not None:
    print(f"({loc['x']}, {loc['y']}) elev={loc['z']}")
```
