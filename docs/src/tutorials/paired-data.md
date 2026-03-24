# Paired Data

Paired data stores X-Y relationships such as frequency-flow curves, stage-discharge ratings, and elevation-area-volume tables.

## Writing

```rust
// Single curve
dss.write_pd(
    "/BASIN/DAM/FREQ-FLOW///ANALYTICAL/",
    &[1.0, 5.0, 10.0, 50.0, 100.0],       // ordinates (X)
    &[500.0, 1000.0, 2000.0, 5000.0, 10000.0], // values (Y)
    1,           // number of curves
    "PERCENT",   // independent variable units
    "CFS",       // dependent variable units
    None,        // optional curve labels
)?;

// Multiple curves with labels
let ordinates = &[1.0, 10.0, 100.0];
let values = &[
    100.0, 200.0, 300.0,   // curve 1 (3 ordinates)
    150.0, 250.0, 350.0,   // curve 2 (3 ordinates)
];
dss.write_pd(
    "/BASIN/DAM/FREQ-FLOW///MULTI/",
    ordinates, values, 2,
    "PERCENT", "CFS",
    Some(&["Low Estimate", "High Estimate"]),
)?;
```

**Python:**
```python
dss.write_pd("/BASIN/DAM/FREQ-FLOW///ANALYTICAL/",
             np.array([1.0, 5.0, 10.0, 50.0, 100.0]),
             np.array([500.0, 1000.0, 2000.0, 5000.0, 10000.0]),
             1, "PERCENT", "CFS")
```

## Reading

```rust
if let Some(pd) = dss.read_pd("/BASIN/DAM/FREQ-FLOW///ANALYTICAL/")? {
    println!("Ordinates: {:?}", pd.ordinates);
    println!("Values: {:?}", pd.values);
    println!("{} ordinates, {} curves", pd.number_ordinates, pd.number_curves);
    println!("Units: {} vs {}", pd.units_independent, pd.units_dependent);
}
```

**Python:**
```python
result = dss.read_pd("/BASIN/DAM/FREQ-FLOW///ANALYTICAL/")
if result is not None:
    ordinates, values = result
    print(f"Ordinates: {ordinates}")
    print(f"Values: {values}")
```

## Query Without Data

```rust
if let Some((n_ord, n_curves, units_i, units_d)) =
    dss.pd_retrieve_info("/BASIN/DAM/FREQ-FLOW///ANALYTICAL/")?
{
    println!("{n_ord} ordinates, {n_curves} curves");
    println!("Units: {units_i} vs {units_d}");
}
```
