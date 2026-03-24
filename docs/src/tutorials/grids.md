# Grid / Spatial Data

Grid records store 2D spatial data such as precipitation, temperature, or elevation grids. Data is compressed with zlib for efficient storage.

## Writing

```rust
// Create a 100x50 grid
let nx = 100;
let ny = 50;
let data: Vec<f32> = (0..nx*ny).map(|i| (i as f32) * 0.1).collect();

dss.write_grid(
    "/SHG/BASIN/PRECIP/01JAN2020:0600/01JAN2020:1200/NEXRAD/",
    430,          // grid type (430 = SHG)
    nx as i32,
    ny as i32,
    &data,
    "MM",         // data units
    2000.0,       // cell size
)?;
```

**Python:**
```python
import numpy as np
data = np.arange(5000, dtype=np.float32).reshape(50, 100) * 0.1
dss.write_grid("/SHG/BASIN/PRECIP/01JAN2020:0600/01JAN2020:1200/NEXRAD/",
               430, 100, 50, data.flatten().tolist(), "MM", 2000.0)
```

## Reading

```rust
if let Some(grid) = dss.read_grid("/SHG/BASIN/PRECIP/01JAN2020:0600/01JAN2020:1200/NEXRAD/")? {
    println!("{}x{} grid, cell_size={}", grid.nx, grid.ny, grid.cell_size);
    println!("Units: {}", grid.data_units);
    println!("Data points: {}", grid.data.len());
}
```

**Python:**
```python
result = dss.read_grid("/SHG/BASIN/PRECIP/01JAN2020:0600/01JAN2020:1200/NEXRAD/")
if result is not None:
    print(f"{result['nx']}x{result['ny']} grid")
    data = np.array(result['data']).reshape(result['ny'], result['nx'])
```

## Grid Types

| Code | Name | Description |
|------|------|-------------|
| 400 | Undefined (time) | Undefined projection with time stamp |
| 410 | HRAP (time) | Hydrologic Rainfall Analysis Project |
| 420 | Albers (time) | Albers Equal Area Conic |
| 430 | SHG (time) | Standard Hydrologic Grid |

## Compression

Grid data is automatically compressed with zlib on write and decompressed on read. No configuration needed.
